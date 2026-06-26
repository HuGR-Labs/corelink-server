---
type: "ADR"
title: "ADR-S11-002 — Split-tier audit fail-CLOSED discipline (S-11 privacy)"
description: "Why S-11 privacy audit emit failures must abort the operation (fail-CLOSED), distinct from the S-10 billing fail-OPEN tier."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-002-split-tier-audit-fail-closed.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s11", "audit", "fail-closed", "split-tier", "privacy"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-002 — Split-tier audit fail-CLOSED discipline (S-11 privacy)

CoreLink runs a canonical split-tier audit discipline: billing audit (S-10 usage events) is
fail-OPEN — a dropped event misses revenue but the customer hot-path request still succeeds, backed by
reconciliation + replay — while privacy audit (S-11 DSR pipeline) is regulatory-grade and must be
fail-CLOSED. This ADR gives the eight S-11 WIs an unambiguous statement of those fail-CLOSED semantics:
an audit emit failure aborts the operation at the trait surface. It is the cross-WI discipline the
erasure-consistency (ADR-S11-004) and consent-schema (ADR-S11-005) decisions rely on.

# Context

Lote 10.6bis established the canonical split: billing fail-OPEN (customer hot path, revenue-recoverable
via 3-layer reconciliation + 24h replay) versus privacy fail-CLOSED (regulatory, LGPD Art. 37 / GDPR
Art. 30 records of processing). S-11 spans eight WIs (DSR API, erasure worker, consent ledger, notice,
sub-processor, breach, residency, DPIA) and needed one clear fail-CLOSED contract across all of them.

# Decision

**S-11 audit emit failure aborts the operation at the trait surface.** An `AuditSink::emit(...)?`
failure propagates as a typed error (`ErasureError::Audit` / `ConsentLedgerError::Audit` / etc.), and
the orchestrator MUST emit the audit *before* any state mutation that cannot be replayed safely. Where
a mutation IS replay-safe (per-backend tombstones with a UNIQUE `(dsr_id, backend)` constraint), the
audit precedes the *tombstone* write, not the mutation itself — the canonical erasure-worker pattern:
`started.v1` → fan-out → per-backend `adapter.erase()` → per-backend `backend_completed.v1` →
per-backend tombstone insert, where audit failure at `backend_completed.v1` aborts before the
replay-safe tombstone so a re-run re-fires and re-emits.

# Consequences

- Positive: a clear contract for all eight S-11 WIs; INV-AUDIT-APPEND-ONLY is preserved and
  FailingAuditSink chaos fixtures test the abort semantics deterministically.
- Negative: a per-backend audit emit failure leaves the operation visible at DSR level (`started.v1`)
  but not at per-backend granularity until replay; customer-visible status reflects this ("started; 1+
  backends pending audit emit retry").
- Forbidden: S-11 audit sinks must NOT silently swallow errors — fail-OPEN is reserved for S-10 billing
  per the Lote 10.6bis canonical.

# Citations

1. `specs/03_architecture/adrs/ADR-S11-002-split-tier-audit-fail-closed.md:24-35` — the Context: the
   billing-fail-OPEN vs privacy-fail-CLOSED canonical split across the eight S-11 WIs.
2. `specs/03_architecture/adrs/ADR-S11-002-split-tier-audit-fail-closed.md:37-52` — the Decision: abort
   at the trait surface, audit-before-replay-safe-tombstone, and the canonical erasure event order.
3. `specs/03_architecture/adrs/ADR-S11-002-split-tier-audit-fail-closed.md:65-77` — the Consequences:
   the deterministic chaos fixtures, the per-backend visibility gap, and the no-swallow prohibition.
