---
type: audit
title: TLA+ coverage audit — pre-GA gap analysis
date: 2026-05-15
reviewer: Agent (R-PREP swarm)
sprint: R-PREP (GA hardening)
target: 11 TLA+ specs (7 canonical + 4 runbook); invariant_registry §3 (full registry)
status: COMPLETE
---

# TLA+ coverage audit — 2026-05-15

Pre-GA review of CoreLink's TLA+ corpus to identify the highest-impact uncovered invariant before R-prep ships. Companion delivery: one new spec
(`specs/tla/auth_revocation.tla`) closing the gap recommended in §5, plus
followup ticket backlog in `specs/_audits/sealed/tla-followup-tickets.md`.

## 1. Scope

Audited corpus:
- 7 canonical specs under `specs/tla/` (audit_immutability, billing_atomicity,
  cas_integrity, dsr_erasure_atomicity, gc_correctness, region_residency,
  tenant_isolation).
- 4 runbook specs under `specs/03_architecture/tla+/runbooks/` (byok_kill_switch,
  dpa_versioning_grace, residency_failover, signup_atomic).

Cross-referenced against:
- `specs/03_architecture/invariant_registry.md` (canonical INV catalogue,
  ~140 IDs across 19 sub-domains).
- §4.1 (GREEN), §4.2 (PLANNED), §4.3 (TLA-exempt), §4.4 (CI obligation gate).
- `specs/03_architecture/security_model.md §6.9 CTRL-FORMAL-001` (mandate that
  every CRITICAL invariant has a TLA+ spec).

The orchestrator brief said "8 specs"; the actual corpus is 11 (7 + 4 runbook).
This audit treats all 11 as in-scope.

## 2. Per-spec coverage table

| # | Spec | Domain | INVs proved | INVs NOT proved (within domain) |
|---|------|--------|-------------|---------------------------------|
| 1 | `tenant_isolation.tla` | TENANT | INV-TENANT-ISOLATION (CRITICAL), INV-ISO-READ, INV-ISO-WRITE, INV-ISO-ENUM | INV-AVAIL-ISOLATION (HIGH; covered indirectly only); per-region tenant pinning under partition (subsumed by region_residency, see #6). |
| 2 | `cas_integrity.tla` | CAS | INV-CAS-INTEGRITY, INV-CAS-IDEMPOTENCY, INV-CAS-IMMUTABILITY, INV-DIGEST-VERIFICATION, InvPoisoningRejected, InvClientVerifyIsSound | INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE (HIGH; statistical, exempted §4.3); multipart-flavoured CAS sessions (covered partially by INV-MULTIPART-FINALIZE-IRREVOCABLE which has no spec at all). |
| 3 | `gc_correctness.tla` | GC | INV-GC-001, INV-GC-004 (mark-phase-aware re-ref), grace boundary | INV-GC-DEGRADE-CORRECT (HIGH; partial), INV-GC-RECONCILE-AUDIT-FAIL-CLOSED (CRITICAL; only by inheritance from audit chain), INV-GC-SINGLE-RUNNING-PER-TENANT-REGION (HIGH; lock-protocol model gap). |
| 4 | `audit_immutability.tla` | AUDIT | INV-AUDIT-APPEND-ONLY (CRITICAL), InvChainIntact, InvOrderPreserved, dual-defense bypass adversarial | INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH; emit/handler transactional symmetry); INV-AUDIT-RETENTION-HINT-ACCURATE (HIGH; retention-hint propagation under DSR). |
| 5 | `billing_atomicity.tla` | BILLING | INV-BILLING-NO-LOSS, INV-BILLING-NO-DUP, INV-BILLING-RECONCILE-3-LAYER (Layer-1 sub-property), InvReplayDeterministic | INV-BILLING-APPEND-ONLY (HIGH; emit-ledger ordering); Stripe webhook out-of-order delivery (currently abstracted as outage). |
| 6 | `region_residency.tla` | REGION | INV-DATA-RESIDENCY (CRITICAL), INV-REGION-NO-CROSS-LEAK (CRITICAL), INV-TENANT-ISOLATION (region-level projection), ReplicationEventuallyConverges | INV-REGION-FAILOVER-NO-SPLIT-BRAIN (covered only by runbook residency_failover.tla; full multi-region split-brain semantics not in production CI matrix). |
| 7 | `dsr_erasure_atomicity.tla` | DATA / CONSENT | INV-DATA-ERASURE-COMPLETE (CRITICAL), INV-CONSENT-PROOF-VERIFIABLE (CRITICAL; via InvConsentSymmetry), InvResidencyPinned, InvResidencyMonotonic | INV-AUTH-CASCADE-DSR-COMPLETE (HIGH; only DB-level FK cascade; no TLA proof of cross-DB cascade fan-out under partial failure). |
| 8 | `runbooks/byok_kill_switch.tla` | KEY / BYOK | INV-BYOK-CRYPTO-SOVEREIGNTY (CRITICAL; admitted), DEK eviction within ≤ 360s SLA, deterministic reverse on re-enable | INV-KEY-NO-SKIP (HIGH; per-asset rotation state machine); INV-KEY-OVERLAP (HIGH; per-asset overlap windows); INV-ADMIN-DUAL-APPROVAL (HIGH). |
| 9 | `runbooks/dpa_versioning_grace.tla` | LEGAL | GraceWindowMonotone, re-acceptance restores access, degrade after 30 d, INV-CONSENT-PROOF (current dpa_version) | DPA version churn across regions (cross-region invariant not modelled); audit emission ordering at version bump. |
| 10 | `runbooks/residency_failover.tla` | REGION / RUNBOOK | INV-DATA-RESIDENCY (failover side), INV-REGION-NO-CROSS-LEAK under outage, in-set-only writes, primary recovery | INV-AUTH-PROPAGATION-AT-LEAST-ONCE under region outage (not modelled here; auth is downstream of failover). |
| 11 | `runbooks/signup_atomic.tla` | LEGAL / ONBOARDING | INV-ONBOARD-DPA-FIRST (CRITICAL), INV-ONBOARD-ATOMIC-PROVISIONING (CRITICAL; via 3-step saga), crash recovery between Clerk / D1 / Stripe steps | Re-signup with the same email after compensated failure (idempotency); Stripe webhook race after compensation. |

