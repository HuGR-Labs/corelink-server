---
type: "ADR"
title: "ADR-0013 — Promote REMOTE-CACHE-PRODUCT-PROFILE to a Level-3 canonical source"
description: "Formally promotes the remote-cache product profile, the invariant registry, and the key-management doc to Level-3 canonical sources so inheritance is de jure, not just de facto."
source_files:
  - "specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "canonical-source", "inheritance", "framework", "governance"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0013 — Promote REMOTE-CACHE-PRODUCT-PROFILE to a Level-3 canonical source

CoreLink's spec framework lets work items inherit from canonical sources, but a doc that is referenced by `inherits_from` without being *registered* as canonical leaves every inheriting WI ambiguous (was it promoted? still DRAFT?). This ADR resolves that by formally promoting three architecture docs to Level-3 canonical status, making their inheritance legitimate and giving the spec validator a real anchor. It matters because it is the governance act that lets `validate_specs.py` trust the CAS/AC/GC/BYOK inheritance chain.

# Context

An audit found `remote_cache_product_profile.md` used normative `inherits_from` language but was not listed as a canonical source in the framework, was typed as a generic `protocol`, and lacked the `doc_status: FROZEN` the inheritance rule (REG-INHERIT-001) requires — so it acted as a canonical source de facto but not de jure (`specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:30`). The invariant registry and the key-management (BYOK) doc, created in the same lote, also needed canonical status (`specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:37`).

# Decision

Promote three docs to Level-3 canonical sources: the remote-cache product profile (canonical for CAS, AC, GC, dedup, eviction, REAPI conformance), the invariant registry (canonical for all `INV-XXX` IDs), and the key-management doc (canonical for KMS/BYOK/BYOE/rotation) (`specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:41-47`). Rejected alternatives were keeping it as a generic `protocol` (stays ambiguous) and merging everything into `data_model.md` (violates separation of responsibilities) (`specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:53-55`).

# Consequences

- `inherits_from: ["REMOTE-CACHE-PRODUCT-PROFILE"]` becomes semantically valid and the invariant registry centralizes naming (`specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:60-62`).
- Cost is two more canonical docs to maintain and a reading prerequisite before touching CAS/AC (`specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:64-65`).
- Sibling decisions in this lote: [ADR-0012](/adr/adr-0012-ff-hr-011-gc-reachability.md) and [ADR-0014](/adr/adr-0014-sbom-format-cyclonedx.md).

# Citations

1. `specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:30` — the de-facto-not-de-jure canonical-source gap.
2. `specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:37` — registry + key-management docs also needing promotion.
3. `specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:41-47` — the decision: promote three docs to Level-3.
4. `specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:53-55` — rejected alternatives.
5. `specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:60-62` — inheritance now valid; registry centralizes naming.
6. `specs/03_architecture/adrs/ADR-0013-promote-remote-cache-canonical.md:64-65` — maintenance cost.
