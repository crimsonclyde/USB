//! Conservative destination naming policy; no filesystem drivers are involved.
use crate::catalogue::Game;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const FAT32_MAX_FILE_BYTES: u64 = u32::MAX as u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Filesystem {
    Fat32,
    Exfat,
    Ext4,
}

fn component_ok(part: &str, fs: Filesystem) -> bool {
    if part.is_empty()
        || part == "."
        || part == ".."
        || part.contains(['/', '\\'])
        || part.chars().any(char::is_control)
    {
        return false;
    }
    if fs == Filesystem::Ext4 {
        return part.len() <= 255;
    }
    // ASCII-only on case-insensitive targets avoids guessing Unicode upcase tables.
    if !part.is_ascii()
        || part.len() > 255
        || part.ends_with(['.', ' '])
        || part.contains(['<', '>', ':', '"', '|', '?', '*'])
    {
        return false;
    }
    let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

/// Returns the first stable compatibility rejection, including generated paths.
pub fn incompatibility(game: &Game, fs: Filesystem) -> Option<String> {
    if game
        .metadata
        .filesystems
        .as_ref()
        .is_some_and(|allowed| !allowed.contains(&fs))
    {
        return Some("target excluded by game metadata".into());
    }
    let mut names = BTreeSet::new();
    let root = format!("USB/games/{}", game.metadata.id);
    let entries = std::iter::once("")
        .chain(game.directories.iter().map(String::as_str))
        .chain(game.files.iter().map(|f| f.path.as_str()));
    for path in entries {
        let destination = if path.is_empty() {
            root.clone()
        } else {
            format!("{root}/{path}")
        };
        let max_path = if fs == Filesystem::Ext4 { 4095 } else { 240 };
        if destination.len() > max_path
            || !destination.split('/').all(|part| component_ok(part, fs))
        {
            return Some(format!("incompatible destination path: {destination:?}"));
        }
        let key = if fs == Filesystem::Ext4 {
            destination
        } else {
            destination.to_ascii_lowercase()
        };
        if !names.insert(key) {
            return Some(format!("destination name collision: {path:?}"));
        }
    }
    if fs == Filesystem::Fat32
        && let Some(file) = game.files.iter().find(|f| f.bytes > FAT32_MAX_FILE_BYTES)
    {
        return Some(format!(
            "FAT32 individual file limit exceeded: {:?}",
            file.path
        ));
    }
    None
}
