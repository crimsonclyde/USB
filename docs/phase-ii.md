# ⸸ The transmission — Phase II directory-copy foundation

The current implementation is an experimental synchronous copy engine for
ordinary Linux directories. It transfers **whole catalogue games**, including all
payload files, licences and empty directories. It never intentionally exposes a
partly copied game as a completed offering.

This is a foundation for Phase II, not the USB station daemon. No mounting, device
selection, automatic filesystem-type detection, formatting, repair, execution,
unmounting or safe-removal authorisation is implemented. Admin controls, random
selection and returning-stick cooldowns remain [future goals](future-goals.md).

## Try it with the original demonstration archive

Build with current stable Rust, then use a newly created temporary destination:

```sh
cargo build --workspace --locked
usb_demo_destination=$(mktemp -d)
cargo run -p usb-cli -- --archive examples/archive copy-to-directory \
  --destination "$usb_demo_destination" --filesystem ext4
```

The command writes only under `$usb_demo_destination/USB`. Keep the directory to
inspect the result. It does not automatically remove or clean anything. A second
invocation targeting the same directory refuses the existing USB entry.

The available command is:

```text
usb [--archive PATH] [--config FILE] [--json] copy-to-directory
    --destination EXISTING_DIRECTORY --filesystem fat32|exfat|ext4
    [--capacity-limit BYTES] [--margin-bytes BYTES]
    [--margin-percent PERCENT] [--allocation-unit BYTES]
```

All Phase I configuration and capacity-unit rules still apply. `--capacity-limit`
is an optional upper bound; the engine always measures free space and never
substitutes a user-supplied budget for the real measurement. `--filesystem` selects
compatibility policy, not a detected or validated device filesystem. An ordinary
ext4 test directory can exercise FAT32 filename rules without mounting a drive.

Progress goes to stderr. With `--json`, stdout contains the final JSON report and
stderr contains JSON Lines events, throttling byte progress to roughly 1 MiB.
The report includes the plan, measured space, effective allocation unit, completed
games with SHA-256 checksums, failure classification and an unconfirmed game ID.
It also includes separate staging and journal allocation estimates.
The byte counter describes copied data, not confirmed delivery; completion events
occur only after verification, publication and successful directory syncing.

Ctrl-C, SIGTERM and SIGHUP request cooperative cancellation. The engine checks
between bounded chunks and operations; an in-progress kernel I/O call may still
block. Preparation errors and unsuccessful transfers exit nonzero. Runtime failure
or cancellation still prints a report; a preparation error produces an error on
stderr before destination writes. Selecting zero games succeeds without creating USB.

## Layout and delivery semantics

```text
destination/                       # must already exist
  personal-files...                # left alone; their contents are never read
  USB/                             # must not exist before this invocation
    transfer.jsonl                 # append-only diagnostic journal, schema 1
    .incoming/
      unfinished-game/             # staging, never a completed reward
    games/
      completed-game/              # complete, verified payload only
```

An existing USB entry is refused whether it is a directory, a file or a dangling
symlink. There is no merge, overwrite, automatic cleanup or resume. File creation
uses exclusive creation; game publication uses a kernel no-replace directory rename.
If another writer creates the final name, publication fails and preserves that entry.

Each game is a transaction boundary. A failure in game B leaves an already completed
game A intact. The engine stops that transfer rather than silently claiming the
whole plan succeeded. Other destinations have independent cancellation and state.
A nonblocking advisory lock on the destination directory serializes cooperating
Bureau transfers to that root without writing lock files.

Files in staging remain after cancellation, an error or a crash. This deliberate
policy avoids deleting anything based on a guessed ownership marker. The report
identifies an unconfirmed game. An operator can read the persisted diagnostic
records with `inspect-transfer` and choose a fresh destination for a retry.
Automatic identification and cleanup of stale Bureau-owned staging need a separate
design before resumable transfers.
Consumers must treat only `USB/games` as delivered content.

A rename may succeed before a subsequent directory sync fails. In that case a
complete game can already be visible under `games`, but it remains unconfirmed in
the report. The engine does not roll it back or report it delivered. Successful
local file/directory syncing does not authorise removing a physical device.

## Persistent records and operator recovery

```sh
cargo run -p usb-cli -- inspect-transfer --destination "$usb_demo_destination" --json
```

