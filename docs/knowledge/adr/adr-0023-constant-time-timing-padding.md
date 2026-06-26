---
type: "ADR"
title: "ADR-0023 — Constant-time 404 MissReason timing-padding middleware"
description: "Why CAS 404 responses are padded to a uniform p99 latency so the three MissReason compute paths cannot be timed apart into a cross-tenant existence oracle."
source_files:
  - "specs/03_architecture/adrs/ADR-0023-constant-time-timing-padding.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "side-channel", "timing", "cas", "tenant-isolation", "s02"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0023 — Constant-time 404 MissReason timing-padding middleware

ADR-0028 makes every CAS read miss return an identical HTTP 404, but the three underlying `MissReason` variants reach that 404 over compute paths of very different cost — a KV negative-cache hit (`NeverExisted`), a D1 soft-delete check (`Tombstoned`), and a full D1+R2 round-trip (`R2OrphanRow`). This ADR closes the residual *timing* oracle that the uniform status code alone leaves open, so a tenant cannot enumerate other tenants' blobs (or detect soft-deleted ones) by measuring latency. It is the timing-layer half of the pair whose status-layer half is ADR-0028.

# Context

The S-02 read path resolves a miss into one of three `MissReason` arms whose detection paths differ in cost: a fast KV/absent-row path, a medium soft-delete path, and a slow D1-alive + R2-GET path (ADR-0023:25-39). Because ADR-0028 already collapses all three to a uniform 404 body, an attacker who authenticates in their own tenant can probe candidate digests and cluster the response-latency distributions to recover which `MissReason` applies — an existence oracle that indirectly breaks tenant isolation (threat THR-I-002).

# Decision

A Tower middleware `TimingPaddingLayer` pads **only** 404 responses to a configurable target (200 ms p99) with seeded ±10% jitter using `tokio::time::sleep_until` rather than a CPU spin, and deliberately does not pad 200/403/413/429/5xx where padding buys no security (ADR-0023:43-56). Indistinguishability is enforced as a CI statistical proof gate: a 3-arm Mann-Whitney U methodology (10k samples × 3 arms × 3 trials, Šidák-corrected) plus a bootstrap 95% CI on the pairwise median difference bounded at ≤ 1 ms (ADR-0023:57-63). Trade-offs such as constant-time D1 queries and adaptive ML padding were rejected (ADR-0023:84-96).

# Consequences

The cross-tenant existence oracle is closed at the timing layer and `INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE` is enforced with evidence-grade methodology, at the cost of a ~150 ms average latency tax applied to roughly 10% of requests (the 404 non-fast-path slice) while CF Worker CPU cost stays at $0 because padding is a sleep timer, not a spin (ADR-0023:70-82). It pairs with the status-layer freeze in [ADR-0028](/adr/adr-0028-missreason-uniform-404-freeze.md) and governs the [native CAS surface](/surfaces/native-cas.md).

# Citations

1. `specs/03_architecture/adrs/ADR-0023-constant-time-timing-padding.md:25-39` — the three MissReason compute paths and the timing-enumeration threat (Context).
2. `specs/03_architecture/adrs/ADR-0023-constant-time-timing-padding.md:43-56` — pad only 404, seeded-jitter sleep, no padding on other status codes (Decision).
3. `specs/03_architecture/adrs/ADR-0023-constant-time-timing-padding.md:57-63` — the 3-arm Mann-Whitney + bootstrap-CI statistical proof gate (Decision).
4. `specs/03_architecture/adrs/ADR-0023-constant-time-timing-padding.md:70-82` — oracle closed; ~150 ms tax on ~10% of requests; $0 CPU (Consequences).
