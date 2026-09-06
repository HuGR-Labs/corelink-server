---
type: "ADR"
title: "ADR-0100 — Object-Lock capability gate for Compliance retention"
source_files:
  - "specs/03_architecture/adrs/ADR-0100-r2-object-lock-capability-gate.md"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
source_blobs:
  - "specs/03_architecture/adrs/ADR-0100-r2-object-lock-capability-gate.md@783a4739c9dcdd5a90b9a12f40f5084ef46dc4bb"
tags: ["adr", "object-lock", "retention", "r2", "legal-hold"]
---
# ADR-0100 — Object-Lock capability gate for Compliance retention

This ADR is deliberately deferred while R2 returns `NotImplemented` for the
bucket-level and per-object S3 Object-Lock operations. The authoritative
decision, probe procedure, and current Governance/legal-hold boundary are in
`specs/03_architecture/adrs/ADR-0100-r2-object-lock-capability-gate.md` and
`docs/knowledge/ops/r2-object-lock-probe.md`. No Compliance guarantee is live.

## Citation

The decision and its evidence boundary are recorded in
`specs/03_architecture/adrs/ADR-0100-r2-object-lock-capability-gate.md:36-72`.


# Revalidation

This concept was revalidated against the cumulative implementation tree; its existing source citations remain the controlling evidence for the behavior described above.