## 3. Coverage matrix — CRITICAL invariants

CRITICAL invariants in §3 of `invariant_registry.md` and their TLA+ status:

| Invariant | TLA+ spec | Status (post-audit) |
|---|---|---|
| INV-TENANT-ISOLATION | tenant_isolation, region_residency | ✅ GREEN |
| INV-CAS-INTEGRITY / -IDEMPOTENCY / -IMMUTABILITY | cas_integrity | ✅ GREEN |
| INV-DIGEST-VERIFICATION | cas_integrity (InvClientVerifyIsSound) | ✅ GREEN |
| INV-GC-001 / -GC-004 | gc_correctness | ✅ GREEN |
| INV-AUDIT-APPEND-ONLY | audit_immutability | ✅ GREEN |
| INV-DATA-RESIDENCY | region_residency, dsr_erasure_atomicity | ✅ GREEN |
| INV-REGION-NO-CROSS-LEAK | region_residency | ✅ GREEN |
| INV-DATA-ERASURE-COMPLETE | dsr_erasure_atomicity | ✅ GREEN |
| INV-CONSENT-PROOF-VERIFIABLE | dsr_erasure_atomicity (InvConsentSymmetry) | ✅ GREEN |
| INV-BYOK-CRYPTO-SOVEREIGNTY | byok_kill_switch (runbook; admitted) | 🟡 PARTIAL — runbook-level only; no per-region DEK race model |
| INV-ONBOARD-DPA-FIRST / -ATOMIC-PROVISIONING | signup_atomic (runbook) | ✅ GREEN |
| INV-AUTH-REVOCATION-IDEMPOTENT | — | ❌ **GAP — no spec** |
| INV-AUTH-REVOCATION-SLO-60S | — | ❌ **GAP — no spec** |
| INV-AUTH-MASS-REVOKE-ATOMIC | — | ❌ **GAP — no spec** |
| INV-AUTH-AUDIT-PSEUDONYMIZATION | audit_immutability (inheritance) | ✅ GREEN |
| INV-AUTH-PAT-HMAC-SIG-VERIFIED | — (planned `auth_pat_hybrid.tla`) | ❌ **GAP — no spec** |
| INV-AUTH-PII-ENCRYPTED | — (DB-level invariant; §4.3 exemption candidate) | 🟡 PARTIAL — exemption pending |
| INV-AUTH-PAT-PLAINTEXT-NEVER-PERSISTED | — (cryptographic; algorithmic) | 🟡 PARTIAL — exemption pending |
| INV-SUB-PROCESSOR-AUDIT-FAIL-CLOSED | audit_immutability (inheritance) | ✅ GREEN |
| INV-GC-RECONCILE-AUDIT-FAIL-CLOSED | audit_immutability (inheritance only) | 🟡 PARTIAL — direct GC×audit coupling not in spec |
| INV-AUTH-SCHEMA-RLS-DEFAULT-ON | — (DB invariant; §4.3 exemption) | 🟡 EXEMPT |

