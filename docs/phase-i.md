# ⸸ Consulting the archive — Phase I

Phase I implements a read-only catalogue and distribution planner. These commands do not
access destination devices, detect free space, mount drives, or transfer files.
Supply **available free bytes**, not a drive's advertised capacity. Plans are
estimates for future transfers and confer no permission to overwrite anything.

## Build and verification

Install current stable Rust with rustfmt and Clippy. There are no native library,
USB, SQLite, runtime service or cross-compilation dependencies.

```sh
cargo build --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p usb-cli -- --help
```

Linux x86-64 and ARM64 are supported by the same code. CI builds, lints and tests
on native runners for both architectures. For a compile-only ARM64 check on x86:

```sh
rustup target add aarch64-unknown-linux-gnu
cargo check --workspace --target aarch64-unknown-linux-gnu --locked
```

An ARM64 executable built on x86 additionally needs an appropriate cross-linker;
building directly on the Raspberry Pi avoids that requirement. A stable toolchain
is required; no older MSRV is promised yet. Cargo.lock is committed for this CLI.
Dependency downloads occur during development, not during station operation.

To keep development downloads and generated test files inside this checkout, run
from the repository root before the commands above:

```sh
mkdir -p .dev/cargo-home .dev/tmp
export CARGO_HOME="$PWD/.dev/cargo-home"
export TMPDIR="$PWD/.dev/tmp"
```

The repository-local wrapper `sh scripts/cargo-local.sh` sets these paths, uses a
local toolchain under `.dev/rustup` when installed, and defaults to offline mode.
For example, `sh scripts/cargo-local.sh test --workspace`. Set
`CARGO_NET_OFFLINE=false` explicitly when downloading new development dependencies.
The entire `.dev/` directory is excluded by `.gitignore`; build outputs remain in
the ignored `target/` directory. After the dependencies have been downloaded, add
`--offline` to Cargo build/test commands to reuse the local cache. Rust itself must
still be installed; these settings do not themselves download a toolchain.

## Commands

Run from the repository root; after `cargo install --path crates/usb-cli --locked`,
replace `cargo run -p usb-cli --` with `usb`.

```sh
cargo run -p usb-cli -- catalogue scan examples/archive
cargo run -p usb-cli -- --archive examples/archive catalogue list
cargo run -p usb-cli -- --config examples/config.json plan --capacity 16G --filesystem exfat
cargo run -p usb-cli -- --archive examples/archive plan --capacity 64G --filesystem fat32
cargo run -p usb-cli -- --archive examples/archive plan --capacity 1MiB --filesystem ext4 --margin-bytes 0 --margin-percent 0 --allocation-unit 4KiB --json
```

`--archive`, `--config` and `--json` are global flags. `catalogue scan PATH`
overrides the configured archive for that invocation. `catalogue list` and `plan`
rescan the configured archive (default `/srv/game-archive`); scan saves no state.
Missing/invalid archives produce an error and a nonzero exit code. An empty valid
archive or a plan selecting no games succeeds and reports the result.

All capacity inputs are integers. `G`/`GiB` are binary (2^30), `GB` is decimal
(10^9); similarly K, M and T. Bare integers are bytes. Negative, fractional,
unknown-unit and overflowing inputs are errors. Filesystems: fat32, exfat, ext4.
Use `--json` for source paths, per-file sizes, metadata and machine-readable plans.
Human output escapes names and paths to prevent terminal control sequences.

Configuration is strict JSON, with optional `archive` and `planner` fields; see
[the runnable example](../examples/config.json). CLI flags override configuration.
Relative archive paths resolve from the process's working directory. Unknown keys,
invalid units and priority overrides referring to unknown IDs fail closed.

## Archive schema (version 1)

```text
/srv/game-archive/              # any local directory works
  archive.json
  computers/                   # grouping directories have no special meaning
    my-game/
      game.json
      content/                 # entire tree is one atomic game
        game.bin
        manual.txt
        extras/
          readme.txt
```

`archive.json` declares named platforms and collections:

```json
{
  "schema_version": 1,
  "platforms": [{"id": "dos", "name": "MS-DOS", "priority": 0}],
  "collections": [{"id": "ancient", "name": "Ancient Machines", "priority": 0}]
}
```

Each `game.json` declares exactly one game:

```json
{
  "schema_version": 1,
  "id": "my-game",
  "name": "A Curated Game",
  "platform": "dos",
  "collection": "ancient",
  "release_year": 1987,
  "priority": 0,
  "redistribution_approved": false,
  "approval_note": null,
  "filesystems": ["fat32", "exfat", "ext4"]
}
```

IDs are curator-assigned, stable across archive moves: 1–64 lowercase ASCII
letters, digits, underscores or hyphens. IDs are globally unique within their
kind; each game references one platform and one collection. Additional platforms
and collections require metadata only. Platform and collection IDs may overlap.
Group/game names must be nonempty. Years are positive u16 or null/absent (unknown).
Schema version must be 1; unknown fields are rejected to catch approval typos.

