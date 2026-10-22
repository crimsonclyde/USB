//! Read-only Linux device inventory. Assessments identify candidates for operator
//! review, never permission to mount, write, repair or authorise safe removal.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    process::{Command, Stdio},
};
use usb_core::compatibility::Filesystem;

pub const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;
const MAX_DEVICES: usize = 1024;
pub const COLUMNS: &str = "KNAME,PATH,TYPE,MAJ:MIN,RO,TRAN,FSTYPE,FSVER,MOUNTPOINTS,SIZE,PKNAME";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("device inventory I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid device inventory JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub kname: String,
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "maj:min")]
    pub device_number: String,
    pub ro: Option<bool>,
    pub tran: Option<String>,
    pub fstype: Option<String>,
    pub fsver: Option<String>,
    pub mountpoints: Vec<Option<String>>,
    pub size: Option<u64>,
    pub pkname: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    blockdevices: Vec<Block>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    UnsupportedLayout,
    NotUsb,
    ReadOnlyOrUnknown,
    MountedDisk,
    EmptyOrUnknownSize,
    UnsupportedFilesystem,
}

#[derive(Debug, Serialize)]
pub struct Device {
    pub block: Block,
    pub filesystem: Option<Filesystem>,
    pub candidate_for_review: bool,
    pub rejections: Vec<Rejection>,
}

#[derive(Debug, Serialize)]
pub struct Inventory {
    /// True only describes diagnostic provenance; neither source grants permission.
    pub live_snapshot: bool,
    pub devices: Vec<Device>,
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn kernel_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn root<'a>(block: &'a Block, blocks: &'a BTreeMap<String, Block>) -> Result<&'a Block> {
    let mut node = block;
    let mut seen = BTreeSet::new();
    while let Some(parent) = &node.pkname {
        if !seen.insert(&node.kname) {
            return Err(invalid("cycle in block-device parents"));
        }
        node = blocks
            .get(parent)
            .ok_or_else(|| invalid("unknown block-device parent"))?;
    }
    Ok(node)
}

/// Parse an explicit-column lsblk snapshot. Unknown/duplicate metadata, aliases,
/// cycles, malformed device paths and oversized inventories fail closed. Output
/// is stable kernel-name order. Snapshot data cannot authorise device operations.
pub fn parse_snapshot(bytes: &[u8]) -> Result<Inventory> {
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(invalid("device snapshot exceeds 1 MiB"));
    }
    let snapshot: Snapshot = serde_json::from_slice(bytes)?;
    if snapshot.blockdevices.len() > MAX_DEVICES {
        return Err(invalid("device snapshot exceeds 1024 entries"));
    }
    let mut blocks = BTreeMap::new();
    let mut numbers = BTreeSet::new();
    for block in snapshot.blockdevices {
        let number = block
            .device_number
            .split_once(':')
            .and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)));
        // Device-mapper aliases are retained for diagnostics only. They can
        // never satisfy the simple disk/partition candidate layout below.
        let known_path = block.path == format!("/dev/{}", block.kname)
            || (!matches!(block.kind.as_str(), "disk" | "part")
                && block
                    .path
                    .strip_prefix("/dev/mapper/")
                    .is_some_and(kernel_name));
        if !kernel_name(&block.kname)
            || !known_path
            || number.is_none_or(|(a, b)| format!("{a}:{b}") != block.device_number)
            || !numbers.insert(block.device_number.clone())
            || block.pkname.as_ref().is_some_and(|p| !kernel_name(p))
            || block
                .mountpoints
                .iter()
                .flatten()
                .any(|p| !p.starts_with('/'))
        {
            return Err(invalid(
                "invalid or duplicate block-device identity/mount metadata",
            ));
        }
        if blocks.insert(block.kname.clone(), block).is_some() {
            return Err(invalid("duplicate kernel device name"));
        }
    }
    let mut roots = BTreeMap::new();
    let mut mounted = BTreeSet::new();
    for block in blocks.values() {
        let disk = root(block, &blocks)?;
        roots.insert(block.kname.clone(), disk.kname.clone());
        if block.mountpoints.iter().any(Option::is_some) {
            mounted.insert(disk.kname.clone());
        }
    }
    let mut devices = Vec::new();
    for block in blocks.values() {
        let disk = root(block, &blocks)?;
        let descendants: Vec<_> = blocks
            .values()
            .filter(|b| roots[&b.kname] == disk.kname && b.kname != disk.kname)
            .collect();
        // Initially support only a whole-disk filesystem or one direct partition.
        // RAID, LVM, dm-crypt, multi-parent and multiple partitions need later policy.
        let layout = disk.kind == "disk"
            && disk.pkname.is_none()
            && ((block.kname == disk.kname && descendants.is_empty())
                || (block.kind == "part"
                    && block.pkname.as_deref() == Some(&disk.kname)
                    && descendants.len() == 1
                    && disk.fstype.is_none()));
        let filesystem = match (block.fstype.as_deref(), block.fsver.as_deref()) {
            (Some("vfat"), Some("FAT32")) => Some(Filesystem::Fat32),
            (Some("exfat"), _) => Some(Filesystem::Exfat),
            (Some("ext4"), _) => Some(Filesystem::Ext4),
            _ => None,
        };
        let mut rejections = Vec::new();
        if !layout {
            rejections.push(Rejection::UnsupportedLayout);
        }
        if disk.tran.as_deref() != Some("usb") {
            rejections.push(Rejection::NotUsb);
        }
        if block.ro != Some(false) || disk.ro != Some(false) {
            rejections.push(Rejection::ReadOnlyOrUnknown);
        }
        if mounted.contains(&disk.kname) {
            rejections.push(Rejection::MountedDisk);
        }
        if block.size.is_none_or(|n| n == 0)
            || disk.size.is_none_or(|n| n == 0)
            || block.size > disk.size
        {
            rejections.push(Rejection::EmptyOrUnknownSize);
        }
        if filesystem.is_none() {
            rejections.push(Rejection::UnsupportedFilesystem);
        }
        devices.push(Device {
            block: block.clone(),
            filesystem,
            candidate_for_review: rejections.is_empty(),
            rejections,
        });
    }
    Ok(Inventory {
        live_snapshot: false,
        devices,
    })
}

/// Collect metadata using sysfs/udev only. Explicitly disable raw-device probing
/// by selecting udev as the sole property source. Unsupported lsblk versions fail
/// without a fallback. This does not settle udev, watch hotplug, map ports or mount.
pub fn inventory() -> Result<Inventory> {
    let mut command = Command::new("/usr/bin/lsblk");
    command
        .args([
            "--json",
            "--list",
            "--bytes",
            "--output",
            COLUMNS,
            "--properties-by",
            "udev",
        ])
        .env("LC_ALL", "C");
    collect(command)
}

fn collect(mut command: Command) -> Result<Inventory> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .ok_or_else(|| invalid("missing lsblk stdout"))?
        .take(MAX_SNAPSHOT_BYTES as u64 + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > MAX_SNAPSHOT_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        read?;
        return Err(invalid("device snapshot exceeds 1 MiB"));
    }
    if !child.wait()?.success() {
        return Err(invalid(
            "lsblk failed; explicit columns and --properties-by udev support are required",
        ));
    }
    let mut inventory = parse_snapshot(&bytes)?;
    inventory.live_snapshot = true;
    Ok(inventory)
}

#[cfg(test)]
mod tests;
