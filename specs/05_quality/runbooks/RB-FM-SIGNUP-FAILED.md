---
id: "RB-FM-SIGNUP-FAILED"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p2", "onboarding", "signup", "atomicity", "consistency"]
---

# RB-FM-SIGNUP-FAILED — Signup Atomicity Failure (Orphan Tenant Detection + Cleanup)

> **INV:** INV-ONBOARD-ATOMIC-PROVISIONING + INV-ONBOARD-DPA-FIRST | **SLA:** detect ≤ 24h, remediate ≤ 7d

## Pré-condições

- S-19 onboarding pipeline live (signup → tenant provisioning → DPA → Stripe → first PAT).
- D1 schema `account`, `tenant`, `user_account`, `membership`, `pat`, `consent_ledger`.
- Conversion funnel métricas (S-19 R-S19-11) instrumentadas.
- Stripe Customer + DPA signed records.

## Detecção

### Sinais primários

- D1 query identifies orphan rows: tenant sem corresponding Stripe customer OR DPA acceptance.
- Customer complaint: "I signed up but billing fails" OR "I'm billed but didn't sign DPA".
- Conversion funnel anomaly: `signup_completed` count >> `first_pat_created` count (>= 5% gap sustained).
- Reconciliation cron detects mismatch.

### Detection query

```sql
-- Orphan tenants: provisioned mas missing Stripe ou DPA
SELECT
  t.id as tenant_id,
  t.created_at,
  CASE WHEN cb.stripe_customer_id IS NULL THEN 'NO_STRIPE' END as stripe_status,
  CASE WHEN cl.subject_id IS NULL OR cl.purpose != 'dpa-acceptance' THEN 'NO_DPA' END as dpa_status,
  COUNT(p.id) as pat_count
FROM tenant t
LEFT JOIN customer_billing_profile cb ON cb.tenant_id = t.id
LEFT JOIN consent_ledger cl ON cl.subject_id = t.id AND cl.purpose = 'dpa-acceptance' AND cl.granted = true
LEFT JOIN pat p ON p.tenant_id = t.id AND p.deleted_at IS NULL
WHERE t.created_at < NOW() - INTERVAL '1 hour'  -- exclude in-progress signups
  AND (cb.stripe_customer_id IS NULL OR cl.subject_id IS NULL)
GROUP BY t.id, t.created_at, cb.stripe_customer_id, cl.subject_id
ORDER BY t.created_at DESC
LIMIT 100;
```

### Métricas

```
corelink.onboarding.step_started_total{step}
corelink.onboarding.step_completed_total{step}
corelink.onboarding.step_abandoned_total{step, reason}
corelink.onboarding.orphan_tenants_total
corelink.onboarding.atomic_rollback_total{reason}
```

## Comunicação

- **Severidade**: SEV-2 (data consistency + customer trust); SEV-1 se > 100 orphan tenants OR billing inadvertent.
- **Page**: SRE on-call + Engineer onboarding (S-19 owner).
- **Internal channel**: `#incidents-corelink-onboarding`.
- **Customer outreach**: required per orphan tenant; transparent + apologetic + remediation offer.
- **Status page**: usually internal; customer-visible se sustained > 1h.

## Mitigação imediata (≤ 7 dias)

### Step 1: Identify orphan tenants (≤ 1h)
1. Run detection query.
2. Identify pattern: single transaction failure vs systemic.
3. Categorize:
   - **NO_STRIPE**: tenant + DPA OK, missing Stripe customer.
   - **NO_DPA**: tenant + Stripe OK, missing DPA.
   - **Both missing**: tenant orphan; nunca completou onboarding.

### Step 2: Per-orphan remediation (≤ 24h por tenant)

**Case A: NO_STRIPE (most common)**
- Manual cleanup: complete Stripe customer creation via admin API.
- Customer notification: "We noticed signup didn't complete; service activated; please verify billing".
- Audit emit `corelink.onboarding.orphan_remediated`.

**Case B: NO_DPA**
- Customer outreach mandatory (regulatory issue).
- Send DPA acceptance email with re-link.
- Tenant status: degrade `read_only` (S-19 R-S19-5 30d grace + degrade) until DPA signed.
- Audit emit + Privacy Officer notify.

**Case C: Both missing**
- Customer outreach: confirm intent.
- If yes: complete onboarding manually (DPA + Stripe).
- If no: rollback tenant + cleanup.

### Step 3: Audit + reconciliation (≤ 7d)
1. Audit emit per orphan: `corelink.onboarding.orphan_detected/remediated/rolled_back`.
2. Reconciliation re-run.
3. Customer success outreach.
4. Refund se billing inadvertent.

## Diagnóstico (≤ 24h)

### Causa raiz típica

1. **Stripe outage durante signup** (FM-151) — 30-40% dos casos:
   - Customer signed up; Stripe API unavailable; transaction not committed.
   - Customer left thinking signup complete.
2. **Browser/network drop mid-signup** (20-30%):
   - Customer disconnected before final confirmation.
3. **Race condition em D1 transaction** (5-10%):
   - INV-ONBOARD-ATOMIC-PROVISIONING violation; bug em commit logic.
4. **Bug em DPA acceptance flow** (10-15%):
   - Customer clicked but click didn't propagate (bug em S-16 UI).
5. **DPA legal language change mid-flight** (rare):
   - DPA version bumped while customer was reading; signature mismatch.

### Investigação

- D1 transaction logs.
- Stripe API logs (status codes during signup window).
- Worker logs (signup orchestration trace).
- Cloudflare Network status.
- Customer session recording (S-16 if available).

## Resolução

### Hot fix (24-72h)

- Cleanup orphans via admin API.
- Customer notifications + remediation.
- Audit chain integrity verified.
- Reconciliation re-run green.

### Cold fix (1-4 weeks)

- **INV-ONBOARD-ATOMIC-PROVISIONING property test**: 10k iterations chaos Stripe outage during signup; verify rollback consistency.
- **Chaos test S-19 implementation**: forçar Stripe outage mid-signup → expect graceful rollback.
- **DPA versioning safe-stop**: se DPA bumped mid-signup, re-prompt customer com version diff.
- **Customer recovery flow** UI: "Resume signup" se browser drop detected.

## Post-incident

- Post-mortem se > 5 orphans/month OR systemic pattern.
- 5-Why obrigatório.
- INV-ONBOARD-ATOMIC-PROVISIONING review.
- Conversion funnel review.
- Privacy Officer involvement se DPA-related.

## Evidence

- Orphan detection query results (pre/post fix).
- Stripe API logs cross-reference.
- D1 transaction log audit.
- Customer outreach log + remediation outcomes.
- Audit emissions trail.

## References

- `invariant_registry.md` INV-ONBOARD-ATOMIC-PROVISIONING + INV-ONBOARD-DPA-FIRST (§3.12).
- `specs/04_sprints/S19/_spec_contract.md` (onboarding flow + R-S19-2 atomicity).
- `specs/04_sprints/S10/_spec_contract.md` (Stripe customer mapping).
- `specs/04_sprints/S11/_spec_contract.md` (DPA = consent type).
- `specs/03_architecture/error_taxonomy.md` `COR_BILLING_DPA_NOT_SIGNED`.
- RB-FM-151 (Stripe outage — common upstream).
- LGPD Art. 7 + GDPR Art. 7 (consent legitimacy).
