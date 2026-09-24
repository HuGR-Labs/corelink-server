# Finalization wave plan — 2026-09-23

Baseline for this wave: `3df52eb71acdd4e00084d42f0b64191674bf42eb`.
Latest observed `origin/main`: `e0f231110524fe81ed0b8d3451903879daf9d519`.

## Acceptance items

- A1: adapter-host, rate-headers and reapi public contracts contain exact symbols, signatures, behavior, errors and evidence.
- A2: handler-customer relations are atomic for materially distinct endpoints/flows and have reciprocal/backlink coverage.
- A3: every tracked `Cargo.toml` is classified and reconciled against the 105 prepared identities, with explicit exclusions.
- A4: the candidate standard has no unresolved structural or contract blocker for freeze, and all required gates are named.
- A5: open/closed issue and backlog reconciliation is read back package-by-package without publishing during the preflight.
- A6: changed bytes receive independent cold review; registry, documentary tests, diff check and final audit remain green.

## Work packages

| WP | Owner files | Mode | Dependency | Return shape |
|---|---|---|---|---|
| W1 | `docs/ownership/crates/{corelink-adapter-host,corelink-rate-headers,corelink-reapi}/REFERENCE.md` | edit | none | changed files, checks, unresolved blockers |
| W2 | `docs/ownership/crates/corelink-handler-customer/{REFERENCE,BLAST_RADIUS,MAINTENANCE}.md` | edit | none | changed files, checks, unresolved blockers |
| W3 | new evidence report only | read-only | none | manifest census counts, classifications, discrepancies |
| W4 | new evidence report only | read-only | none | standard freeze gate verdict and blockers |
| W5 | new evidence report only | read-only | none | duplicate/backlog preflight and publication blockers |

The work packages are conflict-free because no package document is shared. Registry/index/status
regeneration is intentionally serial after W1/W2. Publication is out of scope for this wave.

## Global done gate

All acceptance items must be evidenced. No approval is inferred from a structural checker alone;
no Cargo/runtime/deployment claim may be upgraded without execution evidence. Any changed artifact
requires a fresh cold review before freeze or publication.

## Wave 2 — cold-review fixes

- W6: align the three W1 references with the exact R01–R08 standard mapping, correct the
  stale repository identity, and document the OCI storage-cap behavior.
- W7: close the W2 cold-review findings for canonical routing, public contracts, direct and
  inverse relation census, atomic SLI/method boundaries, validation commands and recovery.
- W8: repair the standard/schema/gate/index contract for metadata-only readback and review/publication
  state derivation; do not publish issues in this wave.
- W9: refresh pilot calibration from current bytes and reconcile the server manual measurement
  without changing implementation claims.
- W10: re-anchor or explicitly stale-mark packages affected by current-main Cargo/lockfile drift
  before any freeze or publication decision.