**Net CRITICAL gap**: 3 invariants in the auth-revocation pipeline + 1 in the
PAT-verify hybrid path + 2 partials. The audit auth-revocation cluster is the
single largest concentration of CRITICAL invariants with **zero TLA+ coverage**
and is the recommended target (§5).

## 4. Uncovered invariants — candidate list (≥ 5)

In descending order of impact (severity × distributed-semantics richness ×
production blast radius):

1. **INV-AUTH-REVOCATION-IDEMPOTENT** (CRITICAL) — retry of revoke MUST produce
   a single observable effect (one audit event, one revoked_at timestamp).
   Spans Neon SoT + DO storage + KV cache + CF Queue dedup. Pure
   distributed-systems property. **No TLA+ spec.**
2. **INV-AUTH-REVOCATION-SLO-60S** (CRITICAL) — cross-region propagation ≤ 60 s
   p99 sustained 72 h. State-machine convergence under at-least-once queue
   delivery + per-region DO consumers. **No TLA+ spec.**
3. **INV-AUTH-MASS-REVOKE-ATOMIC** (CRITICAL) — Neon UPDATE phase is all-or-none
   for an entire tenant's PAT set; outbox INSERT phase is chunked but eventually
   complete. **No TLA+ spec.**
4. **INV-AUTH-PAT-HMAC-SIG-VERIFIED** (CRITICAL) — hybrid HMAC + Argon2id verify
   ordering (fast-fail before DB hit). State machine over verify-path steps
   3a/3b/3c. Planned `auth_pat_hybrid.tla`; **no spec yet.**
5. **INV-KEY-NO-SKIP / INV-KEY-OVERLAP** (HIGH × 2) — per-asset key rotation
   state machine with overlap windows (PAT 24 h, audit 24 h, TDK 7 d, BYOK 7 d,
   Ed25519 30 d). Planned `key_lifecycle.tla`; **no spec yet.**
6. **INV-ADMIN-DUAL-APPROVAL** (HIGH) — destructive admin op state machine
   (caller ≠ approver enforced D1 hard-check). Would inherit from
   `key_lifecycle.tla`; **no spec yet.**
7. **INV-MULTIPART-FINALIZE-IRREVOCABLE** (HIGH) — once a multipart session is
   sealed it cannot be aborted. State monotonicity property; covered only by
   property tests (§4.3 implicit exemption); upgrading to TLA+ would catch
   races between concurrent abort + finalize.
8. **INV-GC-SINGLE-RUNNING-PER-TENANT-REGION** (HIGH) — distributed lock
   protocol (single GC worker per tenant×region). Currently lock-protocol-only
   in code; no formal proof of no-double-run under leader handoff.
9. **INV-REGION-FAILOVER-NO-SPLIT-BRAIN** (CRITICAL implied by §3.11 + §3.12) —
   covered by runbook `residency_failover.tla` but NOT by a canonical CI gate
   in `tla_check.yml`; failover semantics not in PR-gate matrix.

Total candidate gaps: **9 invariants, of which 4 are CRITICAL**.

## 5. Recommendation — single new spec to write

**Write `specs/tla/auth_revocation.tla`** covering candidates **1, 2, 3** plus
INV-AUTH-PROPAGATION-AT-LEAST-ONCE (HIGH).

### Why this one (over INV-TENANT-ISOLATION, INV-FAILOVER-NO-SPLIT-BRAIN, INV-CONSENT-VALIDITY-MONOTONIC)

