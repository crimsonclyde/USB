# ⛧ Further rituals — future goals and ideas

This is the Bureau's living product notebook. Add future goals here so they survive
individual development sessions. Distinguish requirements from ideas still needing
design. These notes do not expand the implemented Phase I scope.

## Station decisions — 2026-10-22

These requirements come from the operator's answers to the hardware and product
questions. Recommendations below remain proposals until accepted. None of this
section adds implemented features or changes current directory-copy semantics.

### Confirmed direction

- First target: the operator's prepared Raspberry Pi 5, with an external disk
  available for later repository/archive storage. The operator owns an older hub
  and will locate it; its model and suitability remain unknown.
- Visitor devices are destinations only. Never execute their software, import
  their files or read unrelated personal contents. Device/filesystem metadata,
  bounded Bureau records and read-back of newly written Bureau payloads are the
  intended technical scope; verification must retain the existing read-back checks.
- On reconnection, preserve previous attempts and create a fresh visit directory
  with bureaucratic cosmic-horror case naming when sufficient safe space remains,
  replacing the initial `t23_<unix_timestamp>` idea. Never merge, resume, overwrite
  or automatically clean old attempts. The exact layout and
  collision suffix need design; the current engine still refuses an existing `USB`.
  A timestamp or Bureau-looking name alone proves neither ownership nor delivery.
- The public dashboard shows connected sticks, connection time and an assigned
  `40c3_nnn` name. Use available device identifiers/serials for recognition where
  possible, without assuming they are present, unique or authentic.
- Keep minimal identity and visit state locally in MySQL: observed identifier,
  serial when useful, assigned name and last connection. MySQL is the operator's
  requested store, replacing the earlier tentative SQLite direction for this state;
  no database is added to the current planner or transfer engine by this decision.
- Remember devices that cannot be written to and show a ZONK rather than repeating
  the same unsuccessful operation on every reconnect. Delayed automatic reassessment
  for transient failures plus explicit admin retry is accepted; exact delays still
  need design. A past failure never authorises writes or safe removal.
- Issue a random Bureau identifier inside a designated Bureau distribution folder
  when hardware identifiers are unreliable; remember its association locally.
  This is a recognition aid, never proof of ownership or authenticity. Handle
  copied, missing, malformed or conflicting identifiers conservatively, using
  bounded no-follow reads of designated records without inspecting personal files.
- Optional surprise selection uses seeded weighted randomness favouring higher
  priorities, older games and smaller games, with previously confirmed deliveries
  excluded when reliable history is available. This supersedes the narrower
  equal-priority/year shuffle proposal. Exact weights and algorithm remain open;
  whole-game eligibility and capacity rules remain mandatory.
- Playful cooldowns are disabled for the first version. Technical ZONKs and retry
  rules remain independent of that setting.
- Defer disposable hardware provisioning and power-loss tests until the Pi is
  running. No expendable sticks or disposable boot medium have been confirmed.
- Extend the Bureau identity into an oppressive, absurd bureaucracy run by
  Cthulhu's cultists beneath the dreaming city: registration, case numbers, decrees,
  seals and ANGST. Use cosmic horror through implication, impossible procedure,
  claustrophobia and antiquarian decay: dust, worn wood, mismatched civilian clerks
  and a barely visible tentacle. Avoid clean cyberpunk styling, uniform robes and
  vast open halls. No gore or splatter. Errors and safe-removal
  instructions remain plain and unambiguous.

### Case-naming proposal and development access

Proposed visitor folder: `USB_AKTE_40C3_000042`, carrying a fresh visit/case number,
distinct from the stick's persistent registration. Create exclusively and choose
another case number on collision; names and numbers grant no ownership. Proposed
record titles: `EINGANGSBESCHEID.txt` and `AKTENVERMERK.json`, with a random Bureau
token only in a designated bounded record. Final layout/names remain proposals.
Suggested copy: “Department of Involuntary Enlightenment” and “Release order
issued — Safe to remove”. No ritual text may hide failure or imply false safety.

The operator asked about Codex installation versus remote Pi debugging. Recommend
SSH over the local network with a dedicated development account/key and Codex CLI
on the Pi for an app SSH project, where supported by the installed client. Verify
ARM64 runtime compatibility during setup. Keep the private key on the controlling
machine and install only its public key on the Pi; verify the host key. Direct
SSH from the development computer is a fallback if app integration is unavailable.
No endpoint/key has been provided; no installation or connection has occurred.
Codex development may use online model services; the Bureau application must stay
fully offline and independent of Codex. VMs remain for disposable fault tests,
not a prerequisite for ordinary Pi debugging.

