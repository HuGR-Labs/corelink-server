# Standard contract repair — 2026-09-23

Scope: `STANDARD.md` §9.3/§10, package-record schema, ownership gate,
population registry generator and focused documentary tests. This report records
mechanism behavior, not an independent cold-review verdict or freeze approval.

## CR-STD-01: metadata-only normalization

The schema now represents `semantic_review` and `metadata_readback` separately.
For a metadata-only exception, the record must retain the reviewed artifact
bytes as `REVIEW` evidence, the reviewed SHA-256, the current SHA-256, the exact
changed frontmatter fields, reader/session/time and readback evidence. The gate
compares the two files after removing only the permitted frontmatter lines;
all remaining bytes must match. Semantic body edits, missing snapshots, stale
hashes, other frontmatter edits and missing review evidence fail the gate.
The ordinary path still requires exact reviewed bytes.

Synthetic fixture tests cover a passing source-pin normalization, a failing
body edit and a missing snapshot/evidence. These tests do not assert that any
real package approval can be reused.

## CR-STD-02: registry states

The population generator reads `docs/ownership/records/<package>.json` when
present, validates it through `ownership_gate.record_errors`, and then records
`REVIEW_EVIDENCE_CONSISTENT` with the four recorded verdicts. Missing records
remain `UNVERIFIED`; invalid records become `INVALID_REVIEW_RECORD`. The label
does not certify reviewer independence or semantic correctness.

Publication state comes from the ledger. `CONFIRMED`/`REUSED` require a pinned
issue readback file, valid issue identity, exact body hash/marker and matching
ledger issue number/URL before the registry reports `PUBLISHED`/`REUSED`.
Absent readback is `READBACK_REQUIRED`; mismatches are `INVALID_READBACK`.
The current ledger has zero confirmed publications and therefore remains at
zero. Synthetic tests exercise both positive and negative readbacks.

**Residual:** no `docs/ownership/records/` directory or genuine pilot package
record exists in this checkout. The cold-review report requested a demonstration
using one real pilot; the generator supports it, but that demonstration is
`BLOCKED` until a record is authored from actual independent review evidence.
No reviewer, record or verdict was fabricated for this repair.

## CR-STD-03: calibration currentness

`generate_registry.calibration_errors` now checks the five expected pilot rows,
their profiles, all four measured line/word/byte triples and the presence of
material population, overflow and capacity decisions. The generator records
`calibration=STALE` and exits nonzero if measurements differ. A regression test
changes a copied pilot reference and confirms stale metrics are detected.
The five-pilot current-byte readback was updated separately in this wave and
passes the checker at the time of this report. Capacity evidence does not
approve the documents.

## Validation and limits

- `python3 -m unittest discover -s docs/ownership/tests -q`: **143 tests OK**.
- `git diff --check`: **PASS**.
- No Rust build, runtime operation, GitHub mutation or issue publication was
  performed by this work package.
- The independent reviewer must re-read the final standard/schema/tool bytes.
  Prior review hashes do not approve these changes.