This command locks the destination against cooperating transfers and reads only
`USB/transfer.jsonl`, without an archive scan or payload reads. It never modifies,
cleans or resumes the existing tree. Keep failed staging intact. To retry, choose
another fresh temporary destination and run `copy-to-directory` again; source
approval, hashes, capacity and collisions are checked afresh. Do not rename or
delete an existing visitor's USB directory to make a retry fit.

Journal schema 1 is strict JSON Lines. Each line, including its newline, is at
most 512 bytes. Records are appended with exclusive initial file creation and a
file sync after every record; the journal's parent is synced before game writes.
The allowed sequence is:

1. `prepared` with `schema_version: 1` and a positive `selected_games` count.
2. For each game, `started`, `publishing`, then `committed`, each carrying the same
   stable `id`. IDs cannot repeat or exceed the selected count.
3. `finished` with status `completed`, `cancelled` or `failed`. Completion requires
   every selected game to have a commit. Failure/cancellation can leave an active
   game. No records may follow a terminal record.

`started` is synced before staging creation. `publishing` is synced after payload
verification and before the no-replace rename. `committed` is synced after the
rename and all required directory syncs, before adding the game to the report or
emitting `GameCompleted`. `GamePublished` describes the intermediate state before
the journal commit and must not be treated as delivery confirmation. A journal
write/sync failure stops the transfer; no further records are appended to that
failed stream. Previously confirmed games remain in the report.

Inspection exposes `recorded_completed`, `active_game`, `publication_uncertain`
and `terminal_status`. A missing terminal record means **interrupted or unknown**,
even when all commits are present. An unmatched publication intent means the game
may still be staged or may already be published; inspection never promotes it to
recorded completion. Missing journals (including trees from older builds), unknown
versions/fields, invalid transitions and torn/truncated records fail closed and
require operator inspection. No automatic repair or prefix salvage is performed.

These are historical, unauthenticated diagnostic claims. They do not prove current
payload integrity, current approval or ownership, and grant no cleanup, overwrite
or safe-removal permission. A sync failure can leave bytes visible even though the
caller could not confirm durability. For example, a failed commit sync can leave
`recorded_completed` containing a game that the live report keeps unconfirmed.
A failed terminal sync can leave a readable `completed` terminal record while the
live report says `failed`. Reading those records cannot establish that their syncs
succeeded; inspection must never be used as delivery or reward authorisation.
See [operator recovery policy](recovery.md) for supported actions and proposed
future work. Hardware power-loss guarantees still need
disposable-filesystem validation; process-exit tests do not simulate a power cut.

## Source validation and verification

The engine builds a fresh catalogue internally; it does not execute serialized or
stale plans supplied by callers. Before destination writes it captures selected
source trees, sizes, device/inode identities, modes, modification/change timestamps
and SHA-256 hashes, plus the relevant manifest identities and hashes.

Game metadata is compared with the freshly indexed metadata. The archive manifest,
selected game's manifest and complete source tree are revalidated before copying
and before publication. Same-size file replacements, changed approval metadata,
added/missing files, and encountered symlinks or special files fail closed.

During transfer, source bytes are hashed again and compared with the snapshot.
All copied files are synced, reopened from staging and hashed for read-back
verification. The staged tree is checked for missing, replaced or additional entries
before publication. Directories are synced from deepest children upward, then the
verified game is renamed without replacement and the relevant parents are synced.
Only then is the game added to the completed report.

Files receive mode 0644 plus the source's ordinary executable bits; setuid, setgid,
sticky and special permission bits are not preserved. Ownership, timestamps, ACLs,
extended attributes, hard-link relationships and sparse extents are not copied.
Hard-linked source files are copied as independent files. Manifest files themselves
are not payload; required content licences must live under each game's `content/`.

The archive must remain curator-controlled and stable. Initial catalogue indexing
still has the Phase I race/resource limitations. Handle confinement, identities and
hashes add defensive checks; they are not an isolation boundary against a hostile
process with the same user privileges mutating source or destination concurrently.
Use a read-only archive and exclusive destination access when integrating the daemon.

## Capacity and filesystem handling

The engine uses `fstatvfs` available blocks for the caller's free-space measurement.
Its allocation unit is the larger of the configured estimate and the reported
block/fragment sizes; unsupported units fail validation. One additional unit for
`.incoming` plus a journal estimate are deducted before planning. The journal
estimate reserves three maximum-sized records per catalogue game plus header and
terminal records, rounded to allocation units, plus one unit for file metadata.
Using the complete catalogue count avoids a circular dependency on selection and
can conservatively reduce capacity for large archives. Percentage reserves are
calculated before these deductions, preserving the configured safety margin.
The report exposes `staging_overhead_bytes` and `journal_overhead_bytes` separately
from the embedded planner budget. A zero-game plan still writes nothing.

