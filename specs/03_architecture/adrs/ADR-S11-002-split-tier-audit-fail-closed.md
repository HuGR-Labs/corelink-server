---
id: "ADR-S11-002"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s11", "audit", "fail-closed", "split-tier"]
---

# ADR-S11-002 — Split-Tier Audit Fail-CLOSED Discipline

## Status

Accepted — 2026-05-13. Cross-WI reference (S-11 only; not back-applied
to S-10 billing fail-OPEN per Lote 10.6bis canonical split-tier).

## Context

Lote 10.6bis established the canonical split-tier discipline:

| Tier | Audit failure behavior | Rationale |
|---|---|---|
| **Billing** (S-10 usage_event_emit) | **fail-OPEN** | Customer hot path; dropping an event = miss revenue but request succeeds; backed by 3-layer reconciliation + 24h replay forensics |
| **Privacy** (S-11 DSR pipeline) | **fail-CLOSED** | Regulatory-grade; audit failure must abort the operation; LGPD Art. 37 / GDPR Art. 30 records of processing |

S-11 needs an unambiguous statement of fail-CLOSED semantics across
the 8 WIs (DSR API, erasure worker, consent ledger, notice, sub-
processor, breach, residency, DPIA).

## Decision

**S-11 audit emit failure aborts the operation at the trait surface**:

- `AuditSink::emit(...)?` failure propagates as `ErasureError::Audit`
  / `ConsentLedgerError::Audit` / etc.
- The orchestrator MUST emit the audit BEFORE any state mutation that
  cannot be replayed-safe.
- Where a mutation IS replay-safe (per-backend tombstones with
  UNIQUE `(dsr_id, backend)` constraint), the audit precedes the
  TOMBSTONE write, not the mutation itself. This is the canonical
  pattern for `corelink-privacy-erasure-worker::orchestrator`:
  `started.v1` → fan-out → per-backend `adapter.erase()` → per-backend
  `backend_completed.v1` → per-backend tombstone insert. Audit failure
  at the per-backend `backend_completed.v1` step aborts before the
  replay-safe tombstone, so a re-run will re-fire and re-emit.

## Rationale

Distinct from S-07 P1-1 (reservation pre-emit) where the mutation site
itself is reversible (release reservation). Here the backend mutation
is partially irreversible (Stripe customer pseudonymize, R2 mutable
DELETE) but the **regulatory observability guarantee** is satisfied at
the DSR level via `dsr.erasure.started.v1` (emitted before fan-out)
plus the 24h verification job posting EVT-048 evidence with 7y
retention. The per-backend events are best-effort observability rather
than the canonical regulatory record.

## Consequences

**Positive**: clear contract for all 8 S-11 WIs. INV-AUDIT-APPEND-ONLY
preserved (audit log itself is append-only). FailingAuditSink chaos
fixtures test the abort semantics deterministically.

**Negative**: per-backend audit emit failure means the operation is
visible at DSR-level (`started.v1`) but not at per-backend granularity
until replay. Customer-visible status reflects this: "started; 1+
backends pending audit emit retry".

**Forbidden**: S-11 audit sinks MUST NOT silently swallow errors
(fail-OPEN is reserved for S-10 billing per Lote 10.6bis canonical).

## References

- Lote 10.6bis split-tier canonical
- S-07 P1-1 reservation pre-emit fix
- WI-S11-001 §6.1.5, WI-S11-002 §9.3 DD-005, WI-S11-003 §9 cascade
- INV-AUDIT-APPEND-ONLY (CRITICAL, invariant_registry.md §3.6)
- failure_modes.md FM-105, FM-450, FM-452
