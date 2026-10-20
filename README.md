# ⛧ UИSPΞΛKΛBLΞ STØRΛGΞ BURΞΛU ⛧

### USB — The Unspeakable Storage Bureau

**Forbidden Knowledge. Freely Distributed.**

*An offline, self-service game distribution machine. Ten portals. One archive. Countless forgotten worlds.*

---

## ⸸ What is USB?

The **Unspeakable Storage Bureau (USB)** is an open-source, Linux-based distribution station designed to spread retro games, freeware, homebrew and other freely redistributable software through ordinary USB flash drives.

No accounts. No cloud. No internet connection. No subscriptions.

Just bring a USB stick, connect it to one of the Bureau's ports, and receive a collection of digital artefacts from its archives.

The Bureau automatically determines how much space is available, selects suitable content and fills the stick as far as safely possible — without formatting it or deleting existing files.

**The archive remembers. The Bureau distributes.**

## ⸸ How the ritual works

1. **Offer your vessel** — Insert a USB flash drive into an available port.
2. **Vessel examination** — The Bureau detects the drive, filesystem and available capacity.
3. **Archive allocation** — A suitable collection is assembled, prioritising smaller and older games.
4. **The transmission** — The selected games are copied while progress is displayed.
5. **Sealing the ritual** — All pending writes are completed and the filesystem is safely unmounted.
6. **Release authorised** — The display confirms that the USB stick is safe to remove.

Every port operates independently. Multiple visitors can receive their collections simultaneously.

## ⸸ The archives

The Bureau maintains a growing, locally stored catalogue of games and software.

Planned collections include:

| Archive | Contents |
|---|---|
| Ancient Machines | Commodore 64, Amiga, Atari and other early computers |
| Forgotten Commands | MS-DOS games |
| Pocket Dimensions | Nintendo DS / DSi and other handheld homebrew |
| The Interpreter's Vault | ScummVM-compatible adventures |
| The Emulation Chambers | RetroArch-compatible games |
| The Larger Abominations | Larger console and modern freeware games |
| The Uncatalogued | Future platforms and collections |

Only software with suitable redistribution rights is eligible. Being old, abandoned or freely downloadable does not automatically make a game redistributable.

### The allocation doctrine

**Small and ancient first. Large and unspeakable later.**

Every flash drive receives as much useful content as possible.

The distribution engine follows configurable priorities, attempting to include complete games rather than blindly copying entire collections.

If a game is too large for the remaining space, the Bureau skips it and searches for smaller eligible offerings.

The goal is to use nearly all available space while preserving a safety margin for filesystem overhead.

No game should be left intentionally incomplete.

## ⸸ The machine

The initial prototype is planned around:

- Raspberry Pi 5 (ARM64)
- Attached SSD containing the master archive
- Raspberry Pi OS Lite or Debian Linux
- Externally powered USB hub
- HDMI display with kiosk dashboard
- Rust-based distribution daemon

The target configuration supports **ten independent USB distribution ports**.

The software will remain hardware-independent wherever practical, allowing deployment on ordinary x86-64 Linux computers.

## ⸸ Built with forbidden technology

| Component | Planned technology |
|---|---|
| Core software | Rust |
| Asynchronous runtime | Tokio |
| Local API | Axum |
| USB detection | Linux udev |
| Filesystem management | UDisks2 / Linux |
| Catalogue | SQLite and manifests |
| Frontend | HTML, CSS and JavaScript |
| Live progress | WebSockets or Server-Sent Events |
| Service management | systemd |

The Bureau is intended to run entirely offline during normal operation.

## ⸸ Preserve the vessel

**The Bureau must never sacrifice a visitor's existing data.**

The following principles guide development:

- Never format or repartition visitor drives.
- Never erase or overwrite existing personal files.
- Never execute software from visitor drives.
- Never inspect unrelated personal file contents.
- Write only within a dedicated distribution directory.
- Reject unsupported, unsafe or ambiguous devices.
- Do not attempt automatic filesystem repairs.
- Keep the master archive protected from destination writes.
- Isolate failures so one faulty stick does not interrupt others.
- Show *Safe to remove* only after successful finalisation and unmounting.

USB filesystems require defensive handling. No implementation can guarantee protection from faulty hardware, malicious devices or every possible filesystem failure, but preserving visitor data is the project's highest priority.

## ⸸ The Bureau's interface

The dashboard is intended to be simple, readable and slightly unsettling.

Ten fixed port indicators display the status of every connected vessel.

| Bureau status | Meaning |
|---|---|
| AWAITING VESSEL | USB port available |
| VESSEL DETECTED | Device connected |
| EXAMINING VESSEL | Checking filesystem and capacity |
| CONSULTING THE ARCHIVE | Selecting content |
| TRANSMITTING | Copying games |
| SEALING THE RITUAL | Finishing writes and unmounting |
| RELEASE AUTHORISED | Safe to remove |
| CONTAINMENT BREACH | Transfer interrupted or device removed |
| VESSEL REJECTED | Unsupported or unsafe device |

The visual identity draws inspiration from witch house typography, occult symbols, dark terminals and cosmic horror.

Mysterious presentation must never compromise essential usability or safety information.

## ⸸ Development roadmap

The project is currently in its initial design stage.

- [ ] **Phase I — The Catalogue:** Rust workspace, archive indexing, metadata and capacity-aware selection engine
- [ ] **Phase II — The Transmission:** Reliable copying, progress tracking and interruption handling
- [ ] **Phase III — The Portals:** USB detection, filesystem validation, physical port mapping and safe unmounting
- [ ] **Phase IV — The Interface:** Local dashboard with live transfer progress
- [ ] **Phase V — The First Summoning:** Raspberry Pi 5 deployment with two test USB drives
- [ ] **Phase VI — The Tenfold Gate:** Scale to ten ports and optimise simultaneous transfers
- [ ] **Phase VII — The Eternal Archive:** Catalogue maintenance, incremental distributions and further improvements

## ⸸ Development principles

1. **Preserve existing data.** Safety always takes precedence over filling a drive.
2. **Remain offline.** Normal operation must not depend on external services.
3. **Keep it portable.** The application should run on ARM64 and x86-64 Linux.
4. **Stay predictable.** Given the same catalogue and available capacity, generate reproducible distribution plans.
5. **Fail independently.** One problematic USB stick must not compromise other transfers.
6. **Grow without limits imposed by the software design.** New platforms and collections should not require rewriting the distribution engine.
7. **Keep the interface simple.** Plug in, receive, remove.

## ⸸ Status

**Pre-alpha — Under construction.**

There is currently no production-ready release. The architecture, hardware integration and safety mechanisms remain subject to development and testing.

Do not connect USB drives containing irreplaceable data to experimental builds.

## ⸸ Licence

The project licence has not yet been selected.

The Bureau's source-code licence and the redistribution licences of the games and other content in its archives are separate matters.

Game archives, copyrighted ROMs and other third-party content are not included in this repository unless their respective licences explicitly permit redistribution.

---

### ⛧ THE UNSPEAKABLE STORAGE BUREAU ⛧

**TΞN PØRTΛLS. ØNΞ ΛRCHIVΞ.**

*What was forgotten shall be distributed.*

`[ VESSEL AWAITED — TRANSMISSION NOT YET INITIATED ]`
