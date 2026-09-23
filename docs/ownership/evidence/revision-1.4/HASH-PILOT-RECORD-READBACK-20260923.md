# `corelink-hash` pilot record feasibility — 2026-09-23

Decision: **BLOCKED; no `docs/ownership/records/corelink-hash.json` authored.**
This is a readback of existing evidence, not an author validation or cold review.

The package identity is available: repository ID `1232040291`, package
`corelink-hash`, manifest `crates/corelink-hash/Cargo.toml`, skill slug
`own-corelink-hash`, frontmatter evidence set `hash-pilot-source-20260919`,
and source pin `cacc44fc43ee2f481e933419ba88b9a19ac6c8e8`. The scoped
source-identity file records that pin and explicitly limits itself to SOURCE
evidence. The current calibration readback measures 43 relations, five
implementation source files and six procedures, supporting profile H as a
capacity decision only.

The four current artifact SHA-256 values are:

| Artifact | Current SHA-256 | Latest reported cold-review SHA-256 |
|---|---|---|
| Skill | `5811f2415fd38ac29ba8378559fc844d0a2c709f08d3b47e28c386f1e3e4054c` | `48c2a3c3d21ac762c13a9a89e15efa80f4f5070947c2d921323495c5c275ba9f` |
| Reference | `86a578a0b750125f7409ea64b8595dbea7c2a726a264c0417ccce5ec0582855e` | same |
| Blast radius | `2d5f44b617577c7cfff4328be35e344c71dfed28ceb424c3e6de646d526f1547` | `b37edf4552c8ec5c1b8b71a6093aa68dbe4cbc25e90e5b112dbe556d08db666d` |
| Maintenance | `0092be79326c6bec486b163a3ec71ba32436af801d68df7bce1c18cb5b30af86` | same |

The 2026-09-23 peer-reconciliation review reports the rightmost hashes but
does not cover the current skill or blast bytes. A diff against its reported
checkout `247af6c5c36f43d49a4bdb75ae14706a45763431` shows a changed skill
decision row and changed blast relation population, identities and peer
descriptions, including REL-043. These are body/semantic changes, so the
`STANDARD.md` §9.3 metadata-only exception cannot preserve that review. No
current-byte cold review or immutable REVIEW snapshot plus field-specific
metadata readback was found for those two files. The earlier 2026-09-22
current-byte review gave blast radius `BLOCKED`; the later scoped review
approved only the then-current peer reconciliation. Neither proves approval
of this four-file set.

Required record fields not supported by the available evidence include:

- `author.identity` and `author.session_id` for the actual artifact author.
  Git commits and report filenames do not attest to that session.
- A current-byte `review.reviewer`, `review.session_id`,
  `fresh_context_confirmed`, `independence_evidence` and exact
  `reviewed_at`, together with four per-artifact `verdict`, `checks`,
  `findings` and `limitations` bound to the current hashes. The available
  reports identify some reviewer agent paths and dates but do not supply a
  complete current-byte record for this set.
- `semantic_review` snapshot evidence and `metadata_readback` reader,
  session, timestamp, changed fields and REVIEW evidence, if a future
  frontmatter-only normalization needs that exception. Those fields cannot
  repair the present semantic changes.
- The record's complete `discovery.rules` and measured
  `discovery.snapshot_sha256`, `build_selections`, structured 43-entry
  `relations` with contract fingerprints, and six `procedures` with
  per-procedure review/execution evidence. Source and calibration notes are
  useful inputs, but they are not a reviewed record for these fields.

The JSON schema has no record-level `state` property, so adding a literal
`AUTHOR_VALIDATED` or `BLOCKED` state would violate it. Its required review
object cannot be omitted or filled with placeholders. The current
`ownership_gate.py` also requires fresh independent review, four `APPROVE`
verdicts and passing checks, and therefore cannot report
`EVIDENCE_CONSISTENT` for a truthful non-approved record. A structurally
populated `BLOCKED` JSON would appear as `INVALID_REVIEW_RECORD` in the
registry, not demonstrate metadata-only readback.

To make a real record reviewable, first stabilize the four artifact bytes and
the source/peer claims, capture author and independent reviewer provenance,
review all four current hashes, and populate the source, discovery, relation,
selection and procedure evidence. Record any later frontmatter-only change
with the exact immutable reviewed bytes and a separate current-byte readback.
No runtime, deployment or approval claim is inferred from the existing local
Cargo reports.

Validation of this readback: `check_docs.py --profile H` returned
`IMPLEMENTED_CHECKS_PASS` with zero structural errors for each of the four
current artifacts. `git diff --check` passed for this report, and
`docs/ownership/records/corelink-hash.json` is absent. The ownership record
gate requires an input record, so it was not run against a fabricated one.
