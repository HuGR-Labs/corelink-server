# `corelink-hash` relation-ledger reconciliation — 2026-09-22

The candidate ledger `evidence/revision-1.3/relations-candidate.json` was
reconciled against the 42 relation records in the current `BLAST_RADIUS.md`
and the existing relation supplement.

- Before: 19 relation records; REL-020..REL-042 only in the supplement.
- After: **42/42** relation records serialized in the candidate ledger.
- Stable keys, local anchors and source IDs are present for REL-001..REL-042.
- Every relation remains `peer_review=not_reconciled`; no peer, owner, runtime,
  Cargo-resolution or production claim was promoted.
- `BLAST_RADIUS.md` B06 now reports the same 42-record population.

This closes the ledger completeness gap but not peer/owner reconciliation or
the cold-review verdict. The ledger and blast-document bytes changed, so the
current four-artifact `corelink-hash` set requires a fresh independent review.
