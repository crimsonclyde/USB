# ⛧ UИSPΞΛKΛBLΞ STØRΛGΞ BURΞΛU ⛧

![A cramped, dusty Bureau records office: mismatched clerks pore over old files by oil lamplight; a USB stick lies among the dossiers and a small tentacle hides beside a cabinet.](assets/brand/bureau-archive-banner.png)

### DepΛrtment of Invøluntary Enlightenment · External Vessels Divisiøn

**Forbidden Knowledge. Freely Distributed. Duly Recorded.**

> Your arrival has been entered into the register.<br>
> The register predates your arrival.<br>
> Please have your vessel ready.

Ten portals. One archive. A queue that appears shorter when you stop counting.

[Current implementation](#current-implementation) · [CLI usage](#cli-usage) · [Preservation rules](#preservation-rules)

---

## ⸸ Nøtice to persøns above the wΛterline

Far beneath the dreaming city, the lamps of the records office remain lit.

Its clerks serve the Sleeper through an unbroken succession of inventories,
authorisations and meticulously countersigned transfers. Somewhere among the
shelves lies the order establishing the Bureau. Requests to retrieve it return
bearing the applicant's signature, dated several centuries before their birth.

The Bureau has recently opened a public counter.

You are invited to present an ordinary USB stick. The clerks will allocate a
portion of the archive, enter the transaction into the record, and return your
vessel when the necessary seals have been applied. What you do with the knowledge
thereafter is, according to a footnote no clerk will discuss, already accounted for.

**The archive remembers. The Bureau distributes.**

## ⸸ WhΛt is USB?

The **Unspeakable Storage Bureau (USB)** is an open-source, Linux-based system for distributing curated digital content through ordinary USB flash drives: books, zines, music, films, artwork, documentation, datasets, software and other material approved for redistribution.

Each offering is a complete content bundle with an explicit manifest, stable identifier and redistribution approval. The Bureau copies files; it does not need to understand or execute their contents. File naming, supported filesystems and capacity limits still apply.

**Current status:** a working CLI prototype for catalogue indexing, planning and verified transfers into ordinary directories. Automatic USB handling and the visitor interface are still under development. The station experience described below is the intended product.

No visitor accounts. No cloud. No internet connection during operation.

The Department does not require your email address. It has other means of
maintaining a sense of administrative inevitability.

Just bring a USB stick, connect it to one of the Bureau's ports, and receive a collection of digital artefacts from its archives.

The finished station will measure available space, select eligible bundles and use that space while preserving a safety reserve — without formatting the stick or deleting existing files.

## ⸸ Prøcedure 01 — Λdmission of an external vΞssel

*Intended station workflow. Automatic device management is not implemented yet.*

1. **Present the vessel.** Insert your USB stick into an available numbered port. Your place in the queue has been provisionally recognised.
2. **Submit to examination.** The Bureau checks the device, filesystem and available space. Your personal files are outside its jurisdiction.
3. **Await allocation.** The archive selects complete, approved bundles according to priorities, compatibility and capacity. Appeals concerning the size of infinity are handled by another department.
4. **Remain in attendance.** Content is copied and read back for verification. The illuminated port is processing your case.
5. **Await the seal.** Writes are finalised and the filesystem is unmounted. Do not remove the stick while this is in progress.
6. **Receive the release order.** Remove the stick only when the display explicitly says **Safe to remove**.

> A completed progress bar is a statement of quantity.<br>
> A release order is a statement of permission.<br>
> The Department recognises a distinction.

The target is ten independently managed ports. Bounded directory transfers already run concurrently; physical port management remains unimplemented.

## ⸸ Høldings of the submerged ΛrchivΞ

Some holdings are catalogued by subject. Others are catalogued by the room in
which they were first heard. The filing system remains internally consistent,
provided no one attempts to draw it.

The actual archive is local and curator-controlled. Possible collections include:

| Department | Example contents |
|---|---|
| Office of Printed Revelations | Books, zines, manuals and documentation |
| Registry of Acoustic Disturbances | Music, field recordings and spoken word |
| Department of Moving Apparitions | Films, animation and video |
| Repository of Visual Evidence | Artwork, photography and illustrations |
| Office of Instruments and Inquiry | Software, tools and educational material |
| Registry of Recorded Phenomena | Datasets and research material |
| The Uncatalogued | Other approved digital artefacts |

These are examples, not included collections. Every bundle needs explicit curator
approval and suitable redistribution rights. Public availability, age or a free
download does not establish permission. Required licence material travels with its
bundle. Approval is a curator decision, not an automatic legal or malware check.

### StΛnding ørder — An offering shall remain whøle

**Complete offerings. Explicit approval. No sacrifice of existing data.**

The clerks may disagree about the number of dimensions in a filing cabinet.
They are unanimous about incomplete submissions.

Each transfer aims to deliver as much useful content as safely fits.

The distribution engine follows configurable priorities, attempting to include complete bundles rather than blindly copying entire collections.

If a bundle is too large for the remaining space, the Bureau skips it and searches for smaller eligible offerings.

The goal is to use nearly all available space while preserving a safety margin for filesystem overhead.

No bundle should be left intentionally incomplete. A bundle may contain one file or an entire directory tree; its files and required extras travel together.

## ⸸ Λpparatus apprøved for the surface øffice

The first surface office will occupy a Raspberry Pi 5. A cyclopean plinth is
optional; a reliable power supply is not.

The target setup is:

- Raspberry Pi 5 (ARM64)
- External disk containing the master archive
- Raspberry Pi OS Lite (64-bit), administered through SSH
- Externally powered USB hub
- HDMI display with kiosk dashboard
- Rust-based distribution daemon

The target configuration supports **ten independent USB distribution ports**.

The software will remain hardware-independent wherever practical, allowing deployment on ordinary x86-64 Linux computers.

## ⸸ InstrumΞnts recøgnised by the technical secretariΛt

| Component | Current implementation / intended direction |
|---|---|
| Core software | Rust, edition 2024; unsafe Rust forbidden in workspace crates |
| Catalogue | Strict, versioned JSON manifests |
| Selection | Pure, deterministic capacity-aware planner |
| Transfers | Linux file handles, SHA-256 read-back verification, no-replace publication |
| Concurrency | Bounded synchronous worker threads; no async runtime |
| Device inventory | Read-only Linux sysfs/udev metadata through lsblk |
| Mount management | UDisks2 adapter planned; not implemented |
| Visitor/admin interface | Local web interface planned; not implemented |
| Identity and visit history | Local MySQL requested; not implemented |
| Startup | systemd planned; not implemented |

Normal Bureau operation must remain entirely offline, with no cloud services or
telemetry. Development tooling is separate from the deployed station.

<a id="preservation-rules"></a>

## ⸸ The preservΛtiøn dirΞctive

**The Bureau must never sacrifice a visitor's existing data.**

The vessel remains the visitor's property. The contents already aboard remain
outside the Bureau's remit. No supervisor, omen or sufficiently elaborate seal
may waive the following rules:

- Never format or repartition visitor drives.
- Never erase or overwrite existing personal files.
- Never execute software from visitor drives.
- Never inspect unrelated personal file contents or import files from visitor drives.
- Read back newly written Bureau files to verify the transfer.
- Write only within a dedicated distribution directory.
- Reject unsupported, unsafe or ambiguous devices.
- Do not attempt automatic filesystem repairs.
- Keep the master archive protected from destination writes.
- Isolate failures so one faulty stick does not interrupt others.
- Show *Safe to remove* only after successful finalisation and unmounting.

The current prototype requires a stable, curator-controlled archive and exclusive
destination access. It is not a sandbox for a hostile process modifying either
tree concurrently. Catalogue metadata and total journal history do not yet have
global resource quotas, and blocked kernel I/O has no forced timeout. Device
inventory is observation only: it grants no permission to mount, write or remove.

USB filesystems require defensive handling. No implementation can guarantee protection from faulty hardware, malicious devices or every possible filesystem failure, but preserving visitor data is the project's highest priority.

## ⸸ The public cøuntΞr

The public counter should feel impeccably maintained by an institution whose
continued existence is a mistake in the structure of the universe. Cthulhu's
cultists have had aeons to perfect the paperwork. They have used the time.

Dust in weak lamplight. Swollen drawers. Clerks in threadbare coats who do not
look up together. A case number issued before anyone asks your name. The unease
belongs in the courtesy of the messages, the narrowing space between the shelves,
and a movement behind a cabinet that no one acknowledges.

Ten readable port cards will show the assigned vessel name (`40c3_017`), connection time, progress, errors and removal status. Raw device identifiers belong in the offline admin interface.

| Bureau status | Meaning |
|---|---|
| AWAITING VESSEL | USB port available |
| VESSEL DETECTED | Device connected |
| EXAMINING VESSEL | Checking filesystem and capacity |
| CONSULTING THE ARCHIVE | Selecting content |
| TRANSMITTING | Copying and verifying complete bundles |
| SEALING THE RITUAL | Finishing writes and unmounting |
| RELEASE AUTHORISED | Safe to remove |
| CONTAINMENT BREACH | Transfer interrupted or device removed |
| VESSEL REJECTED | Unsupported or unsafe device |

The visual language combines restrained witch house typography with dusty
archival forms, worn wood, faded ink and the quiet certainty that someone has
already approved this visit. Claustrophobic, antiquarian cosmic horror takes
precedence over clean futuristic surfaces, uniformed figures or monumental open
halls. The uncanny remains peripheral; gore and splatter have no place here.

Mysterious presentation must never compromise essential usability or safety information.

Proposed visit directories such as `USB_AKTE_40C3_000042` open a new case for each
visit, leaving older attempts untouched. Registration records, case naming,
weighted random selection and returning-stick recognition are future work.
Playful cooldowns will initially be disabled. An identifier or Bureau-looking
folder is never proof of ownership or permission to overwrite.

A proposed acknowledgement might read:

> **EINGANGSBESCHEID · Vorgang 40C3/000042**<br>
> Ihr Datenträger wurde behördlich wahrgenommen.<br>
> Die zuständige Stelle befindet sich unterhalb des Meeresspiegels.<br>
> Von persönlichen Rückfragen ist bis zum Erwachen abzusehen.

*Illustrative wording; these records are not generated by the current CLI.*

The planned offline admin interface controls port calibration, archive priorities,
feature switches, diagnostics and explicit retries. Safety and redistribution
approval rules cannot be disabled.

## ⸸ SchΞdule of Λuthorised manifestatiøns

Phase I is implemented: a read-only catalogue indexer, deterministic distribution planner and CLI. Phase II now has an experimental directory-copy foundation with whole-bundle verification, progress and cancellation. Hardware integration remains a future phase.

- [x] **Phase I — The Catalogue:** Rust workspace, archive indexing, metadata and capacity-aware selection engine
- [ ] **Phase II — The Transmission:** Directory copying, persistent diagnostics, fresh-destination retry and bounded batch scheduling implemented; production hardening remains
- [ ] **Phase III — The Portals:** Read-only device inventory implemented; physical port mapping, mount validation and safe unmounting remain
- [ ] **Phase IV — The Interface:** Local dashboard with live transfer progress
- [ ] **Phase V — The First Summoning:** Raspberry Pi 5 deployment with two test USB drives
- [ ] **Phase VI — The Tenfold Gate:** Scale to ten ports and optimise simultaneous transfers
- [ ] **Phase VII — The Eternal Archive:** Catalogue maintenance, incremental distributions and further improvements

Future product ideas, including an admin interface, random whole-bundle offerings
and returning-stick cooldown rituals, are tracked in [Further rituals — future
goals](docs/future-goals.md). These ideas remain separate from completed phases.

<a id="cli-usage"></a>

## ⸸ Cønsulting the ΛrchivΞ

The CLI has been tested on Linux x86-64 and compile-checked for ARM64. Native
Raspberry Pi operation and physical USB behaviour remain unverified.
Install current stable Rust with rustfmt and Clippy, then:

```bash
cargo build --workspace --locked
cargo run -p usb-cli -- catalogue scan examples/archive
cargo run -p usb-cli -- --archive examples/archive catalogue list
cargo run -p usb-cli -- --config examples/config.json plan --capacity 16G --filesystem exfat
cargo run -p usb-cli -- --archive examples/archive plan --capacity 64G --filesystem fat32
```

Capacity means **available free space**, supplied manually. `G` means GiB; `GB`
means decimal gigabytes. These commands read the archive and print estimates;
they do not access or write to destination devices. Add `--json` for structured
output. Install the CLI with `cargo install --path crates/usb-cli --locked` to use
`usb` directly. The default archive path is `/srv/game-archive`.

Bundles have strict, versioned JSON manifests, stable IDs and explicit redistribution
approval. The included sample is original demonstration text. Priority, release
year when supplied, size and stable ID determine selection order. Complete bundles
are chosen greedily; oversized bundles are skipped so smaller offerings can still fit.
Allocation estimates and a configurable safety reserve protect the capacity budget.

**Schema compatibility:** the existing implementation still calls a bundle
`game.json` and uses fields such as `platform` and `release_year`, the
`USB/games/<stable-id>/` output layout, and `/srv/game-archive` as its default path.
These are historical technical names, not a restriction to a particular content
type. A release year can be omitted. No schema, CLI or output paths are renamed by
this README update; see the Phase I guide for the exact format.

See [the Phase I technical guide](docs/phase-i.md) for archive structure,
configuration, architecture, algorithm trade-offs, filesystem compatibility,
verification commands, limitations and the Phase II roadmap. Guidance for future
contributors and coding agents lives in [AGENTS.md](AGENTS.md).

## ⸸ A cøntrølled first trΛnsmissiøn

An experimental `copy-to-directory` command now stages, verifies and publishes
complete bundles into a fresh `USB` directory. It measures available space, preserves
existing files and refuses an existing `USB` entry. Interrupted bundles stay in a
separate staging area and are never reported as completed offerings.

```bash
mkdir -p .dev/tmp
usb_demo_destination=$(mktemp -d "$PWD/.dev/tmp/usb-demo.XXXXXX")
cargo run -p usb-cli -- --archive examples/archive copy-to-directory \
  --destination "$usb_demo_destination" --filesystem ext4
```

Use ordinary temporary directories for this prototype. Progress and Ctrl-C
cancellation are available; mounting and safe-removal authorisation are not.
`usb inspect-transfer --destination "$usb_demo_destination" --json` reads persisted
transfer diagnostics after an interruption. Keep the old tree intact and retry into
a fresh destination. Development dependencies and temporary test files can be kept
in the ignored `.dev/` folder; see the [local setup](docs/phase-i.md#build-and-verification).
See [the Phase II guide](docs/phase-ii.md) for transfer semantics, failure handling,
limitations and the work remaining before device integration.

`copy-batch` accepts up to ten `--destination` arguments with `--workers 1..10`
(default 2), independent job reports and cancellation. See [bounded transfers and
local benchmark results](docs/scheduling.md). These job labels do not identify
physical hub ports. `devices --snapshot examples/devices.json --json` demonstrates
the [read-only device inventory](docs/phase-iii.md) using generated metadata.

## ⸸ StΛnding instructiøns to the clΞrks

1. **Preserve existing data.** Safety always takes precedence over filling a drive.
2. **Remain offline.** Normal operation must not depend on external services.
3. **Keep it portable.** The application should run on ARM64 and x86-64 Linux.
4. **Stay predictable.** Given the same catalogue and available capacity, generate reproducible distribution plans.
5. **Fail independently.** One problematic USB stick must not compromise other transfers.
6. **Grow without limits imposed by the software design.** New platforms and collections should not require rewriting the distribution engine.
7. **Keep the interface simple.** Plug in, receive, remove.

<a id="current-implementation"></a>

## ⸸ StΛte of the depΛrtmΞnt

**Pre-alpha — The surface office is not yet open.**

See the [development handoff](docs/status.md)
for completed work, verification results and the next implementation step.

There is currently no production-ready release. The architecture, hardware integration and safety mechanisms remain subject to development and testing.

Do not connect USB drives containing irreplaceable data to experimental builds.

## ⸸ Λuthority to reprøducΞ

The Bureau source code is licensed under **GNU AGPLv3**; see [LICENSE](LICENSE).

The Bureau's source-code licence and the redistribution licences of archive content are separate matters.

The repository contains generated demonstration fixtures, not a third-party content archive. Any contributed content must have explicit redistribution permission.

---

### ⛧ THE UNSPEΛKABLE STØRAGE BURΞAU ⛧

**TΞN PØRTΛLS. ØNΞ ΛRCHIVΞ.**

*That which sleeps has delegated. That which is delegated must be filed.*

> Office hours are posted beside the entrance.<br>
> If the entrance has moved, your appointment remains valid.

`[ COUNTER OPEN IN PRINCIPLE · RELEASE AUTHORITY NOT YET COMMISSIONED ]`
