---
type: "StorageComponent"
title: "CAS hot-path D1-over-HTTP latency"
description: "The measured root cause of slow /v1/cas: two synchronous D1-over-HTTP control-plane round-trips before the R2 GET — NOT Argon2id — plus the remediation plan and its correctness invariants."
source_files:
  - "crates/corelink-container/src/billing_d1_http.rs"
  - "docs/perf/2026-06-19-cas-hot-path-latency.md"
checkpoint_sha: "175a91320376cd80ada9797944e118ec1ff81c63"
provenance: "AUTHORED"
tags: ["storage", "perf", "d1", "cas", "latency", "hot-path"]
timestamp: "2026-06-26T00:00:00Z"
---

# CAS hot-path D1-over-HTTP latency

`GET/PUT /v1/cas/{tenant}/{key}` measured 3-7s in production, and both the original handoff and a first
code-trace blamed Argon2id. Direct measurement disproved that. The real cost is structural: the request
is served by the native container (not the wasm Worker), and the container's per-request hot path makes
**two synchronous D1-over-HTTP control-plane round-trips** — the [$-ceiling quota charge](/flows/billing-quota-check.md)
and the 410-Gone tombstone check against the [D1 CONFIG_DB](/storage/d1-config-db.md) — **before** the
R2 GET. Each hop crosses the same sync↔async `block_in_place` bridge the billing writer uses, because the
native container reaches D1 only over the slow REST control-plane API. This concept records the measured
evidence and the fix plan so the latency is not re-misattributed to crypto. It is the storage-latency
view of the [native CAS surface](/surfaces/native-cas.md) running on the [container plane](/planes/container.md).

# Role
- The measured root-cause record + 4-work-package remediation plan for the CAS latency blocker
  (`docs/perf/2026-06-19-cas-hot-path-latency.md:9-16`).
- The production D1-over-HTTP writer whose `block_in_place` bridge is the same per-hop cost each D1 round
  -trip pays (`crates/corelink-container/src/billing_d1_http.rs:1-28`).

# How it works
1. `/v1/cas` is served by the native container, whose warm hot path runs two synchronous D1-over-HTTP
   round-trips (quota charge + tombstone) before the R2 GET
   (`docs/perf/2026-06-19-cas-hot-path-latency.md:9-16`).
2. The per-step measurement attributes ~0.3-0.7s to each D1-over-HTTP hop and ~0.05-0.15s to the R2 GET
   (`docs/perf/2026-06-19-cas-hot-path-latency.md:40-53`).
3. Measured prod latency is ~4s cold, ~1.5s warm steady-state; warm requests are PAT-gate cache hits that
   already SKIP Argon2id, proving Argon2id is not the warm dominator
   (`docs/perf/2026-06-19-cas-hot-path-latency.md:18-32`).
4. Each D1 statement is driven through `block_in_place` + `Handle::current().block_on` because the trait
   surface is sync but native D1 access is async over REST
   (`crates/corelink-container/src/billing_d1_http.rs:90-114`).
5. Remediation is four separate-PR work-packages: bulk PUT, drop the two D1 hops, keep the container
   warm, and fast-hash PATs (`docs/perf/2026-06-19-cas-hot-path-latency.md:55-94`).

# Invariants
- The sync billing/control-plane trait stays sync (shared with the wasm Worker); `block_in_place` on the
  multi-thread runtime is the single documented bridge point — no nested runtime
  (`crates/corelink-container/src/billing_d1_http.rs:90-114`).
- Any D1 transport / non-2xx / decode error maps to `Transient` → HTTP 500 so the caller retries; the
  store is fail-CLOSED, never silently treating a failed read as success — the executing
  `.map_err(|e| BillingD1Error::Transient(..))` is at `crates/corelink-container/src/billing_d1_http.rs:113`.
- WP-2 must keep the $-ceiling fail-CLOSED with a bounded, reconciled overshoot — a charge is NEVER lost
  and a tenant definitely over-ceiling is still refused
  (`docs/perf/2026-06-19-cas-hot-path-latency.md:68-80`).
- The tombstone fast-path structure may have false positives (extra D1 check — safe) but NEVER false
  negatives — serving an erased object as 200/404 instead of 410 is a GDPR Art.17 breach
  (`docs/perf/2026-06-19-cas-hot-path-latency.md:74-80`).

# Gotchas
- Argon2id is a red herring for the WARM path — it only runs on a PAT-gate cache miss, and the steady
  ~1.5s is measured WITH it already skipped. Do not "optimize" by weakening Argon2id; the win is dropping
  the D1-over-HTTP hops (WP-2) and cold-start (WP-3).
- The 1.5s floor is a control-plane-locality problem, not a network or transfer one: `connect` is ~40ms
  and a 404 has no body, so the time is all server-side D1 compute.

# Citations
1. `docs/perf/2026-06-19-cas-hot-path-latency.md:9-16` — TL;DR: two synchronous D1-over-HTTP round-trips before R2; Argon2id not dominant.
2. `docs/perf/2026-06-19-cas-hot-path-latency.md:18-32` — measured evidence: ~4s cold / ~1.5s warm; cache hits skip Argon2id.
3. `docs/perf/2026-06-19-cas-hot-path-latency.md:40-53` — per-step hot-path cost table (quota + tombstone D1 hops vs R2 GET).
4. `docs/perf/2026-06-19-cas-hot-path-latency.md:55-94` — the 4 remediation work-packages + their invariants.
5. `docs/perf/2026-06-19-cas-hot-path-latency.md:68-80` — WP-2 $-ceiling fail-closed + tombstone no-false-negative invariants.
6. `docs/perf/2026-06-19-cas-hot-path-latency.md:74-80` — GDPR Art.17 tombstone false-negative prohibition.
7. `crates/corelink-container/src/billing_d1_http.rs:1-28` — the sync↔async D1-over-HTTP bridge rationale.
8. `crates/corelink-container/src/billing_d1_http.rs:113` — fail-CLOSED transport-error → `Transient` → 500 (the executing `.map_err`).
9. `crates/corelink-container/src/billing_d1_http.rs:90-114` — `run`: `block_in_place` + `block_on` per D1 round-trip.
