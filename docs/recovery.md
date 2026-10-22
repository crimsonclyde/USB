# Operator recovery policy

This document defines the supported recovery actions for the experimental directory
copy engine and separates them from future proposals. It grants no ownership of
visitor files and introduces no automatic cleanup, in-place resume or hardware
management. See [Phase II](phase-ii.md) for transfer and journal semantics.

## Supported actions

1. **Retain the existing tree.** After a failure, cancellation or process crash,
   keep staging, published games and diagnostic records intact. Stop using that
   destination for further transfers. Never remove or rename an existing `USB`
   entry merely to satisfy the fresh-tree requirement.
2. **Inspect diagnostics.** Run `usb inspect-transfer --destination PATH --json`.
   This obtains the destination lock and reads only the designated journal. If
   another cooperating transfer holds the lock, inspection fails; wait for it to
   stop. Do not bypass the lock. A missing, malformed or truncated journal requires
   operator investigation; the Bureau does not repair or salvage it automatically.
3. **Retry into a fresh destination.** Choose another ordinary temporary directory
   and run `copy-to-directory` again. It must not already contain a `USB` entry.
   The engine scans current source metadata and checks approval, identity, hashes,
   capacity and collisions again. This is a new transfer, with no reuse of the old
   staging or journal. Keep the previous destination intact for inspection.

These actions concern local test directories. No action authorises physical USB
removal, mounting, formatting or filesystem repair.

## Interpreting evidence

| Evidence | Meaning and permitted response |
|---|---|
| Live report lists a completed game | Copy, verification, publication, required directory syncs and commit sync succeeded at that time. Preserve it even if a later game or terminal record fails. |
| Live report names an unconfirmed game | It may be staged or already published. Retain it; do not count it as delivered or reward a visitor for it. |
| Journal contains a publication intent without commit | Publication may or may not have happened. Retain it; inspection does not resolve durability. |
| Journal contains a commit or terminal completion | The record is readable. Its sync may have failed; it is not proof of delivery, current integrity or ownership. |
| Journal has no terminal record | Transfer state is interrupted or unknown, including when all commit lines are readable. |
| Journal is absent, invalid or torn | Automated interpretation stops. Preserve all entries and investigate outside this prototype. |

Existence under `USB/games/`, a matching stable ID or a matching checksum is not
sufficient to establish ownership or grant permission to delete. Neither is
`destination_write_attempted`, a journal claim, an inode number or the absence of
a file from the current catalogue. A sync error must not trigger rollback of a
successful rename. Read-back cannot prove that a previous sync succeeded.

## Proposals for later operator recovery

The following capabilities are **not implemented**. Keep retention and fresh-
destination retry as the available policy until their requirements are resolved.

- A read-only recovery review could correlate a station-controlled transfer record
  with an explicitly selected destination and identify uncertainty. It would need
  to bind the particular transfer, destination and expected tree; a visitor-writable
  marker alone would not establish ownership. Device identity and offline record
  retention remain open design questions.
- A future operator action could reverify a positively identified, already published
  game against current approved source content. It would need new hashes and full
  tree checks and would record a new verification result. It must not silently
  convert an old commit claim into a confirmed historical delivery or cooldown.
- Any proposed cleanup would need narrowly defined ownership evidence, explicit
  operator selection of exact entries, exclusive destination access, fresh handle
  and tree revalidation, and a separately tested recovery policy. Reject ambiguity
  or changed entries. No broad recursive deletion or permission inferred from
  a write-attempt flag is acceptable.
- An in-place retry would need an independently reviewed collision policy while
  keeping whole-game and no-overwrite guarantees. Current transfers continue to
  require a fresh tree.

Before implementing these proposals, add tests for forged/missing/stale records,
destination replacement, unknown entries, changed approval, interrupted recovery
and concurrent transfers. Hardware power-loss tests need an isolated disposable
filesystem environment. Unit fault injection and abrupt process exits do not
validate real media, controller caches or physical safe-removal behaviour.