Approval defaults to **false** when omitted. Approval true requires a nonempty
`approval_note` recording the curator's licence/source evidence. This is an audit
record, not automatic verification of legal rights. Include required licence text
and other redistribution conditions **inside content/**. Game manifests and archive
metadata are not payload. Omitted filesystem metadata permits all supported targets;
an empty list permits none. Actual filename/file-size checks still apply.

Indexer finds manifests recursively and stops discovery at each game boundary.
Only content/ is indexed inside a game; nested game.json files inside content are
ordinary payload. Files outside games are not distribution units and are ignored.
Empty content trees, malformed manifests, unknown references and duplicate IDs
reject the scan. Symlinks and special files encountered during discovery or payload
walking are rejected, including dangling links. Payload must have UTF-8 paths;
empty payload directories are retained. Canonical source root, content directory,
relative filenames, file sizes and total game sizes are recorded. Metadata is read;
file contents are not read or executed except for JSON manifests.

Keep the archive curator-controlled and stable during scanning. No race-resistant
file-handle confinement or content hashes exist yet; this scanner is not a security
boundary for concurrently hostile archive mutation. Parent paths of the explicitly
supplied root may contain symlinks, while the root itself and walked entries may not.

## Selection strategy and capacity accounting

1. Validate options and catalogue references. For each game, effective priority is
   game + platform + collection priority (i64 sum of i32 values). Config maps replace
   individual manifest values, rather than adding another layer.
2. Sort by effective priority descending, release year ascending (unknown last),
   total payload bytes ascending, then stable game ID ascending.
3. Visit each game once. Reject unapproved content, then incompatible content.
4. Estimate the entire game. Select it only if the complete estimate fits; otherwise
   record a capacity skip and continue. No partial games or early stop at a large game.

This greedy policy is O(n log n) plus path validation and indexing costs, with O(n)
selection storage. It favours curated priorities and age over a globally optimal
fill. Smaller games win within a priority/year group; newer small games can use
space left by older large games. It neither backtracks nor solves knapsack. It may
leave usable space that another combination could fill. Every game appears exactly
once as selected or skipped; skip reasons and order are deterministic.

Safe capacity is available space minus the **larger** of an absolute reserve
(default 64 MiB) and a rounded-up percentage (default 2%). Reserve is clamped to
available bytes. No plan intentionally exceeds safe capacity. Arithmetic is checked;
unrepresentable sizes fail rather than wrap.

Estimated per-game allocation includes each file rounded up to the configured
allocation unit, one extra unit per file for metadata, and one unit per directory,
including the game's root. Empty files consume a metadata unit. An additional two
units for shared `USB/` and `USB/games/` directories are charged once when the first
game is selected. The destination layout is `USB/games/<stable-id>/<payload-path>`.
Selected entries' estimated bytes exclude the shared two units; the plan total
includes them. Remaining safe bytes exclude the reserve; remaining available bytes
include it. Payload bytes exclude all estimates.

The default allocation unit is 32 KiB. A configured value must be a power of two
from 512 bytes to 1 MiB and should be **at least the real destination cluster size**.
A capacity-only planner cannot detect this. Directory entry costs, FAT/exFAT tables,
ext4 metadata, fragmentation and existing directory state vary. These estimates
and reserve are conservative heuristics, not proof of actual disk allocation.
Future copying must measure free space again and handle ENOSPC safely.

## Filesystem compatibility policy

- FAT32 rejects any individual file exceeding 4,294,967,295 bytes. A game may be
  larger if each file is within this limit. No automatic splitting occurs.
- FAT32/exFAT enforce conservative portable names: ASCII only, no controls or
  Windows-forbidden characters, trailing dot/space, DOS device names or case-folded
  collisions. Total generated relative destination paths are capped at 240 bytes;
  components at 255. This intentionally rejects some names those filesystems could
  technically represent; there is no renaming or lossy Unicode normalization.
- ext4 permits UTF-8 names up to 255 bytes/component and 4095 bytes/relative path,
  with case-sensitive collisions. Controls, backslashes, empty/dot/parent path
  components are rejected on all targets. Absolute paths cannot be admitted.
- Directories, including empty ones, are checked alongside files. Metadata can
  impose additional filesystem restrictions. Unknown filesystem types are errors.

## Architecture and boundaries

- `usb-core::catalogue`: strict schema and read-only filesystem indexer.
- `usb-core::compatibility`: destination path and per-file constraints.
- `usb-core::planner`: pure selection; no filesystem I/O, hardware or USB APIs.
- `usb-cli`: argument/config parsing and readable/JSON presentation.

JSON manifests are the source of truth in Phase I. The README's planned SQLite
index, Tokio runtime and Axum API are deferred until persistent queries, parallel
transfers and an interface justify them. Tests generate temporary archives and
sparse synthetic large files, requiring no ROMs, downloads or removable devices.
The Unix special-file rejection test uses the standard `mkfifo` utility (coreutils).

## Limitations and Phase II

The Phase I catalogue and standalone planner have no SQLite cache, persistent
hashes or incremental distributions. The separate [Phase II directory-copy
foundation](phase-ii.md) adds runtime hashes, collision checks, free-space checks
and progress events. Mount management, a progress service and a dashboard remain
unimplemented.
All games and file metadata are held in memory. Very deep or extremely large
untrusted archives can exhaust resources; no adversarial resource quotas are claimed.
Plans do not bind source content and must not be reused after archive changes.
Redistribution rights are a human curator responsibility. Only filename and size
compatibility are assessed; executability and platform runtime availability are not.

The following Phase II foundation is now implemented for ordinary temporary
folders; see [its guide](phase-ii.md) for current boundaries and remaining work:

1. Revalidate immutable source handles, sizes, paths, rights and destination free
   space; reject source/destination overlap, symlinks and conflicting existing names.
2. Create a dedicated distribution tree with no overwrite, staging complete games
   and publishing them only after a verified copy; define interrupted-copy cleanup.
3. Handle short writes, ENOSPC, disconnects, cancellation and checksums. Preserve
   source data and unrelated destination files on every error path.
4. Add progress events and per-transfer state, with bounded concurrency designed for
   ten independent ports. Test fault isolation before adding hardware management.
5. Test only synthetic directory fixtures first. Real mounting and safe unmounting
   belong to Phase III; do not report safe removal until successful finalisation.
