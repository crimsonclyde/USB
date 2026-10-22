# Bounded directory transfers

`usb-transfer::scheduler::run_batch` runs a finite batch of one to ten independent
directory transfers. The CLI exposes it as `copy-batch`. These are local directory
jobs, not detected USB devices or calibrated physical port assignments.

```sh
mkdir -p .dev/tmp
export TMPDIR="$PWD/.dev/tmp"
usb_first_destination=$(mktemp -d)
usb_second_destination=$(mktemp -d)
sh scripts/cargo-local.sh run -p usb-cli -- --archive examples/archive --json copy-batch \
  --destination "$usb_first_destination" --destination "$usb_second_destination" \
  --workers 2 --filesystem ext4
```

Repeat `--destination` up to ten times. Each root must already exist and be separate
from the archive and other destinations, with no existing `USB` entry. The CLI
assigns `port-1`, `port-2`, etc. in argument order; these labels do not identify a
hardware port or stick. All jobs use the selected archive, filesystem policy and
budget flags. `--capacity-limit` limits each job, not the aggregate batch.

## Scheduling and failure behaviour

- Worker count is an upper bound from 1 to 10, defaulting to 2 in the CLI. The
  effective count is capped by job count. If a later worker cannot start, already
  started workers drain the batch. Failure to start even one worker returns an
  error before any destination writes.
- Workers claim jobs in input order. Dispatch, progress and completion events can
  interleave; final `jobs` results always retain input order.
- Batch size, worker bounds, duplicate/invalid job IDs, invalid planner options and
  overlapping or aliased existing destination roots are checked before writes.
  A bad archive, missing/unsafe destination or existing `USB` entry is an isolated
  per-job preparation failure; healthy siblings continue.
- Each worker invokes the existing transfer engine afresh. Queued jobs do not
  reuse plans or snapshots. Rights, identity, hashes, space and collisions are
  checked at execution time. Completed games survive later failures.
- Library jobs have independent `Arc<AtomicBool>` cancellation flags. Cancelling a
  queued job produces a cancelled result without starting a transfer or creating
  USB. Running cancellation remains cooperative between bounded operations.
  CLI Ctrl-C, SIGTERM and SIGHUP set all job flags, including queued jobs.
- A worker panic is caught and represented as failure kind `worker`, with no
  reconstructed delivery claims. Its tree is retained for inspection. Ordinary
  source/IO errors remain contextual errors; process aborts and out-of-memory
  termination cannot be isolated by a thread pool.
- Overall status is `failed` if any job fails, otherwise `cancelled` if any job is
  cancelled, otherwise `completed`. The CLI returns nonzero for either unsuccessful
  status after printing the complete batch report.

JSON stdout contains one final batch report. JSON Lines on stderr wrap transfer
events with `job_id`. The outer `event` is `queued`, `started`, `transfer` or
`finished`; a `transfer` event carries the ordinary transfer event in `transfer`.
A pre-cancelled queued job has no `started` event. Byte progress is throttled by the
CLI per job to roughly 1 MiB. `JobResult.failure` covers preparation/worker errors;
runtime errors and unconfirmed games remain inside its nested `report`.

Reports expose elapsed milliseconds, actual worker count, queue capacity and total
confirmed payload bytes. Only completed whole games contribute to that byte total;
copied, staged or unconfirmed bytes do not. Numeric JSON consumers must preserve
integer precision. Individual transfer/journal schemas are unchanged. The new
`worker` failure kind and batch envelope are additive pre-alpha API additions.

## Resource limits and measurements

At most ten worker threads and ten jobs are admitted. The event/result channel
holds at most twice the effective worker limit: 2–20 messages. A slow observer
applies backpressure; no unbounded progress history is accumulated. The observer
runs on the caller's thread. If it panics, the receiver disconnects before worker
joins; active workers request cancellation on failed sends and stop cooperatively.

Each active stream uses a 128 KiB byte buffer. Catalogue, plan, snapshot and final
report metadata still scale with archive size and are held independently per job;
there is no adversarial memory quota or global disk-space reservation. Destinations
on the same filesystem compete for free space. Runtime space checks and I/O error
handling still apply. A blocked kernel operation can hold a worker indefinitely;
the scheduler does not implement unsafe forced thread termination.

The offline Linux benchmark generates two complete games and ten fresh destinations
per configuration. It retains its archive, output trees, reports and events under
ignored `.dev/benchmarks/`. It refuses a run without enough space for estimated
outputs plus a 1 GiB reserve. It never mounts, deletes, uses removable media or
downloads content.

```sh
sh scripts/cargo-local.sh build --workspace --locked
python3 scripts/benchmark-batch.py
```

One local x86-64 debug-build run on 2026-10-22, Rust 1.95.0, using two 2 MiB games
plus generated licence files per job (40 MiB total confirmed payload):

| Workers | Queue messages | Wall time | Payload MiB/s | Child maximum RSS KiB |
|---:|---:|---:|---:|---:|
| 1 | 2 | 2.675 s | 14.95 | 18,616 |
| 2 | 4 | 1.263 s | 31.68 | 19,000 |
| 4 | 8 | 0.773 s | 51.73 | 19,000 |
| 10 | 20 | 0.423 s | 94.63 | 19,000 |

This is one small, cached local-filesystem run with debug hashing, including process
launch overhead. RSS is the Linux child-process high-water mark, including launch.
It is not a Raspberry Pi, USB hub, physical-stick or large-archive performance
claim. Keep the conservative two-worker CLI default until production measurements
justify changing it. Original artifacts: `.dev/benchmarks/batch-qtmmasa7/summary.json`.

Tests cover ten destinations under a worker limit, stable report ordering, queued
and mid-copy cancellation, independent failures, approval changes while jobs are
queued, duplicate/nested destinations and observer-panic shutdown. Real hub
throughput, ARM64 execution and hardware failure isolation remain unverified.
