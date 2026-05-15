---
id: "RB-TENANT-OFFBOARDING"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-prep"
parent_wi: "WI-R-PREP-TENANT-OFFBOARDING"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "TENANT-OFFBOARDING-SPEC"
  - "RB-DSR-LGPD-FULL"
tags: ["runbook", "offboarding", "tenant-lifecycle", "support", "anti-fraud", "r-prep"]
---

# RB-TENANT-OFFBOARDING — Internal runbook for tenant-level offboarding

> **Status:** DRAFT. Owner: interim Privacy Officer (Gustavo
> Schneiter); transferred to formal DPO post-appointment.
> **Spec:** [`TENANT-OFFBOARDING-SPEC`](../03_architecture/tenant-offboarding-spec.md)
> **Crate:** [`crates/corelink-tenant-offboarding`](../../crates/corelink-tenant-offboarding/)

This runbook covers the **commercial offboarding** of a whole
tenant. For the individual data-subject-rights surface (LGPD
Art. 18 / GDPR Art. 17 — "please erase me"), see
[`RB-DSR-LGPD-FULL`](./RB-DSR-LGPD-FULL.md) +
[`RB-DSR-GDPR`](./RB-DSR-GDPR.md). The two are NOT substitutes.

## 1. Severity / paging

This runbook is **NOT a SEV runbook** under normal operation; the
canonical lifecycle is customer-initiated and runs autonomously
through the daily cron. Pager triggers:

- **SEV-3** — daily cron fails to advance a tenant whose canonical
  timer threshold has elapsed (the audit chain shows no
  `corelink.tenant.offboarding.{grace_started, read_only_entered,
  suspended_entered}` row when expected). On-call: privacy oncall
  rotation; escalation: DPO.
