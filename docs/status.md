# Local development handoff — 2026-10-22

**Status: bounded directory scheduling, resource measurements, ARM64 compile checks
and read-only device inventory implemented. The full application remains unfinished.**

The user asked to run through the remaining tasks without requesting approval at
individual development steps. Continue useful implementation autonomously. The
operator has now confirmed a prepared Raspberry Pi 5 and an external disk for later
repository/archive storage. An older hub exists but must be located/identified;
disposable hardware tests are deferred until the Pi runs. See the dated decisions
in [future goals](future-goals.md): fresh bureaucratically named case folders,
Bureau-issued identity fallback, minimal local MySQL history, weighted surprise
selection, remembered technical ZONKs with retry, and playful cooldowns disabled.
Folder names and SSH/Codex development access are proposals, not installed features.
These are documentation decisions; current copy commands and device inventory have
not gained those capabilities.

## Implemented

- Phase I: strict manifest-led catalogue, deterministic pure planner and read-only CLI.
- Directory-copy engine: source/approval revalidation, whole-game staging, SHA-256
  verification, no-replace publication, space checks, cancellation and failure reports.
- Strict synced transfer journal, read-only inspection and explicit retain/inspect/
  fresh-destination retry [recovery policy](recovery.md).
- Fault injection and crash tests: torn records, EIO/ENOSPC, payload/directory sync
  errors, successful rename followed by failed sync, visible but unconfirmed records,
  terminal failures and independent transfers. Normal builds omit fault hooks.
- Bounded synchronous scheduler: at most ten jobs/workers, bounded event channel,
  independent cancellation, input-order final reports and per-job failure isolation.
  `copy-batch` CLI defaults to two workers. See [scheduling](scheduling.md).
- Offline benchmark harness, ten generated destinations at 1/2/4/10 workers,
  retained reports/events and measured child RSS/CPU/time. The small debug-build
  benchmark is not a physical USB/Raspberry Pi performance claim.
- `usb-device` read-only sysfs/udev inventory via explicit-column lsblk output and
  strict bounded snapshot parsing. `devices --snapshot examples/devices.json --json`
  uses generated metadata. Candidates are for review only; they grant no mount,
  write, ownership or safe-removal permission. See [Phase III](phase-iii.md).

## Local development storage

All development downloads, toolchains and temporary files belong inside ignored
`.dev/`, following the user's instruction. Build outputs remain in ignored `target/`.
Dependencies are in `.dev/cargo-home/`, Rust 1.99.1 plus host/ARM64 standard libraries,
Clippy and rustfmt are in `.dev/rustup/`, and test temporary files use `.dev/tmp/`.
The original temporary download files were moved into `.dev/previous-cargo-home/`
and `.dev/rustup-init.sh`.

Use the wrapper, which sets local paths and defaults to offline operation:

```sh
sh scripts/cargo-local.sh fmt --all -- --check
sh scripts/cargo-local.sh clippy --workspace --all-targets -- -D warnings
sh scripts/cargo-local.sh test --workspace
sh scripts/cargo-local.sh check --workspace --all-targets --target aarch64-unknown-linux-gnu --locked
```

The wrapper uses the local toolchain when installed and ordinary Cargo otherwise.
Use `CARGO_NET_OFFLINE=false` explicitly when downloading new development dependencies.
Cargo.lock includes the new workspace crate; no third-party versions changed.

## Verification

- Workspace formatting and Clippy with warnings denied — passed on Rust 1.99.1.
- Workspace tests — **65 passed** on Linux x86-64.
- Locked host build — passed.
- Locked ARM64 all-targets compile check — passed. No ARM64 native execution or
  hosted CI run was performed in this session. Native x86-64/ARM64 checks remain
  configured in `.github/workflows/rust.yml`.
- Benchmark: all ten destinations completed both generated games at each worker
  count. Results are documented in scheduling.md; original artifacts remain at
  `.dev/benchmarks/batch-qtmmasa7/` (debug Rust 1.95.0, before toolchain upgrade).
- CLI device snapshot tests and synthetic collector helper tests — passed. No host
  inventory or real removable device was inspected as a test.
- Shell wrapper syntax and Python benchmark syntax — checked.
- No third-party games, real USB media, mounting or hardware actions used.

## Boundaries and remaining work

- A readable journal commit/terminal line cannot prove its sync succeeded. Live
  failure reports retain unconfirmed games and prior confirmed deliveries. Old,
  missing, malformed or truncated journals grant no ownership or cleanup rights.
- Existing USB entries are still refused. No cleanup, merging, in-place resume or
  ownership-aware recovery actions were added; these remain design proposals.
- Scheduler counts and event queues are bounded, but archive metadata is still held
  per transfer. There is no adversarial memory quota, global disk reservation or
  forced timeout for blocked kernel I/O.
- Device candidates are metadata observations, not validated mounts or permissions.
  There is no hotplug watcher, calibrated physical port map, filesystem-health
  validation, mounting/unmounting adapter or safe-removal authorisation yet.
- Hardware power-loss, actual disconnects, target-hub throughput and Raspberry Pi
  execution remain unverified. Unit faults/process exits do not replace them.
- Dashboard/admin interface, deployment, random selection, returning-stick identity,
  cooldowns and incremental distributions remain unimplemented future work.

## Next work

Use the remaining device work in [Phase III](phase-iii.md): explicit physical-port
configuration, current identity/topology binding, exclusion of system/archive disks,
filesystem health policy and a narrowly scoped UDisks2 adapter. Keep all development
verification fixture-based. Calibrating and validating physical operation requires
the intended station/hub and a separately provisioned disposable test environment.
Do not enable privileged mount/write/removal actions based on inventory candidates.
Do not describe the larger roadmap as completed.

## Latest documentation/security review — 2026-10-22

README now describes general approved content bundles, preserves technical schema
names for compatibility, includes the bureaucratic identity and separates current
features from intended station behaviour. A second editorial pass establishes
a consistent cultist civil-service voice with cosmic dread rather than gore.
The README embeds a generated dusty, claustrophobic records-office banner, saved in assets/brand/
with its generation prompt and provenance. The broader product scope is recorded
in future-goals.md. No runtime/schema changes were made.

Focused source review covered transfer confinement/no-replace writes, source
revalidation, journal parsing, device admission observations and CI permissions.
Known limits remain: unbounded initial catalogue metadata/total journal history,
no forced timeout for blocked I/O, and no implemented mount/removal authorisation.
No common credential patterns were found in publishable files; `.dev/` and
`target/` remain excluded. This was not a comprehensive audit or a current
dependency-advisory scan. No physical media or privileged actions were used.

## Local repository state

This checkpoint includes the prototype, generated fixtures, CI configuration,
product decisions and Bureau README artwork. Consult Git history and `git status`
for commit/push state; no physical hardware actions or production deployment have
been performed.
