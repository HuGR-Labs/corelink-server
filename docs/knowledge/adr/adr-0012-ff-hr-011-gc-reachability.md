---
type: "ADR"
title: "ADR-0012 — FF-HR-011 forcing factor for GC / reachability changes"
description: "Adds a dedicated high-risk forcing factor so any change to garbage-collection, refcount, or the reachability invariant is automatically routed to the HIGH_RISK lane."
source_files:
  - "specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "risk-lanes", "garbage-collection", "reachability", "framework"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0012 — FF-HR-011 forcing factor for GC / reachability changes

CoreLink's risk-lane framework classifies every work item by its blast radius; a misclassified GC change is dangerous because deleting a still-reachable blob is a silent cross-tenant data-integrity break. This ADR closes a gap where GC/refcount work could slip through as a STANDARD lane because no forcing factor matched its semantics — so it adds `FF-HR-011` to make that routing automatic. It matters as the governance hook that forces TLA+ + chaos testing onto exactly the changes that can corrupt the cache.

# Context

An audit (Lote 3+4) found the product profile proposed `FF-HR-011` but the framework only defined `FF-HR-001..010`, leaving GC/refcount changes able to be treated as a STANDARD lane and the CRITICAL `INV-GC-001` ("reachable never deleted") with no precisely-matching forcing factor (`specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:30`). Failure modes for refcount/race in GC (`FM-300`, `FM-303`, `FM-404`) were left uncovered by the risk-lane system (`specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:34`).

# Decision

Add `FF-HR-011` to the framework: any change to a GC algorithm, refcount, or reachability invariant (`INV-GC-*`) with data-integrity blast radius is a high-risk forcing factor (`specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:38-40`). The alternatives of reusing `FF-HR-005` (security) or `FF-HR-006` (retention policy) were rejected as semantically imprecise, hiding the real risk (`specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:46-47`).

# Consequences

- Any WI touching the GC algorithm is automatically `HIGH_RISK`, requiring TLA+, 10–12 sign-offs, and chaos testing (`specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:53`).
- Cost is a minor framework version bump and heavier ceremony on future GC work — accepted as the correct ceremony for the risk (`specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:56-57`).
- Related re-indexed decisions: [ADR-0013](/adr/adr-0013-promote-remote-cache-canonical.md) (canonical sources) and [ADR-0014](/adr/adr-0014-sbom-format-cyclonedx.md).

# Citations

1. `specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:30` — the missing forcing factor and the STANDARD-lane risk it created.
2. `specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:34` — uncovered GC refcount/race failure modes.
3. `specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:38-40` — the decision: add FF-HR-011.
4. `specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:46-47` — rejected reuse of FF-HR-005 / FF-HR-006.
5. `specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:53` — HIGH_RISK routing consequence.
6. `specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md:56-57` — the accepted ceremony cost.
