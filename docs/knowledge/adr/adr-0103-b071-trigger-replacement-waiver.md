---
type: "ADR"
title: "ADR-0103 — Name-scoped B-071 accounting trigger replacement"
source_files:
  - "specs/03_architecture/adrs/ADR-0103-b071-trigger-replacement-waiver.md"
source_blobs:
  - "specs/03_architecture/adrs/ADR-0103-b071-trigger-replacement-waiver.md@d7731ad0cdc4862b0dda4eb313dc5ab6bfc78b21"
checkpoint_sha: "d987926e18f3816dc95ba8a951cdd2248c32c825"
tags: ["adr", "d1", "migration", "gc", "b-071", "additive"]
---

# ADR-0103 — Name-scoped B-071 accounting trigger replacement

Migration `0143` may replace only the two named B-071 accounting triggers.
The additive verifier rejects all other destructive statements there.
Fresh installs use `0142`; deployed `0118` installations upgrade through
`0143`. No production apply is authorized.

## Citations

1. `specs/03_architecture/adrs/ADR-0103-b071-trigger-replacement-waiver.md:24-40` — Decision: the exact two-trigger exception, its migration boundaries, preserved accounting invariants, hosted replay coverage, and the requirement for a new decision for later migrations.
