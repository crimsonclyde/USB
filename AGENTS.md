# USB — guidance for future sessions

Read docs/status.md first for the latest local stopping point and verification notes.

Read README.md, docs/phase-i.md, docs/phase-ii.md and docs/future-goals.md before
changing architecture.
Preserve the Bureau's identity and the visitor-data preservation principles.
Record new product goals in docs/future-goals.md; distinguish firm requirements
from proposals and do not treat future ideas as implemented features.

## Architecture
- Keep `usb-core` independent of hardware, mounting, USB APIs and CLI presentation.
- Catalogue indexing performs read-only filesystem I/O; planning is a pure function.
- Games are atomic units with explicit stable IDs. Never derive approval from file
  discovery, age, availability online, collection membership or filename.
- Keep manifests versioned and strict. Fail closed on malformed metadata, duplicate
  IDs, symlinks, special files, unknown references and arithmetic overflow.
- Phase I does not mount, copy, format, repair, delete, execute or unmount anything.
- Phase II must revalidate source identity, sizes, approval, free space and destination
  collisions at transfer time. A plan is an estimate, never permission to overwrite.
- Keep Linux file-handle operations in `usb-transfer`; keep the planner pure.
- Directory transfers require a fresh USB tree. Never merge, overwrite or clean
  existing entries automatically. Publish only complete, verified games with a
  no-replace rename; partial staging and unconfirmed publication are not rewards.
- Retain failed staging for inspection until an ownership/recovery policy exists.
  A report's write-attempt flag is not proof of ownership or permission to delete.
- Do not add async runtimes, databases or generic service layers until needed.

## Rust conventions
- Use safe Rust, edition 2024, rustfmt and idiomatic small modules. Unsafe is forbidden.
- Use checked arithmetic for sizes and capacity. Stable sorting needs explicit tie
  breakers. Do not let filesystem enumeration or hash map order affect output.
- Return contextual errors for user input and I/O; do not panic on archive data.
- Update Cargo.lock for dependency changes; prefer maintained portable Rust crates.
- Document observable selection rules and schema changes, including compatibility.

## Security
- Treat archive manifests, paths and filenames as untrusted input. Do not follow
  symlinks or admit special files. Never read visitor personal contents.
- Keep archive and destinations separate. Future writes belong only in a dedicated
  distribution directory and must never overwrite existing content.
- Do not download games. Use generated or clearly licensed fixtures only.
- Offline operation is mandatory; no cloud services or telemetry.
- Preserve the existing AGPLv3 licence and distinguish it from content licences.

## Verification
Run before completing a change:
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Use temporary directories and generated files. Cover selection invariants,
reproducibility, rejection paths and integer boundaries. Never test against real
removable media. Check Linux x86-64 and ARM64 in CI; report missing toolchains and
unrun tests honestly. Keep README usage and docs consistent with actual commands.
