# The portals — read-only device inventory

Phase III has a read-only inventory foundation. It is not an automatic device
manager: there is no hotplug watcher, port calibration, mounting, filesystem health
check, privileged service, unmounting or safe-removal authorisation.

Inspect the generated example without accessing host device metadata:

```sh
sh scripts/cargo-local.sh run -p usb-cli -- devices --snapshot examples/devices.json --json
```

`usb devices [--json]` instead collects a live metadata snapshot with the system's
`/usr/bin/lsblk`. Collection requires support for these explicit columns and options:

```text
lsblk --json --list --bytes \
  --output KNAME,PATH,TYPE,MAJ:MIN,RO,TRAN,FSTYPE,FSVER,MOUNTPOINTS,SIZE,PKNAME \
  --properties-by udev
```

The [util-linux lsblk manual](https://man7.org/linux/man-pages/man8/lsblk.8.html)
documents explicit-column/list output and property sources. Selecting only `udev`
prevents fallback to raw filesystem probing. Older or incompatible utilities fail
without a weaker fallback. The collector does not run `udevadm settle` or read
visitor file contents. Newly connected devices may have incomplete metadata and
are rejected until a later observation provides the necessary facts.

## Diagnostic schema and assessment

`usb-device` owns collection and assessment, keeping hardware concerns outside
`usb-core` and Linux directory transfer operations in `usb-transfer`. Tests use
generated JSON and harmless fixture-output helpers; they do not enumerate host
drives or run the system lsblk collector. Helper tests cover missing commands,
nonzero exits, oversized output, bounded reads and diagnostic provenance.

Input is the strict, flat explicit-column lsblk JSON schema shown in the example.
Snapshots are bounded to 1 MiB and 1024 entries. Unknown fields, duplicate kernel
names or major/minor identities, path/name mismatches, malformed numbers, unknown
parents and parent cycles reject the whole snapshot. Kernel names are limited to
ASCII letters, digits, underscores and hyphens; candidate paths must be
`/dev/<kname>`. Unsupported mapper nodes may retain `/dev/mapper/<name>` aliases
for diagnostics without becoming candidates or following those aliases.
Missing nullable facts do not establish eligibility. Output is stable kernel-name
order. `live_snapshot` records whether collection happened live, not whether the
facts are trustworthy or still current; user-supplied snapshots always set it false.

A **candidate for review** must have all of the following observed facts:

- A USB-transport parent disk with no parent of its own.
- A whole-disk filesystem with no descendants, or a single direct partition with
  no sibling/dependent devices and no filesystem on the parent disk.
- Known writable disk and payload device, nonzero sizes and a payload no larger
  than the parent disk.
- No reported mountpoint anywhere in that disk's tree, including OS or visitor mounts.
- Recognised ext4 or exFAT, or `vfat` explicitly reporting `FSVER: FAT32`. FAT12,
  FAT16, unknown and unsupported filesystems are rejected.

Multiple partitions, device-mapper/RAID/LVM layouts, missing parent metadata,
read-only or unknown status and mounted disks are refused conservatively. Removable
flags, kernel enumeration order and filenames never establish trust or approval.

This assessment cannot authorise a mount or write. Snapshots may be stale or forged.
It does not establish physical identity, filesystem health, clean state, exclusivity,
correct mount options, safe space or redistribution rights. Device kernel names and
major/minor numbers are observations, not persistent stick IDs or ownership tokens.
The existing directory-copy commands remain explicit directory prototypes and do
not consume this inventory as permission.

## Remaining device work

The next device work needs an explicit station port map and revalidation protocol:

1. Calibrate physical hub paths on the intended station and configure unique port
   selectors. Never assign port numbers from discovery order or treat missing or
   ambiguous mappings as permission to proceed.
2. Bind observations to current block-device identity and topology, excluding the
   OS/archive and any already-mounted or shared disk. Recheck after every device
   event and immediately before privileged operations; reject replacement/races.
3. Implement a narrowly scoped UDisks2 adapter and disposable-environment tests for
   fresh private mounts with `nodev,nosuid,noexec`, supported filesystems and no
   repair/format/repartition fallback. Validate the returned mount identity and
   options before invoking the directory engine.
4. Finalise local writes and explicitly unmount the same validated device. An
   unmount failure, device replacement or uncertain finalisation must block
   `RELEASE AUTHORISED`, even after every game copied successfully.
5. Add disconnect, stale-event, multi-port, failed-mount and failed-unmount fixtures,
   followed by separately provisioned disposable-filesystem power-loss validation.

These are outstanding implementation and validation tasks. No mounting/removal
behaviour is implied by the snapshot classifier or the ten-job directory benchmark.
