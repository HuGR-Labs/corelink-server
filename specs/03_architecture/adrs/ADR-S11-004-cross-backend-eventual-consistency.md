---
id: "ADR-S11-004"
title: "Cross-backend erasure pipeline is eventual-consistency (NOT atomic 2PC) with 24h verification window gating dsr.completed.v1"
status: "ACCEPTED"
date: "2026-04-29"
tags: ["adr", "s-11", "privacy", "erasure", "eventual-consistency", "verification"]
deciders:
  - "Gustavo Schneiter (owner / final approver)"
  - "Architect"
  - "Privacy Officer (interim Gustavo até hire)"
  - "Compliance Officer"
sup​ersedes: null
superseded_by: null
parent: "WI-S11-002"
---

# ADR-S11-004 — Cross-backend eventual consistency

## Context

The canonical 12-backend canonical erasure pipeline pós Lote 10.11.0-bis
(privacy_model.md §6.2 source-of-truth) spans 4 fundamentally
heterogeneous storage classes:

1. **Effective + transactional** — Neon multi-tabela (DELETE cascade),
   D1 (row purge), KV (key prefix DELETE).
2. **Effective + irreversible third-party APIs** — Stripe
   `Customer.update` (PII nullify; `Customer.delete` would break
   GAAP ASC 606 + LGPD Art. 16 fiscal 5y compliance — see WI-S11-002
   §28 R-004 CRITICAL).
3. **Effective + eventual consistency** — R2 CAS refcount-aware
   soft-delete + 72h GC sweep grace; R2 AC mutable; Loki cold archive
   delete API (settle delay up to 24h per S-09 R-S09-5).
4. **Pseudonymized + WORM regulatory immutability** — R2 audit Object
   Lock 7y (CTRL-AUDIT-IMMUTABILITY); Neon PITR backup 30d (no manual
   delete); R2 CAS legal_hold partition governance mode; R2 evidence-*
   buckets 7y.

A canonical 2-phase commit (2PC) orchestrator would require:
- Stripe API to support a "prepare → commit/rollback" protocol
  (it does not — Customer.update is unilateral).
- R2 Object Lock to support a "prepare → release" protocol (it does
  not — Object Lock is unilateral until retention expiry).
- Loki delete API to support a "prepare → confirm" protocol (it does
  not — `/loki/api/v1/delete` queues + the retention compaction
  consumes asynchronously with up to 24h settle delay).

Per WI-S11-002 §9.2 + §28 R-001/R-002 the canonical industry-standard
solution (OneTrust, Transcend, DataGrail) is **eventual consistency
with a verification gate** rather than 2PC.

## Decision

**Cross-backend erasure is eventual-consistency with a 24h
verification window**:

1. **Per-backend independent execution** with a tombstone serializing
   progress in the canonical D1 `dsr_erasure_log` table (UNIQUE
   `(dsr_id, backend)` constraint enforces replay-safe per
   PAT-RETRY-IDEMPOTENT-001).
2. **Per-backend canonical 5-arm outcome enum** (`erased |
   pseudonymized | partial_failure | failed | not_applicable`)
   recorded as the canonical tombstone.
3. **24h verification cron worker sweep** at `queued_at_ms + 24h`:
   per-backend re-fingerprint via the canonical
   [`BackendErasureAdapter::verification_hash`] surface; effective
   backend → 0 rows expected (hash == `CANONICAL_EMPTY_TENANT_HASH`);
   pseudonymized backend → 100% `pii_redacted=true` marker presence.
4. **Decision arms**:
   - Sweep all green → `verification_passed.v1` + `completed.v1`
     CloudEvents → `dsr_tickets.status = 'completed'`.
   - Any backend non-green → `verification_failed.v1` CloudEvent +
     SEV-1 alert per FM-450 + RB-DSR-ERASURE-INCOMPLETE runbook +
     retry-once at 48h post-original-deadline.
   - SLA elapsed (>24h) with incomplete tombstones → `SlaBreached`
     decision arm + SEV-1 alert.

## Consequences

### Positive

- **Regulatory baseline**: LGPD Art. 18 §III + GDPR Art. 17.1 "without
  undue delay" + CCPA §1798.105 + EDPB Guidelines on the right to
  erasure all satisfied via the canonical 24h verification gate
  (well within the 30d / 1mo / 45d statutory windows).
- **Operational tractability**: per-backend independent retry loops
  for transient transport failures (R2 503 / Stripe rate-limit / Loki
  timeout) without rolling back the entire 12-backend pipeline.
- **Forensic trail**: every pipeline run produces a canonical signed
  report (`erasure-report.json` JCS-canonical + BLAKE3-keyed MAC) in
  R2 evidence-dsr-`<region>` retain 7y per EVT-048; auditor evidence
  trail is byte-deterministic across re-runs.

### Negative

- **24h-ish customer perception**: customer receives the
  canonical `verification_passed` notification 24h post the
  `started` notification (vs instant on a transactional system). DPA
  + privacy notice document the verification window explicitly.
- **Partial failure exposure**: a single unhealthy backend
  (Loki settle delay, R2 503 burst) blocks the canonical
  `dsr.completed.v1` until the next 48h retry sweep clears. SEV-1
  alert path keeps the Privacy Officer informed; canonical
  RB-DSR-ERASURE-INCOMPLETE runbook documents the 48h retry +
  manual-resolution decision tree.
- **Compensating-rollback inviável**: post `dsr.erasure.completed.v1`
  the canonical Stripe `Customer.update` PII nullify + the canonical
  R2 mutable DELETE are irreversible. Mitigation: 24h verification
  window is the canonical "kick the tires" gate before final
  customer-facing notification.

### Neutral

- TLA+ specification: WI-S11-008 binds the canonical
  `dsr_erasure_atomicity.tla` proof to this eventual-consistency
  model; the canonical INV-DATA-ERASURE-COMPLETE invariant is
  formalised as "exists a finite t such that t ≤ queued + 30d AND
  every backend at t is in a successful state".

## Sign-off

- **Architect**: ✅ accepts subject to TLA+ proof at WI-S11-008.
- **Privacy Officer (interim)**: ✅ accepts subject to DPA + privacy
  notice documentation of the 24h window.
- **Compliance Officer**: ✅ accepts subject to RB-DSR-ERASURE-INCOMPLETE
  runbook activation pre-cutover.
- **Owner / Final Approver (Gustavo)**: ✅ accepts.

## References

- WI-S11-002 §9.2 ADR-S11-004 trigger.
- privacy_model.md §6.2 12-backend canonical pipeline.
- failure_modes.md FM-450 (DSR erasure-incomplete cross-backend) +
  FM-452 (consent-tampering-detected).
- specs/05_quality/runbooks/RB-DSR-ERASURE-INCOMPLETE.md decision
  tree per backend × failure class × remediation.
- ADR-S11-002 — split-tier discipline canonical (DSR fail-CLOSED vs
  billing fail-OPEN) cross-WI rationale.
- ADR-S11-003 — erasure_salt management interim.
- LGPD Art. 18 §III + GDPR Art. 17.1 "without undue delay".
- EDPB Guidelines 4/2019 on right to erasure (Art. 17 GDPR).
