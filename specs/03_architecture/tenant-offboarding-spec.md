---
id: "TENANT-OFFBOARDING-SPEC"
type: "architecture"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-05-15"
updated: "2026-05-27"
sprint: "R-prep"
parent_wi: "WI-R-PREP-TENANT-OFFBOARDING"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "PRIVACY-MODEL"
  - "INVARIANT-REGISTRY"
  - "ADR-S11-002"
tags: ["architecture", "offboarding", "tenant-lifecycle", "lgpd", "gdpr", "byok", "r-prep"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-admin-dry-run`, `corelink-byok-revocation`, `corelink-privacy-erasure-worker`, and `corelink-tenant-offboarding` were absorbed into `corelink-byok`, `corelink-ops`, and `corelink-privacy` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-byok-absorption.md / specs/_audits/sealed/2026-05-26-w35-p2-ops-absorption.md / specs/_audits/sealed/2026-05-26-w35-p2-privacy-absorption.md. Canonical consumer path is now `corelink_byok::*`, `corelink_ops::*`, `corelink_privacy::*`.

# Tenant-Offboarding Spec — 5-state lifecycle + 30/45/90-day windows

> **Status:** DRAFT. Owner: Gustavo Schneiter (interim Privacy Officer);
> transferred to formal DPO post-appointment.
> **Crate:** [`crates/corelink-tenant-offboarding`](../../crates/corelink-tenant-offboarding/)
> **Migration:** [`migrations/d1/0046_tenant_offboarding_state.sql`](../../migrations/d1/0046_tenant_offboarding_state.sql)
> **Runbook:** [`specs/_runbooks/RB-TENANT-OFFBOARDING.md`](../_runbooks/RB-TENANT-OFFBOARDING.md)
> **Customer guide:** [`apps/docs/docs/how-to/leave-corelink.mdx`](../../apps/docs/docs/how-to/leave-corelink.mdx)

## 1. Why this spec exists (distinct from DSR)

CoreLink already ships an individual data-subject-rights surface
(`corelink-dsr` for the LGPD Art. 18 / GDPR Art. 17 surface;
`corelink-privacy-erasure-worker` for the cross-backend cascade).
That surface answers the question *"as a natural person whose data
this tenant processes, please erase me"*.

It does NOT answer the question *"as a customer of CoreLink, we are
leaving — please offboard the whole tenant cleanly"*. These are
distinct lifecycles with distinct SLAs, distinct authorization
gates, distinct rollback windows, and distinct cryptographic
mechanics:

| Dimension | DSR (existing) | Tenant offboarding (this spec) |
|---|---|---|
| Subject | Individual natural person | Whole tenant (org account) |
| Triggered by | Data subject WebAuthn step-up | Customer click + support-agent anti-fraud verify |
| SLA | LGPD Art. 19 15d / GDPR Art. 12.3 30d (one-shot) | T+0 → T+90 lifecycle |
| Restoration | None (one-way commitment) | T+0..T+45 self-service revert |
| Export window | Single export endpoint | T+0..T+30 full-tenant CLI export |
| Cryptographic erasure | Per-record cascade (R2/D1/KV) | Tenant-wide cascade + BYOK CMK destroy |
| Authorization | WebAuthn step-up | Customer click + admin RBAC + dual-approval at commit |

The tenant lifecycle ALSO is what customers expect when they say
"we are cancelling our contract". The existing DSR surface is
required for compliance with individual rights; this spec is
required for the **commercial offboarding** that maps to MSA §10
(cancellation) + DPA v1.0.0 §11 (termination).

## 2. Canonical state machine

5 reachable post-cancellation states + the ACTIVE baseline = 6
states total in the canonical taxonomy.

```text
            ┌──────────────────────────────────────────────────┐
            │                                                  │
            │  CustomerReverted (T+0..T+45)                    │
            v                                                  │
        ┌────────┐                                              │
        │ ACTIVE │ <───────────────────────────────────────────┤
        └───┬────┘                                              │
            │ CustomerInitiated                                 │
            │ (T+0; anti-fraud verified)                        │
            v                                                   │
   ┌──────────────────┐                                         │
   │ CANCEL_REQUESTED │ ── CustomerReverted ────────────────────┤
   └────────┬─────────┘                                         │
            │ TimerExpired (T+1)                                │
            v                                                   │
   ┌──────────────────┐                                         │
   │  GRACE_PERIOD    │ ── CustomerReverted ────────────────────┤
   └────────┬─────────┘                                         │
            │ TimerExpired (T+30)                               │
            v                                                   │
   ┌──────────────────┐                                         │
   │    READ_ONLY     │ ── CustomerReverted ────────────────────┘
   └────────┬─────────┘
            │ TimerExpired (T+45)
            v
   ┌──────────────────┐
   │    SUSPENDED     │  (ops force-revert ONLY; not self-service)
   └────────┬─────────┘
            │ AdminCommitErasure (T+90; dry-run preview confirmed)
            v
   ┌──────────────────┐
   │     ERASED       │  (terminal; cryptographic erasure
   └──────────────────┘   committed; tenant_id reserved)
```

The full canonical transition table is encoded in
[`TenantOffboardingTransition::resolve`](../../crates/corelink-tenant-offboarding/src/state.rs).

## 3. Per-state capability matrix

Production middleware reads the canonical
[`TenantOffboardingCaps`](../../crates/corelink-tenant-offboarding/src/state.rs)
surface to gate every request at the handler boundary.

| State | Read | Write | Admin | Export (CLI) | Self-service Restore |
|---|:---:|:---:|:---:|:---:|:---:|
| ACTIVE | yes | yes | yes | yes | n/a |
| CANCEL_REQUESTED | yes | yes | yes | yes | yes |
| GRACE_PERIOD | yes | yes | yes | yes | yes |
| READ_ONLY | yes | no | yes | yes | yes |
| SUSPENDED | no | no | yes (ops only) | no | no (ops force only) |
| ERASED | no | no | no | no | no |

A write attempt against a `READ_ONLY` tenant returns HTTP 409 with
body code `offboarding_read_only` per `error_taxonomy.md`. A read /
export attempt against `SUSPENDED` returns HTTP 410 with body code
`offboarding_suspended`. Any operation against `ERASED` returns
HTTP 410 with body code `offboarding_erased`.

## 4. Transition triggers (canonical 5-arm taxonomy)

Mirrors [`TransitionTrigger`](../../crates/corelink-tenant-offboarding/src/state.rs):

- **`CustomerInitiated`** — customer clicks "Cancel my tenant" in
  the dashboard. Support agent verifies anti-fraud (signing
  authority on the account; canonical T-0 check; see RB-TENANT-
  OFFBOARDING §3.1). Advances ACTIVE → CANCEL_REQUESTED.
- **`CustomerReverted`** — customer changes their mind during the
  self-service window (T+0..T+45). Advances any of CANCEL_REQUESTED
  / GRACE_PERIOD / READ_ONLY back to ACTIVE.
- **`TimerExpired`** — the daily-cron tick observes that the
  canonical per-state timer has been reached. Advances forward.
- **`OpsForced`** — operator force-advance (auditable; runbook-
  gated). Allowed on any non-terminal source.
- **`AdminCommitErasure`** — final erasure commit (irreversible).
  Requires a dry-run preview to have been generated AND confirmed
  by the operator BEFORE the request is issued (defence-in-depth
  against runaway cron).

## 5. Timeline anchors

Per [`CANONICAL_*_DAYS`](../../crates/corelink-tenant-offboarding/src/orchestrator.rs)
crate constants:

| Anchor | Days from T+0 | Transition |
|---|---:|---|
| T+0 | 0 | ACTIVE → CANCEL_REQUESTED |
| T+1 | 1 | CANCEL_REQUESTED → GRACE_PERIOD |
| T+30 | 30 | GRACE_PERIOD → READ_ONLY |
| T+45 | 45 | READ_ONLY → SUSPENDED (end of self-service revert) |
| T+90 | 90 | SUSPENDED → ERASED (operator commits with dry-run preview) |

**Export window:** T+0 to T+30 inclusive. Customer may run
`corelink tenant export --output ./backup.tar.zst` at any time and
receive a content-addressed export of every CAS blob + audit-chain
slice + RBAC roster + DPA acceptance receipt. The export endpoint
remains technically reachable through T+45 (READ_ONLY); the canonical
"export window" is T+0..T+30 because beyond that the customer is
deep into restoration-window territory, not active offboarding.

**Restoration window:** T+0 to T+45 inclusive. Self-service revert
returns the tenant to ACTIVE with full data. Beyond T+45, restoration
requires ops force-revert from SUSPENDED → ACTIVE (auditable; manual
RBAC pair approval).

**Final erasure:** T+90 onward. The daily cron at 03:00 UTC enumerates
every SUSPENDED tenant whose `cancel_requested_at_ms + 90d <= now_ms`
and emits a `corelink.tenant.offboarding.erasure_eligible` event;
the operator (or a future auto-commit policy) issues
`AdminCommitErasure` with the dry-run preview confirmed. The
canonical erasure cascade runs:

1. **BYOK CMK destroy** (when applicable) via
   `corelink-byok-revocation`. Cryptographic erasure: destroying
   the tenant DEK-wrap CMK renders every R2 envelope-encrypted
   blob mathematically unrecoverable.
2. **R2 bucket tombstone-and-purge** for every blob under the
   tenant prefix.
3. **D1 row purge** for every per-tenant table.
4. **KV namespace purge** for every per-tenant key.
5. **Audit chain ENTRY remains** (7y per privacy_model.md §2) —
   the chain is append-only by INV-AUDIT-APPEND-ONLY; the
   `corelink.tenant.offboarding.erased` envelope is the
   regulatory anchor for the erasure event.

## 6. Audit events (canonical 6-event taxonomy)

Mirrors [`TenantOffboardingAuditEventType`](../../crates/corelink-tenant-offboarding/src/audit.rs).
Every state transition fires its canonical row **BEFORE** the
durable store mutation, per ADR-S11-002 split-tier fail-CLOSED
discipline:

- `corelink.tenant.offboarding.cancel_requested` — T+0 click.
- `corelink.tenant.offboarding.grace_started` — T+1.
- `corelink.tenant.offboarding.read_only_entered` — T+30.
- `corelink.tenant.offboarding.suspended_entered` — T+45.
- `corelink.tenant.offboarding.restored` — any self-service or
  ops force-revert back to ACTIVE.
- `corelink.tenant.offboarding.erased` — T+90+ commit.

## 7. Invariants

### INV-OFFBOARDING-GRACE-RESPECTED (HIGH)

Every TimerExpired-driven state advance is gated on the canonical
wall-clock threshold:

- CANCEL_REQUESTED → GRACE_PERIOD requires `now_ms >= cancel_requested_at_ms + 1d`.
- GRACE_PERIOD → READ_ONLY requires `now_ms >= cancel_requested_at_ms + 30d`.
- READ_ONLY → SUSPENDED requires `now_ms >= cancel_requested_at_ms + 45d`.

A request that fails the threshold check returns
[`TenantOffboardingError::GraceNotElapsed`](../../crates/corelink-tenant-offboarding/src/error.rs)
with the canonical `not_before_ms` field for forensic display.
The trait-level orchestrator enforces this AT the trait boundary;
the daily-cron caller is structurally unable to skip-ahead without
forging the `now_ms` argument (and forged time-of-day shows up as
clock-skew in the audit chain).

**OpsForced** transitions skip this check (auditable, runbook-
gated; the use case is regulatory force-advance — e.g. ANPD
takedown order — where the operator deliberately overrides the
canonical grace).

**Pinned by:** [`prop_state_machine_reachability_respects_grace`](../../crates/corelink-tenant-offboarding/tests/prop_tenant_offboarding.rs).

### INV-OFFBOARDING-AUDIT-COMPLETE (HIGH)

For every successful state mutation, exactly one canonical audit
row is emitted BEFORE the durable store mutation. Audit emit
failure aborts the transition; the durable store remains at its
pre-call state (fail-CLOSED per ADR-S11-002 split-tier).

Equivalent property: `len(audit_rows_for(tenant)) == count_of_successful_transitions_for(tenant)`.

**Pinned by:** [`prop_audit_emit_precedes_every_state_mutation`](../../crates/corelink-tenant-offboarding/tests/prop_tenant_offboarding.rs).

Both invariants are MUST-register entries in
[`INVARIANT-REGISTRY`](./invariant_registry.md) (domain: `OFFBOARDING`;
proposed for absorption in the next registry revision).

## 8. Crate surface

[`crates/corelink-tenant-offboarding`](../../crates/corelink-tenant-offboarding/):

- [`state`](../../crates/corelink-tenant-offboarding/src/state.rs)
  — `TenantOffboardingState` (`#[non_exhaustive]` 6-arm),
  `TransitionTrigger` (`#[non_exhaustive]` 5-arm),
  `TenantOffboardingCaps`, `TenantOffboardingTransition::resolve`.
- [`audit`](../../crates/corelink-tenant-offboarding/src/audit.rs)
  — `TenantOffboardingAuditEventType` (`#[non_exhaustive]` 6-arm),
  `TenantOffboardingAuditRecord`, `TenantOffboardingAuditSink`
  trait + `InMemoryTenantOffboardingAuditSink` +
  `FailingTenantOffboardingAuditSink`.
- [`store`](../../crates/corelink-tenant-offboarding/src/store.rs)
  — `TenantOffboardingStore` trait +
  `InMemoryTenantOffboardingStore` +
  `FailingTenantOffboardingStore`.
- [`orchestrator`](../../crates/corelink-tenant-offboarding/src/orchestrator.rs)
  — `TenantOffboardingOrchestrator` trait +
  `InMemoryTenantOffboardingOrchestrator` +
  `AdminCommitErasureRequest`.
- [`error`](../../crates/corelink-tenant-offboarding/src/error.rs)
  — `TenantOffboardingError` (`#[non_exhaustive]`).

Test coverage: 41 unit tests + 2 proptests at the time of authorship.

## 9. Production wiring (deferred to PRR ship gate)

Per the `trait-abstraction-defer` charter pattern, this crate ships
the pure-logic skeleton. Production wiring at PRR ship gate
binds:

- CF Worker routes:
  - `POST /v1/tenant/{tenant_id}/offboard` (initiate; customer click).
  - `POST /v1/tenant/{tenant_id}/offboard/revert` (self-service revert).
  - `GET  /v1/tenant/{tenant_id}/offboard/status` (poll).
  - `POST /v1/admin/tenant/{tenant_id}/offboard/force-advance` (ops; RBAC `admin` + dual-approval).
  - `POST /v1/admin/tenant/{tenant_id}/offboard/commit-erasure` (ops; dry-run preview confirmed).
- D1 mirror per [`migrations/d1/0046_tenant_offboarding_state.sql`](../../migrations/d1/0046_tenant_offboarding_state.sql).
- Daily cron at 03:00 UTC for TimerExpired advances.
- BYOK CMK destroy hook via `corelink-byok-revocation`.
- Admin dry-run preview via `corelink-admin-dry-run`.

## 10. Cross-references

- [`PRIVACY-MODEL`](./privacy_model.md) — LINDDUN + LGPD / GDPR /
  retention model (this spec extends the tenant-boundary lifecycle).
- [`corelink-dsr` crate-root rustdoc](../../crates/corelink-dsr/src/lib.rs)
  — individual data-subject-rights surface (complementary, not a
  substitute).
- [`corelink-privacy-erasure-worker` crate](../../crates/corelink-privacy-erasure-worker/)
  — cross-backend per-record cascade (consumed by the tenant-wide
  cascade at T+90).
- [`ADR-S11-002`](./adrs/) — split-tier audit fail-policy
  (DSR / offboarding fail-CLOSED ≠ billing fail-OPEN).
- [`marketing/sales/FAQ-MASTER.md` P6](../../marketing/sales/FAQ-MASTER.md)
  — customer-facing cancellation refund posture.
- [`ROADMAP-TO-GA.md` §8](../../ROADMAP-TO-GA.md) — GA launch wave
  (this spec is a prerequisite for the cancellation surface
  committed in the MSA §10).
- [`RB-TENANT-OFFBOARDING`](../_runbooks/RB-TENANT-OFFBOARDING.md)
  — operator runbook for the full lifecycle.
- [`leave-corelink.mdx`](../../apps/docs/docs/how-to/leave-corelink.mdx)
  — customer-facing 5-step guide.

## 11. Out of scope (deferred)

- **Auto-commit at T+90.** This spec requires `AdminCommitErasure`
  to be operator-issued with the dry-run preview confirmed. A
  policy-driven auto-commit (e.g. "every SUSPENDED tenant past
  T+120 auto-erases unless legal_hold = true") is a follow-on
  policy decision; the trait surface here would support it (add a
  cron-driven `AdminCommitErasure` issuer behind a feature flag)
  but it is NOT enabled by default for R-prep.
- **Legal hold pause semantics.** If `tenant.legal_hold = true`
  at any point in the lifecycle, all TimerExpired advances pause
  (canonical 5d cap per `privacy_model.md` §6.1 F-11 in DSR; the
  tenant-level inheritance is documented here but enforcement is
  deferred to the production wiring at PRR ship gate).
- **Bulk multi-tenant offboarding.** Enterprise customers that
  offboard 100+ child tenants in a single sweep (e.g. an MSP
  decommissioning) is a separate concern handled by a future
  `corelink-bulk-offboarding` crate.