References: [Codex SSH projects](https://learn.chatgpt.com/docs/remote-connections),
[headless login](https://learn.chatgpt.com/docs/auth).

### Design details — unimplemented; accepted choices above take precedence

1. **Hardware and socket map:** choose a powered USB 3 hub with ten data sockets
   (or two powered hubs), adequate simultaneous-load power and no upstream power
   backfeed. Calibrate physical sockets 1–10 individually with generated test media,
   bind them to controller/hub topology, then test reboot and USB 2/3 variants.
   Keep the archive connection outside the visitor map. Unknown or ambiguous paths
   stop admission; enumeration order and `/dev/sdX` never identify a socket.
2. **Admission and removal:** require a mapped visitor socket, current identity,
   exclusion of system/archive disks, no existing mounts/shared storage, a supported
   simple writable filesystem and acceptable health. Reject dirty/uncertain media
   without automatic repair. Determine filesystem-specific checks and prevent
   unintended journal replay during preflight. Mount privately through a narrow
   adapter with `nodev,nosuid,noexec`; validate actual device/options and measure
   space before writes. Only successful finalisation, closed handles, successful
   ordinary unmount and verification that the same device has no remaining mounts
   permit safe removal. Failed or forced/lazy unmounts cannot produce that status.
3. **Disposable testing:** use a QEMU/KVM Linux guest with generated FAT32/exFAT/ext4
   USB disk images, disposable overlays and no host-disk passthrough. Test virtual
   disconnects, abrupt guest termination, full filesystems, busy unmounts and
   injected I/O/sync errors. Guest termination is a crash test, not proof of physical
   cache/power-loss behaviour. Later use separate expendable media and a disposable
   Pi test system; never cut power to the prepared archive or development disk.
4. **Visit directories:** exclusively create a fresh bureaucratically named case
   directory at the destination root, resolving collisions without replacement;
   retain whole-game staging
   and no-replace publication inside it. Recheck capacity including retained partial
   attempts. Inspect only designated, bounded Bureau records without following
   links; do not recursively inspect old payloads to decide whether to start again.
   Operator-assisted reverification may be designed later; automatic cleanup and
   in-place recovery are not requested by the fresh-folder policy.
5. **Interfaces and state:** public cards add progress, plain-language failure reasons
   and safe-removal status; raw identifiers remain in authenticated local admin.
   Admin owns port calibration, catalogue approval/priorities, worker/reserve limits,
   feature switches, diagnostic inspection, explicit retry/reset, retention and
   offline updates. Safety and approval rules cannot be disabled. Add only necessary
   failure reason/retry state, successful-delivery game IDs and cooldown expiry to
   the local database when those features exist; define bounded retention and backup.
6. **Selection:** keep the existing deterministic planner available. The accepted
   surprise mode uses seeded weighted randomness with eligibility, capacity and
   reliable delivery-history filtering. Define bounded priority/age/size weights
   before implementation and test integer limits. Consider remaining whole games
   that fit. Persist seed and policy version for reproducibility. Unknown history
   gives no duplicate-free guarantee.
7. **Recognition and ZONK:** correlate serial/vendor/product and filesystem UUID as
   fallible observations, with explicit ambiguous/anonymous states. Never merge
   devices solely because they share a missing or generic serial. A Bureau-issued
   random token is the accepted fallback, not ownership or authorisation.
   Cache unsupported/read-only failures until identity/configuration changes or an
   operator requests recheck; transient I/O, full-media and busy-unmount failures
   need a bounded retry interval or explicit retry, not a permanent blacklist.
   Every admitted retry still needs current safety checks. Distinguish technical
   ZONK from the earlier playful cooldown proposal. Cooldowns are disabled
   initially, starting only after a successful visit including safe unmount if
   enabled later. Early returns do not extend expiry; failures do not earn penalties.
   Partial successes require per-game delivery state, independent of visit cooldown.
8. **Deployment:** “station” means the assembled Pi, archive disk, powered hub,
   display and Bureau software. Propose 64-bit Raspberry Pi OS Lite, systemd startup,
   a restricted daemon account and narrow mount permissions, a localhost kiosk,
   read-only archive access, bounded logs and database/configuration backups. Import
   authenticated offline update bundles with rollback while ports are idle. Never
   trust or run an update found on a visitor stick. Reconcile interrupted visits
   conservatively after reboot; old progress cannot imply safe removal.

Reference material for the recommendations:
[Raspberry Pi hardware/power](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html),
[UDisks mount options](https://storaged.org/udisks/docs/mount_options.html),
[QEMU USB emulation](https://www.qemu.org/docs/master/system/devices/usb.html).

## Firm requirements

### General content distribution

The Bureau is a general distributor of approved digital bundles, including books,
zines, music, films, artwork, documentation, datasets and software. Public product
descriptions must not frame it as limited to games. Existing game-oriented schema
names and paths remain compatible until a separately designed migration; broader
positioning does not relax approval, completeness or filesystem safety rules.

### Whole offerings, always

**Distribute complete games, never half a game.** Random selection must choose whole
catalogue games with all their payload files, required extras and licence material.
It must never select a random subset of a game's files or truncate a game to fill
space. If the complete game cannot fit safely, skip it and consider another.

A future transfer must only report a game as delivered after the entire game has
been copied and verified. Interrupted or failed transfers must not present partial
games as completed rewards. Existing visitor data must remain untouched.

### An offline admin interface

Provide a local operator interface where features can be switched on and off.
Gamification needs operator controls rather than being permanently forced on.
The exact interface, access controls and implementation phase remain to be designed.

Candidate settings to refine:

- Enable or disable random game selection.
- Enable or disable returning-stick cooldowns and ZONK responses.
- Configure the cooldown range, initially proposed as 3 minutes to 2 hours.
- Choose which approved platforms and collections participate, and adjust priorities.
- Configure visitor-facing ritual messages.

Feature switches must preserve the project's safety, capacity, compatibility and
explicit redistribution-approval rules. They cannot turn incomplete or unapproved
content into eligible offerings.

## Gamification direction

Make visiting the Bureau feel like a small ritual with surprise and a reason to
return. Random games are a desired future selection mode. The exact reward rules
remain open; the ordinary priority-based planner should remain available.

Randomness changes **which complete eligible games** are chosen, not whether their
contents are delivered intact. Decide how randomness interacts with the small-and-
ancient doctrine, collection priorities, capacity use and games previously received.
Keep selection testable: consider an explicit seed as an input so a given catalogue,
capacity, policy and seed can reproduce a plan. This mode is not implemented yet.

## Idea to explore: the stars are not ready

This earlier playful cooldown proposal is separate from the operator's newer
requirement to remember technical write failures, recorded above.

Recognise a returning USB stick using a future stick identifier ("UID"). After a
successful visit, give that identifier a randomly chosen waiting period, initially
suggested as **3 minutes to 2 hours**. Reconnecting during that period would produce
a **ZONK** response instead of another distribution.

Possible visitor message:

> **ZONK — The stars do not shine brightly for you just yet. Return when they align.**

The purpose is playful anticipation and repeat visits. This is a design proposal,
not an implemented blocklist or a settled policy.

A candidate flow to discuss:

1. A visitor receives a selection of complete games.
2. On successful completion, record a local cooldown expiry for that stick ID.
3. An early return shows ZONK and an understandable waiting message; it does not
   modify the stick or remove previously received games.
4. After expiry, the stick becomes eligible for another offering.

Open decisions:

- What should identify a stick? Evaluate device identifiers, filesystem identifiers
  and a Bureau-issued token before choosing; do not assume a universal reliable UID.
  Preserve the rule against reading unrelated personal file contents.
- Should the cooldown start only after a successful distribution? How should a
  failed or interrupted visit be retried without penalising the visitor?
- Does reconnecting leave the original expiry unchanged? Avoid accidentally
  extending the wait on every early return.
- How is the random duration chosen, and should visitors see the exact remaining
  time or only an atmospheric message?
- What happens when identification is unavailable, ambiguous, changed or shared?
- How should offline timekeeping, restarts and clock changes affect expiry?
- What minimal local history should be retained, for how long, and how can an
  operator reset or override a cooldown?
- Should repeat visits favour games the stick has not received, and how should
  that history interact with existing-file preservation?

## Relationship to the roadmap

[Operator recovery policy](recovery.md) defines the current retain/inspect/fresh-
destination retry actions. Ownership-aware review, reverification, cleanup and
in-place retry are proposals requiring further design and tests. They are not
implemented recovery features; visitor-data preservation and whole-game guarantees
remain firm requirements for any future recovery action.

The [Phase II directory-copy foundation](phase-ii.md) now supports complete-game
verification and interruption handling. Finish its recovery and production
hardening work before using real reward mechanics. Stick recognition depends on later device handling; the admin interface
belongs with the future local interface work. Random selection can be designed and
tested independently in the planner once its policy is agreed.

No returning-stick identification, cooldown enforcement, random selection or admin interface is
implemented by this document. Keep this notebook updated as ideas become decisions,
and link the eventual design and tests here.