- **SEV-2** — a customer-reported failed self-service revert
  (customer claims "we changed our mind but the dashboard still
  shows us SUSPENDED"). On-call: support T-2 + privacy on-call.
- **SEV-1** — final erasure (T+90 commit) executed without dry-run
  preview confirmation OR an `AdminCommitErasure` was committed on
  a non-Suspended state (= structural invariant violation). On-call:
  DPO + Gustavo + Legal. Treat as a regulatory anomaly until proven
  benign; preserve the audit chain in read-only mode.

## 2. State-by-state operator playbook

### 2.1 ACTIVE → CANCEL_REQUESTED (T+0; customer click)

**Trigger:** customer clicks "Cancel my tenant" in the dashboard.
The CF Worker route validates the canonical anti-fraud envelope
(see §3.1) BEFORE invoking
`TenantOffboardingOrchestrator::initiate_cancel`.

**Operator action:**

1. **Verify anti-fraud (T-0 check).** Confirm the initiating user
   is (a) authenticated, (b) has RBAC role `owner` or `admin`,
   (c) has a valid MFA factor registered, (d) has been on the
   account for ≥ 7 days (anti-takeover; recently-added admin is
   the canonical insider-threat scenario per CTRL-AUTH-013).
   The CF Worker enforces (a)..(c) structurally; (d) is operator-
   gated for tenants under 30 days old.
2. **Send canonical confirmation email.** Template:
   `tenant_offboarding_cancel_requested` (en-US + pt-BR + es-MX).
   Body must include:
   - The canonical timeline (T+1 grace, T+30 read-only, T+45
     suspended, T+90 erased).
   - The export CLI command:
     `corelink tenant export --output ./backup.tar.zst`.
   - The self-service revert URL.
   - The support email for questions.
3. **Confirm audit row landed.** Query the audit chain for
   `corelink.tenant.offboarding.cancel_requested` with the new
   `tenant_id`. If absent, the request failed-CLOSED at the
   audit envelope; investigate via SEV-3 on-call.

### 2.2 CANCEL_REQUESTED → GRACE_PERIOD (T+1; daily cron)

**Trigger:** the daily cron at 03:00 UTC observes that the
canonical T+1 threshold has elapsed for the tenant. The cron
invokes `TenantOffboardingOrchestrator::cron_advance(tenant_id,
now_ms)`. INV-OFFBOARDING-GRACE-RESPECTED gates the advance at
the trait boundary.

**Operator action:** **none** under normal operation. SEV-3
applies only if the cron tick fails for ≥ 2 consecutive days
on a single tenant (= a structural store or audit failure;
see §4).

### 2.3 GRACE_PERIOD → READ_ONLY (T+30; daily cron)

**Trigger:** daily cron observes T+30 threshold.

**Operator action:** **none** under normal operation. Customer-
facing notification template `tenant_offboarding_read_only`
fires automatically (customer can still read + export + restore).

### 2.4 READ_ONLY → SUSPENDED (T+45; daily cron)

**Trigger:** daily cron observes T+45 threshold.

**Operator action:** **none** under normal operation. This is
the **end of the self-service revert window.** Customer-facing
notification template `tenant_offboarding_suspended_final_warning`
fires automatically (revert from this state requires ops force
per §3.3).

### 2.5 SUSPENDED → ERASED (T+90; operator commit with dry-run preview)

**Trigger:** daily cron at T+90 emits
`corelink.tenant.offboarding.erasure_eligible` (this is NOT a
state transition — it is a SIGNAL that the tenant is now eligible
for final erasure). An operator picks up the signal and runs the
canonical commit-with-dry-run flow:

1. **Generate the dry-run preview.** Run:
   ```bash
   corelink-admin offboarding preview \
     --tenant-id <ULID> --output /tmp/preview-<ULID>.json
   ```
   The preview enumerates every R2 blob, D1 row, KV key, and
   BYOK CMK that the canonical erasure cascade will touch. Cap:
   1 GB preview manifest (larger tenants emit a manifest of
   manifests).
2. **Review.** A second operator (dual-approval per
   `corelink-dual-approval`) reviews the preview, confirms the
   tenant_id matches the customer signal, and signs off. Both
   operator ids land in the canonical
   `corelink.tenant.offboarding.erased` audit row.
3. **Commit.** Run:
   ```bash
   corelink-admin offboarding commit-erasure \
     --tenant-id <ULID> \
     --dry-run-preview /tmp/preview-<ULID>.json \
     --confirmed-by <operator-2-id>
   ```
   The orchestrator validates `dry_run_preview_confirmed = true`
   AND the canonical `(operator_1, operator_2)` dual-approval
   envelope BEFORE issuing the `AdminCommitErasure` transition.
4. **Verify.** Confirm the audit chain landed the canonical
   `corelink.tenant.offboarding.erased` row AND the
   `corelink-byok-revocation` worker emitted the canonical CMK
   destroy receipt. If either is missing, treat as SEV-1.

## 3. Special paths

### 3.1 Anti-fraud verification (T+0)

The structural gates at the CF Worker layer:

- **Authenticated session** (`corelink-clerk` JWT valid).
- **RBAC role** ∈ `{owner, admin}`.
- **MFA factor registered** (`corelink-webauthn` `HasFactor` query).
- **Tenant age** ≥ 7 days OR operator-approved (anti-takeover).

If any structural gate fails, the request returns HTTP 403 with
body code `offboarding_not_authorized`. No state mutation.

Manual operator escalation is required for tenants flagged by
`corelink-abuse` (e.g. impossible-travel signal in the last
24 h). The escalation queue lives in support tier-2; the operator
calls the customer back at a known-good contact number BEFORE
advancing.

### 3.2 Self-service revert (T+0..T+45)

Customer clicks "Restore my tenant" in the dashboard. The CF
Worker invokes `TenantOffboardingOrchestrator::customer_revert`.
The orchestrator validates the current state is in the canonical
revert window and emits the
`corelink.tenant.offboarding.restored` row BEFORE mutating the
store.

**Operator action:** none under normal operation. SEV-2 applies
only if the customer claims a failed revert (see §1).

### 3.3 Ops force-revert from SUSPENDED → ACTIVE

Allowed but auditable. Use case: customer reaches out via
support at T+50 saying "we made a mistake, we want to come back".

**Process:**

1. Operator opens a `corelink-dual-approval` ticket with the
   canonical force-revert envelope.
2. A second operator approves.
3. Run:
   ```bash
   corelink-admin offboarding ops-revert \
     --tenant-id <ULID> --approved-by <operator-2-id>
   ```
4. The orchestrator emits `corelink.tenant.offboarding.restored`
   with the canonical operator ids.

### 3.4 Ops force-advance (auditable)

Use case: regulatory force-advance (ANPD takedown order).

Same dual-approval envelope as §3.3. Bypasses the canonical
grace threshold AT operator request (the audit row carries the
canonical `OpsForced` trigger + both operator ids for forensic
clarity). The runbook author SHOULD attach the canonical legal
authorization document (PDF) to the support ticket BEFORE the
second operator approves.

## 4. Common failure modes

### 4.1 Daily cron stuck on a single tenant

**Symptom:** the canonical
`corelink.tenant.offboarding.{grace_started, read_only_entered,
suspended_entered}` audit row is missing for a tenant whose
canonical threshold elapsed > 24h ago.

**Diagnosis:**

1. Read the tenant's current `tenant_offboarding_state` row.
2. Check the cron log for the canonical `cron_advance` invocation;
   look for `TenantOffboardingError::Audit` (fail-CLOSED) or
   `TenantOffboardingError::Store`.
3. If `Audit`: investigate audit-chain append health (this is
   the wider INV-AUDIT-APPEND-ONLY surface).
4. If `Store`: investigate D1 health for the canonical table.

**Mitigation:**

- If audit chain is healthy AND store is healthy AND the row is
  still stuck, replay the cron tick manually:
  ```bash
  corelink-admin offboarding cron-replay --tenant-id <ULID>
  ```
- If the replay also fails, file a SEV-3 page; do NOT manually
  UPDATE the D1 row (that bypasses the canonical audit envelope
  and violates INV-OFFBOARDING-AUDIT-COMPLETE).

### 4.2 Customer-reported failed export

**Symptom:** customer reports `corelink tenant export` fails or
returns a truncated archive.

**Diagnosis + Mitigation:** see
[`RB-DSR-LGPD-FULL`](./RB-DSR-LGPD-FULL.md) §4 (the export
plumbing is shared with the DSR Portability surface).

### 4.3 Final erasure cascade partially failed

**Symptom:** `corelink.tenant.offboarding.erased` row landed but
the `corelink-byok-revocation` cascade reports a partial failure
(e.g. AWS KMS API timeout on CMK destroy).

**Treatment:** SEV-1 — this is INV-BYOK-CMK-ERASURE-ATOMICITY
adjacent. Preserve the canonical erasure ticket. The
`corelink-byok-revocation` retry loop (exponential backoff;
canonical 24h cap) handles the transient case; a 24h+ failure
escalates to the BYOK rotation oncall + the canonical
`RB-CUSTOMER-SUPPORT-T-90` runbook.

## 5. Cross-references

- [`TENANT-OFFBOARDING-SPEC`](../03_architecture/tenant-offboarding-spec.md)
  — master spec for the lifecycle.
- [`RB-DSR-LGPD-FULL`](./RB-DSR-LGPD-FULL.md) — individual DSR
  surface; complementary, not a substitute.
- [`RB-DSR-GDPR`](./RB-DSR-GDPR.md) — GDPR variant.
- [`RB-CUSTOMER-SUPPORT-T-90`](./RB-CUSTOMER-SUPPORT-T-90.md) —
  T+0..T+90 customer-support triage.
- [`apps/docs/docs/how-to/leave-corelink.mdx`](../../apps/docs/docs/how-to/leave-corelink.mdx)
  — customer-facing 5-step guide.
- [`marketing/sales/FAQ-MASTER.md` P6 + M4](../../marketing/sales/FAQ-MASTER.md)
  — refund + data-egress posture.
