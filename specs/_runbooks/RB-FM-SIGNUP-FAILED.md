---
id: "RB-FM-SIGNUP-FAILED"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-05-14"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S19-006"
tags: ["runbook", "p1", "onboarding", "signup", "atomic-provisioning", "dpa-first", "stub", "s19"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §6.

# RB-FM-SIGNUP-FAILED — Signup Atomicity Violation (Orphan Tenant / DPA-First Race / Stripe Outage Partial)

> **FM:** FM-X-SIGNUP-FAILED (S=4, O=2, D=3, RPN=24, P1) | **CTRL:** CTRL-ONBOARD-001..006, CTRL-PRIV-CONSENT-001..006 | **INV:** **INV-ONBOARD-ATOMIC-PROVISIONING** + **INV-ONBOARD-DPA-FIRST** | **SLA:** detect ≤ 10 min, mitigate ≤ 60 min, customer-notify ≤ 24 h
>
> **Stub-only at S-19 (WI-S19-006)**; full runbook + quarterly dry-run cadence deferred to **S-20 GA ship-gate**.

> **INV-ONBOARD-ATOMIC-PROVISIONING**: signup orchestration (Clerk verify → tenant row → DPA receipt → Stripe customer → first PAT) MUST commit as a single D1 transaction. Any partial-state observed in prod is by definition a violation; this runbook is the operator response.
>
> **INV-ONBOARD-DPA-FIRST**: a tenant row MUST NOT exist without a corresponding `dpa_acceptance` row joined by D1 lock; a Stripe subscription MUST NOT be activated without DPA receipt JWT verifiable (INV-CONSENT-PROOF-VERIFIABLE).

---

## 0. Pré-condições

- S-19 onboarding pipeline live (signup → tenant provisioning → DPA → Stripe → first PAT).
- D1 schema-of-record tables: `account`, `tenant`, `user_account`, `membership`, `pat`, `consent_ledger`, `customer_billing_profile` (S-10/S-11 contract).
- Conversion funnel métricas (S-19 R-S19-11) instrumentadas.
- Stripe Customer + DPA signed records.

---

## 1. Failure modes covered

| # | Sub-mode | Trigger | Symptom |
|---|---|---|---|
| 1 | **Network partition mid-transaction** | Cloudflare D1 connectivity blip during multi-statement TX | `corelink_onboarding_step_abandoned_total{step=signup, reason=tx_aborted}` spike |
| 2 | **Clerk webhook down / delayed** | Clerk service degradation; email-verified webhook never fires | `corelink_onboarding_step_duration_seconds_bucket{step=email_verified}` p99 > 5 min |
| 3 | **D1 unavailable** | Cloudflare D1 regional outage | tenant insert 5xx; orphan Clerk user with no tenant row |
| 4 | **Stripe outage during checkout** | Stripe API degradation; customer ID created but subscription activation fails | tenant row + DPA acceptance present, no `stripe_subscription_id` |
| 5 | **Race condition: DPA accept after Stripe activation** | Webhook reordering; INV-ONBOARD-DPA-FIRST violation | audit chain shows `stripe_subscription_activated` BEFORE `dpa_signed` for same `tenant_id` |
| 6 | **Audit chain break under concurrent emit** | 5 onboarding handlers stress hash deterministic computation | `corelink_audit_chain_integrity_violations_total` > 0 |

---

## 2. Detecção

- **Alert** `corelink_onboarding_step_abandoned_total{step, reason}` per-step > 5 × baseline em 5 min.
- **Alert** `corelink_onboarding_atomicity_violation_total` > 0 (CRITICAL; integrity-check cron emits hourly).
- **Alert** `corelink_onboarding_dpa_first_violation_total` > 0 (CRITICAL; INV-ONBOARD-DPA-FIRST property breach).
- **Alert** `corelink_onboarding_orphan_tenant_total` > 0 (HIGH; tenant row sem first_pat OR sem dpa_acceptance OR sem stripe_customer_id).
- SLO breach SLO-ONBOARD-SIGNUP-DURATION p99 > 3 min sustained 10 min.
- SLO breach SLO-ONBOARD-ATOMICITY < 100 % sustained 5 min.
- Customer report inbound (support inbox tag `signup-failed`).

---

## 3. Comunicação

- **SEV-2** if isolated to ≤ 5 tenants AND no INV violation flagged.
- **SEV-1** if INV-ONBOARD-ATOMIC-PROVISIONING OR INV-ONBOARD-DPA-FIRST violation flagged (legal exposure window opens).
- **SEV-1 mandatory escalation**: Privacy Officer + Legal Counsel + DPO paged for any DPA-related sub-mode (5).
- Page SRE on-call + Engineer S-19 lead + Privacy Officer (if DPA-first violation).
- Status page if customer-visible impact > 10 signups in 30 min.
- Internal Slack channel `#sev-onboarding` opened.

---

## 4. Mitigação imediata

1. **Identify scope**: query D1 + audit chain for partial-state tenants em últimos 60 min:
   ```sql
   SELECT t.tenant_id, t.created_at,
          d.tenant_id AS dpa_present,
          s.tenant_id AS stripe_present,
          p.tenant_id AS pat_present
   FROM tenant t
   LEFT JOIN dpa_acceptance d ON d.tenant_id = t.tenant_id
   LEFT JOIN stripe_customer s ON s.tenant_id = t.tenant_id
   LEFT JOIN pat p             ON p.tenant_id = t.tenant_id
   WHERE t.created_at > datetime('now','-60 minutes')
     AND (d.tenant_id IS NULL OR s.tenant_id IS NULL OR p.tenant_id IS NULL);
   ```

   **Alternate query** (S-10/S-11 contract table names — use when `customer_billing_profile` and `consent_ledger` are the schema-of-record):
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
   WHERE t.created_at < datetime('now', '-1 hour')  -- exclude in-progress signups
     AND (cb.stripe_customer_id IS NULL OR cl.subject_id IS NULL)
   GROUP BY t.id, t.created_at, cb.stripe_customer_id, cl.subject_id
   ORDER BY t.created_at DESC
   LIMIT 100;
   ```

   **Categorize** observed orphans (post-hoc data state, orthogonal to sub-mode trigger):
   - **NO_STRIPE**: tenant + DPA OK, missing Stripe customer (most common — Stripe outage during signup).
   - **NO_DPA**: tenant + Stripe OK, missing DPA acceptance (regulatory issue — Privacy Officer mandatory notify).
   - **BOTH_MISSING**: tenant orphan; nunca completou onboarding (browser/network drop most likely).

2. **Halt new signups** (feature flag `onboarding.signup_enabled = false` via config singleton — S-13 admin plane; requires dual-approval per CTRL-ADMIN-001).
3. **Per sub-mode**:
   - **Network partition** (1) / **D1 unavailable** (3): rely on D1 native retry; if persisted > 5 min, switch admin-emit banner "signup temporarily unavailable" and wait for CF recovery. Do NOT manually retry partial transactions.
   - **Clerk down** (2): if Clerk degraded, document on status page; existing email-verified users continue; new email-verify requests will queue at Clerk side.
   - **Stripe outage** (4): partial tenants in `awaiting_stripe` state are safe (idempotent retry covers); do NOT reset tenant rows. Worker `corelink-billing-stripe` will reconcile when Stripe recovers (saga pattern per WI-S19-001).
   - **DPA-first race** (5): IMMEDIATE — disable affected `tenant_id` (config singleton flag), audit emit `dpa_first_violation_detected`, notify Privacy + Legal within 2 h.
   - **Audit chain break** (6): IMMEDIATE — pause onboarding emitter; replay last clean checkpoint; cross-reference with WI-S09-004 audit chain processor.
4. **Customer notification**: per affected tenant, send `signup_retry_required` email (templated) within 1 h of detection.
5. **Refund se billing inadvertent**: any tenant categorized BOTH_MISSING ou NO_DPA cujo `customer_billing_profile.stripe_customer_id` has been charged DEVE receber refund via Stripe admin API; emit `corelink.onboarding.orphan_refund` audit event.

---

## 5. Diagnóstico

1. Query R2 onboarding audit chain bucket últimos 60 min; cross-reference timestamps Clerk + D1 + Stripe events.
2. Inspect tracing spans (S-09 OTel) for orphaned tenant request_ids; correlate region + tier.
3. Check Cloudflare D1 health page + Clerk status page + Stripe status page.
4. Run integrity-check ad-hoc: `corelink-onboarding-funnel integrity-check --since=60m` — exits non-zero with orphan list.
5. Confirm INV-ONBOARD-DPA-FIRST + INV-ONBOARD-ATOMIC-PROVISIONING property test still green em CI (regression possibility).

**Observed root-cause frequency** (use as triage prior when sub-mode isn't immediately obvious):

| Root cause | Observed % | Notes |
|---|---|---|
| Stripe outage during signup (FM-151 upstream) | 30-40 % | Maps to sub-mode 4 |
| Browser / network drop mid-signup | 20-30 % | Maps to BOTH_MISSING categorization |
| Bug em DPA acceptance flow (S-16 UI) | 10-15 % | Customer clicked but click didn't propagate |
| Race condition em D1 transaction | 5-10 % | INV-ONBOARD-ATOMIC-PROVISIONING violation; sub-mode 1/3 |
| DPA legal-language change mid-flight (race) | rare | DPA version bumped while customer was reading; signature mismatch |

---

## 6. Resolução

**Hot fix per sub-mode:**

| Sub-mode | Hot fix |
|---|---|
| 1 / 3 | Wait CF recovery; D1 native retry resolves; no manual intervention. |
| 2 | Clerk recovery; queue drains automatically. |
| 4 | Stripe recovery; `corelink-billing-stripe` reconciler picks up `awaiting_stripe` tenants. |
| 5 | Disable affected tenant; manual rollback (delete tenant + dpa + stripe_customer rows in single TX); customer re-signup; Privacy + Legal review. |
| 6 | Pause emitter; replay last clean checkpoint; rotate audit chain signing key if suspected key compromise (S-13 rotation worker). |

**Cold fix (post-incident):**

- Strengthen D1 transaction wrapper (add idempotency key per signup attempt).
- Add chaos drill scenario "Stripe outage during checkout" to RB-CHAOS-CATALOG.md.
- Increase property test iterations for INV-ONBOARD-DPA-FIRST cross-WI integration (1k → 10k).
- Review failure_modes.md FM-X-SIGNUP-FAILED RPN; possibly upgrade to P0 if recurrence.
- **DPA versioning safe-stop**: se DPA bumped mid-signup, re-prompt customer com version diff (avoids the rare DPA legal-language mid-flight race).
- **"Resume signup" UI recovery flow**: detect browser-drop (BOTH_MISSING categorization) → UI prompt on next session to resume from last completed step.

---

## 7. Decision tree (operator)

```
[orphan_tenant detected]
   │
   ├─ INV violation flagged? ──── YES ──► SEV-1 + Privacy + Legal page + halt signups
   │      │
   │      NO
   │      ▼
   ├─ Sub-mode identified?
   │      │
   │      ├─ Network partition / D1 outage ──► wait CF recovery (≤ 30 min)
   │      ├─ Clerk down                    ──► status page + wait
   │      ├─ Stripe outage                 ──► saga reconciler will resolve
   │      ├─ DPA-first race                ──► IMMEDIATE manual rollback + Privacy notify
   │      └─ Audit chain break             ──► pause emitter + replay checkpoint
   │
   └─► Customer notify ≤ 24h + post-mortem if SEV-2+
```

---

## 8. Post-incident

- Post-mortem mandatory if SEV-1 OR INV violation OR > 5 affected tenants.
- 5-Why mandatory se > 5 orphans/month OR systemic pattern (cumulative trigger, complementary to per-incident bar above).
- Customer outreach per affected tenant (signup-retry email + apology + offer extended trial if commercial impact).
- DPO + Privacy Officer review for any DPA-first violation (GDPR Art. 33 breach notification window: 72 h to supervisory authority if personal data risk).
- Update FM-X-SIGNUP-FAILED RPN in `failure_modes.md` based on observed Severity × Occurrence × Detection.
- Add findings to `specs/_audits/<date>-rb-fm-signup-failed-dry-run.md` (next quarterly drill).

---

## 9. Stub scope (S-19 vs S-20)

**Committed at S-19 (this stub):**
- 6 sub-modes documented.
- Detection alerts wired.
- Decision tree operator-ready.
- Communication + escalation matrix.

**Deferred to S-20 GA (full runbook):**
- Quarterly dry-run cadence (3 dry-runs per WI-S17-003 pattern).
- Synthetic incident report template instance.
- On-call training tabletop scenario.
- Customer notification template Legal + Privacy reviewed.
- Cross-reference with full failure_modes.md FM-X-SIGNUP-FAILED entry.

---

## 10. References

- `failure_modes.md` FM-X-SIGNUP-FAILED (to be added; placeholder until S-20).
- `specs/04_sprints/_sealed/S19/_spec_contract.md` (signup atomicity + DPA-first contract).
- `specs/04_sprints/_sealed/S19/work_items/WI-S19-001-signup-orchestration-atomic-provisioning-clerk-d1-tx-chaos-stripe-outage.md`.
- `specs/04_sprints/_sealed/S19/work_items/WI-S19-004-tier-selection-stripe-checkout-inv-onboard-dpa-first-d1-lock.md`.
- `specs/02_governance/invariant_registry.md` §3.12 (INV-ONBOARD-DPA-FIRST + INV-ONBOARD-ATOMIC-PROVISIONING).
- `specs/05_quality/runbooks/RB-FM-160-auth-invalid-storm.md` (pattern template).
- `specs/_runbooks/RB-GA-CUTOVER.md` §3.6 + §3.11 — GA cutover Clerk JWT issuer flip + signup-open feature flag (this runbook is consumed if signup fails post-cutover).
- `specs/03_architecture/error_taxonomy.md` `COR_BILLING_DPA_NOT_SIGNED` (error code surfaced when DPA receipt absent at Stripe activation).
- `specs/05_quality/runbooks/RB-FM-151-stripe-outage.md` (Stripe outage — common upstream root cause for sub-mode 4).
- GDPR Art. 33 (breach notification, 72h supervisory window).
- LGPD Art. 7 + GDPR Art. 7 (consent legitimacy — applicable when DPA acceptance is missing).

---

**Fim RB-FM-SIGNUP-FAILED v0.2.0 — S-19 ship-gate stub + Wave C duplicate-merger consolidation (2026-05-27).**
