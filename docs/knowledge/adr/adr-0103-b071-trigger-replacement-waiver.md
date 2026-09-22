---
type: "ADR"
title: "ADR-0103 — Name-scoped B-071 accounting trigger replacement"
source_files:
  - "specs/03_architecture/adrs/ADR-0103-b071-trigger-replacement-waiver.md"
tags: ["adr", "d1", "migration", "gc", "b-071", "additive"]
---

# ADR-0103 — Name-scoped B-071 accounting trigger replacement

Migration `0143` may replace only the two named B-071 accounting triggers.
The additive verifier rejects all other destructive statements there.
Fresh installs use `0142`; deployed `0118` installations upgrade through
`0143`. No production apply is authorized.

See the [authoritative ADR](../../../specs/03_architecture/adrs/ADR-0103-b071-trigger-replacement-waiver.md).