Free space is checked again before directory creation, each game, each write chunk,
and publication. Size arithmetic is checked. Short and interrupted writes are
handled by `write_all`; I/O errors, failed syncs, disappearing paths and ENOSPC stop
the transfer without publishing the current partial game.

These remain allocation estimates, not reserved disk extents. Concurrent external
writes, fragmentation and filesystem metadata can still consume space or fail I/O.
No drive-full or disconnect experiments are performed against actual media.

## Architecture and supported environment

- `usb-core` remains independent of transfer I/O, operating-system USB APIs and CLI.
  It exposes reusable budget validation in addition to catalogue and planning.
- `usb-transfer` owns Linux directory I/O, source snapshots, bounded streaming,
  integrity checks, progress/cancellation, per-transfer reports and the bounded
  directory-job scheduler described in [scheduling](scheduling.md).
- `usb-cli` adds configuration, signal handling and human/JSON presentation.

The implementation uses the safe APIs of
[rustix](https://docs.rs/rustix/latest/rustix/fs/index.html),
[RustCrypto SHA-2](https://docs.rs/sha2/latest/sha2/) and
[ctrlc](https://docs.rs/ctrlc/latest/ctrlc/). Project crates continue to forbid unsafe
Rust. There are no new native-library dependencies, async services or custom drivers.

Linux x86-64 and ARM64 use the same implementation. Transfers require kernel
`openat2` support, no-replace directory renames, advisory directory locks and
successful file/directory syncing. Missing support produces errors without a weaker
fallback. Child traversal refuses symlinks, magic links and nested mount crossings.
The full supplied root paths must be symlink-free, which is stricter than Phase I
read-only scanning. Overlapping roots and destination aliases encountered within
the archive directory tree are rejected. Directory walks keep only a bounded
number of open handles, while catalogue and snapshot metadata remain in memory.

## Validation and remaining work

Run the workspace formatting, Clippy and unit/integration checks listed in
[Phase I](phase-i.md#build-and-verification). Generated temporary fixtures cover
whole-game copying, licence files, executable-bit handling, existing personal data,
empty budgets, eligibility, symlinks, overlap, cancellation, source/approval changes,
staged corruption, no-replace races, capacity loss, short/interrupted writes,
ENOSPC, read failures and isolation between simultaneous transfers. CLI tests cover
JSON reports, progress events and refusal to repeat a write to an existing USB tree.
Journal tests cover strict schemas, transition errors, truncated and oversized
records, ID and integer boundaries, lock refusal and persisted runtime failures.
Subprocess tests exit abruptly at staging, mid-copy, verification, publication
and commit boundaries, then inspect and retry into fresh temporary directories.
Test-only fault injection sweeps every journal write, journal sync, payload sync
and directory sync in a successful three-game transfer. Journal writes inject EIO
and ENOSPC both before writing and after leaving a real partial record. Sync faults
return EIO before the targeted sync syscall; other filesystem operations remain
real. Focused tests perform a real rename and fail each subsequent parent sync,
check that prior games survive, and retain the unconfirmed published game without
counting it as delivered. Commit and terminal sync failures also exercise visible
but unconfirmed records. Hooks are scoped to a test thread and compiled out of
normal builds; they do not emulate hardware power loss or filesystem behaviour.
No game ROMs or real removable devices are used.

Before treating Phase II as production-ready:

1. Extend diagnostic records and fresh-destination retry into an ownership-aware
   recovery policy only after defining safe recognition and explicit operator
   actions; see [the recovery policy](recovery.md). Add disposable-filesystem
   power-loss tests; automatic cleanup and in-place resume remain unimplemented.
2. Exercise real filesystem behaviour, sync failures and unplug scenarios in an
   isolated disposable-device test environment after Phase III validation exists.
3. Validate [the bounded directory scheduler](scheduling.md) and resource budgets
   on the intended station, powered hub and large real archive. A local generated
   benchmark is implemented and measured; physical port jobs and daemon management
   still depend on [Phase III](phase-iii.md).
4. Design safe existing-Bureau-content recognition and incremental visits before
   relaxing the refusal to reuse USB directories. Keep unrelated files untouched.
5. Integrate device validation, physical port mapping and successful finalisation/
   unmounting in Phase III before showing any safe-removal status.