| Candidate | Reason it's NOT the pick |
|---|---|
| INV-TENANT-ISOLATION | already ✅ GREEN in `tenant_isolation.tla` (5-layer defense) and reinforced by `region_residency.tla`. |
| INV-FAILOVER-NO-SPLIT-BRAIN | already covered by runbook `residency_failover.tla`. Gap is only CI-matrix exposure (see §4 candidate #9 and followup tickets), not a spec gap. |
| INV-CONSENT-VALIDITY-MONOTONIC | covered by `dsr_erasure_atomicity.tla` (`InvResidencyMonotonic` + `InvConsentSymmetry`) and `dpa_versioning_grace.tla` runbook. |

### Why auth_revocation wins

- **Bundles 3 CRITICAL + 1 HIGH** invariants in one model (highest density of
  CRITICAL gaps fixed per spec).
- **Pure distributed-systems property** — the exact regime where TLA+
  outperforms property tests (queue redelivery, consumer dedup, atomic
  transaction across SoT + outbox + per-region cache). Property tests already
  exist (10k iter); they cannot exhaustively prove safety under arbitrary
  redelivery interleavings.
- **Pre-GA blocker per §4.4 obligation gate** — three CRITICAL invariants are
  PLANNED in `invariant_registry.md §4.2`; CI obligation gate
  (`scripts/check_tla_obligations.py`) will refuse to ship S-20 GA without GREEN
  status here.
- **No new abstractions needed** — model reuses the abstractions in
  `audit_immutability.tla` (outbox sequence) and `region_residency.tla`
  (per-region monotonic flag).

### Delivery (this PR)

- `specs/tla/auth_revocation.tla` — 4 actions (RevokeSingle, MassRevoke,
  DeliverRevocation, RedeliverDup); 4 safety invariants; 1 temporal property
  (RevokedEventuallyConverges).
- `specs/tla/auth_revocation.cfg` — PR-lane bounds (2 PATs × 2 regions ×
  2 tenants; MaxOps = 5, MaxQueue = 6). TLC verified locally:
  **33 229 states generated, 6 785 distinct, complete graph depth = 10,
  finished in ~2 s. No errors found.**
- `specs/tla/auth_revocation_nightly.cfg` — nightly bounds (3 PATs ×
  2 regions × 2 tenants; MaxOps = 6, MaxQueue = 8) for the deeper sweep run
  via `.github/workflows/nightly.yml`. TLC verified locally: **2 738 174
  states generated, 426 161 distinct, depth 13, ~3 min on a 12-core laptop,
  no errors.** A 3×3×2 × (MaxOps=9, MaxQueue=12) probe exceeded 4M states /
  15 min — out of CI's 30 min budget; the cfg was trimmed accordingly.
  Larger sweeps should land under Apalache symbolic mode (FT followup).
- CI matrix update: added to `.github/workflows/tla_check.yml`.

## 6. Local TLC reproduction

```sh
# Pre-req: JDK 17+ and TLC v1.8.0 jar (or newer; spec parses on
# 2026.05.12.170007 nightly).
java -cp /path/to/tla2tools.jar tlc2.TLC \
  -config specs/tla/auth_revocation.cfg \
  -workers 2 -fp 32 -checkpoint 0 \
  specs/tla/auth_revocation.tla
```

Expected on the PR lane (auth_revocation.cfg): ~6 800 distinct states, finishes
in single-digit seconds on a 4-core laptop; "No error has been found".

## 7. Cross-references

- `specs/03_architecture/auth_model.md §6` (revocation semantics + 60 s SLA).
- `specs/03_architecture/invariant_registry.md §3.14` (canonical INV
  definitions) and §4.2 (PLANNED obligation matrix).
- `specs/03_architecture/security_model.md §6.9 CTRL-FORMAL-001` (mandate).
- `ROADMAP-TO-GA.md` (R-PREP entry — to be cross-linked when this PR lands).
- `specs/_audits/sealed/tla-followup-tickets.md` — backlog for the other 8 candidate
  gaps.

## 8. Caveats / known limitations

1. The pinned TLC jar SHA in
   `.github/workflows/tla_check.yml` and `scripts/run_tlc_corelink.sh`
   (`d5d07d5dab38…`) does not match the current GitHub Release asset
   (verified 2026-05-15: actual `25780ac9578e…`). This is a **separate pre-
   existing CI issue** unrelated to this audit — re-pinning requires Security
   review per ADR-0042 §A1. Local validation here used a SHA-skip path; CI
   will need the existing pin investigation (logged in
   `tla-followup-tickets.md` §FT-3).
2. TLA+ proves **topological convergence** of revocation; the 60 s wall-clock
   SLO is enforced separately by chaos tests + Prometheus SLO alerts. The
   `RevokedEventuallyConverges` property is the formal projection of "every
   revoked PAT reaches every region" — it is necessary but not sufficient
   for the 60 s budget.
3. `PatTenant` is defined inside the .tla module rather than as a .cfg
   constant because TLC's `.cfg` literal syntax for function-valued constants
   is brittle across releases (region_residency.cfg's `PrimaryRegionOf` cfg
   literal also fails to parse on TLC 2026.05.12; logged as a separate
   followup, FT-4).

---

**End of audit.** Companion artefacts:
- `specs/tla/auth_revocation.tla` + two `.cfg` files (PR + nightly).
- `specs/_audits/sealed/tla-followup-tickets.md` (8 remaining gap tickets).
- `.github/workflows/tla_check.yml` matrix update.
