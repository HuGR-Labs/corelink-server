---
type: "ADR"
title: "ADR-S11-004 — Cross-backend erasure is eventual-consistency with a 24h verification gate (not 2PC)"
description: "Why the 12-backend DSR erasure pipeline is eventual-consistency with a 24h verification window gating dsr.completed.v1, rather than an atomic 2-phase commit."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-004-cross-backend-eventual-consistency.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s11", "privacy", "erasure", "eventual-consistency", "verification"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-004 — Cross-backend erasure is eventual-consistency with a 24h verification gate (not 2PC)

The canonical 12-backend DSR erasure pipeline spans heterogeneous storage classes — transactional
(Neon/D1/KV), irreversible third-party APIs (Stripe), eventual-consistency stores (R2 CAS/AC, Loki),
and WORM-immutable audit (R2 Object Lock 7y). A 2-phase-commit orchestrator is impossible here because
Stripe, R2 Object Lock, and the Loki delete API offer no prepare/commit/rollback protocol. This ADR
(doc_status SEALED) adopts the industry-standard answer: eventual consistency with a 24h verification
gate that must pass before `dsr.completed.v1` fires. It is the consistency model the split-tier
fail-CLOSED discipline (ADR-S11-002) and the interim salt design (ADR-S11-003) build on.

# Context

The pipeline's four storage classes are fundamentally heterogeneous, and the third-party surfaces are
unilateral: Stripe `Customer.update` is one-shot (and `Customer.delete` would break GAAP/LGPD fiscal
5y retention), R2 Object Lock is unilateral until retention expiry, and the Loki delete API queues with
up to a 24h settle delay. Per WI-S11-002 the canonical industry solution (OneTrust, Transcend,
DataGrail) is eventual consistency with a verification gate rather than 2PC.

# Decision

**Cross-backend erasure is eventual-consistency with a 24h verification window.** Per-backend
independent execution serializes progress via a tombstone in the D1 `dsr_erasure_log` table (UNIQUE
`(dsr_id, backend)` enforces replay-safety); each backend records a 5-arm outcome enum
(`erased | pseudonymized | partial_failure | failed | not_applicable`); a 24h verification cron sweep
re-fingerprints each backend via `verification_hash` (effective → 0 rows expected; pseudonymized →
100% redaction marker). All green → `verification_passed.v1` + `completed.v1`; any non-green →
`verification_failed.v1` + SEV-1 + the RB-DSR-ERASURE-INCOMPLETE runbook + a 48h retry; SLA-elapsed
incomplete → an `SlaBreached` arm + SEV-1.

# Consequences

- Positive: the LGPD/GDPR/CCPA "without undue delay" baseline is met well within statutory windows via
  the 24h gate; per-backend independent retry loops handle transient transport failures without rolling
  back the whole pipeline; every run emits a byte-deterministic signed forensic report (R2 evidence,
  7y).
- Negative: a ~24h customer-perceived delay before the `verification_passed` notification, and a single
  unhealthy backend blocks `dsr.completed.v1` until the 48h retry sweep; post-completion Stripe nullify
  + R2 mutable DELETE are irreversible, which is exactly why the 24h window is the "kick the tires"
  gate before the final notification.
- Neutral: WI-S11-008's `dsr_erasure_atomicity.tla` proof binds INV-DATA-ERASURE-COMPLETE to this
  eventual-consistency model (a finite t ≤ queued + 30d where every backend is successful).

# Citations

1. `specs/03_architecture/adrs/ADR-S11-004-cross-backend-eventual-consistency.md:23-54` — the Context:
   the four heterogeneous storage classes and why 2PC is impossible across them.
2. `specs/03_architecture/adrs/ADR-S11-004-cross-backend-eventual-consistency.md:56-80` — the Decision:
   per-backend tombstones, the 5-arm outcome enum, and the 24h verification sweep decision arms.
3. `specs/03_architecture/adrs/ADR-S11-004-cross-backend-eventual-consistency.md:82-122` — the
   Consequences: the regulatory baseline, the 24h delay + irreversibility, and the TLA+ invariant
   binding.
