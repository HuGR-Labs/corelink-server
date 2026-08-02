---
type: "ADR"
title: "ADR-0042 — GC worker scheduler design + gc-pause degrade-mode contract"
description: "Per-region sticky DO GC workers on a jittered 02:00 UTC cron, partial-UNIQUE single-flight, checkpoint-idempotent resume, and a config-singleton gc-pause emergency stop."
source_files:
  - "specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md"
checkpoint_sha: "e8d2d9d5cd2b3b0c83e5c80f72862fb4a9832686"
provenance: "AUTHORED"
tags: ["adr", "gc", "worker", "scheduler", "degrade-mode", "s06"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0042 — GC worker scheduler design + gc-pause degrade-mode contract

The GC worker is the single point of failure for the cache's most dangerous invariant — reachable
data must never be deleted — so its scheduler design is load-bearing for correctness, not just ops.
This ADR is the decision record that pins the topology, concurrency control, crash-recovery, and
emergency-stop contract for garbage collection, which is the architectural backdrop for the
[operations crate cluster](/crates/operations.md). Two later addenda also make this file the home of
the TLA+ tooling pin and the formal-verification scope statement for GC.

# Context

The S-06 GC worker is a single point of failure for INV-GC-001 (reachable never deleted), so the
scheduler design must address per-region cron scheduling with jitter, sticky DO per region, idempotent
re-run via checkpoint, single-running-per-(tenant, region) concurrency, monotonic phase transitions, a
global `gc-pause` degrade-mode, and a manual admin trigger.

# Decision

The design is **per-region sticky DO + a 02:00 UTC daily cron with ±10min jitter + degrade-mode
`gc-pause` via a DO config-singleton + a manual admin trigger deferred to S-13.** Concurrency is made
race-free with a partial UNIQUE index `WHERE status='running'` (the Lote 10.5bis lesson), crashes
resume from a per-batch checkpoint, phases are tracked monotonically by a CHECK-constrained enum, and
degrade-mode is probed per batch boundary so a `gc-pause` propagates within ≤100ms. A single global
cron, no-jitter firing, hash-based concurrency control, a monolithic non-phase-tracked worker, and
synchronous degrade-mode were all rejected. Addendum §A1 additionally pins the TLA+ `tla2tools.jar`
v1.8.0 SHA-256 in CI — and since 2026-08-02 that pin is enforced by `check_tlc_pin_consistency.py`
rather than by prose, because a 2026-07-09 bump skipped the addendum and left the ADR and all eight
CI carriers disagreeing for three weeks with nothing failing. §A3 scopes the formal proof to the
mark-sweep algorithm (the soft-delete grace window is covered architecturally, not by TLA+).

# Consequences

The per-region scheduler scales horizontally with no cross-region coordination, idempotent resume
handles crashes, the partial UNIQUE prevents races, and degrade-mode propagation is bounded; the costs
are that a single fat tenant's GC runs sequentially per region (phase-budget overruns alert SEV-2) and
the manual admin trigger is a forward-S-13 staging stub that returns 501 in prod.

# Citations

1. `specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md:24-34` — Context: GC worker is the single point of failure for INV-GC-001 + the seven design concerns.
2. `specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md:37-37` — Decision: per-region sticky DO + jittered 02:00 cron + config-singleton gc-pause + S-13 manual trigger.
3. `specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md:41-59` — the architectural-choices table (partial UNIQUE single-flight, checkpoint resume, monotonic phases, ≤100ms degrade-mode) and the rejected alternatives (single global cron, no-jitter, hash-based concurrency, monolithic worker, synchronous degrade-mode).
4. `specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md:63-72` — Consequences: horizontal scale + idempotent resume vs sequential per-region GC and the 501 admin stub.
5. `specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md:392-408` — §A3 addendum: the TLA+ formal-verification scope (mark-sweep proven; grace window covered architecturally).
6. `specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md:87-95` — §A1 addendum: pins the TLA+ `tla2tools.jar` v1.8.0 + SHA-256 in CI, and makes that pin enforceable by a CI check rather than prose.
