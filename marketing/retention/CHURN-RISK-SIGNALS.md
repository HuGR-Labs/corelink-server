---
id: "R-PREP-CHURN-RISK-SIGNALS"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "R-PREP-CHURN-RETENTION"
tags: ["marketing", "retention", "churn", "heuristics", "r-prep", "ga", "post-launch"]
---

# CoreLink Churn Risk Signals — Heuristic Catalog

> **Audience.** Customer Success, Account Executives, SRE on-call for SLO breaches, Founder (CEO touch).
> **Purpose.** Catalog of **15 heuristic signals** that suggest a tenant is at risk of cancellation. No ML model — every signal is a query against existing observability + billing + audit data. The retention play (per-signal response) lives in [`RETENTION-PLAYBOOK.md`](./RETENTION-PLAYBOOK.md). The operational runbook (who owns each, escalation, discount authority) lives in [`../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md`](../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md).
> **Non-goal.** Predicting churn with a model. We are buying signal at the cost of false positives; CS triages, not the system. Re-tune quarterly via [`quarterly-review-template.md`](./quarterly-review-template.md).
> **Instrumentation status.** This catalog is a **design spec**, not a live feed. The six PromQL-based signals below (S-01, S-05, S-06, S-10, S-13, S-15) reference `corelink_*` metrics that have **no code emitter anywhere in the codebase today** — they are planned / not-yet-instrumented. CS should NOT expect these six to fire in production until an engineering ticket wires the emitter and the alerting rule. The SQL-based signals (billing/support/CRM queries) are unaffected by this note.
> **Companion.** [`../sales/OBJECTION-HANDLING.md`](../sales/OBJECTION-HANDLING.md) for renewal conversations; [`../lighthouse-kit/CUSTOMER-PLAYBOOK.md`](../lighthouse-kit/CUSTOMER-PLAYBOOK.md) for the customer-facing engagement contract.
> **Roadmap link.** [`../../ROADMAP-TO-GA.md`](../../ROADMAP-TO-GA.md) §8 (post-launch Wave R-8) — retention plays activate from T+1d.

---

## How to read this catalog

Each signal has:

- **Severity** — `high`, `medium`, `low`. Drives the play and the SLA in [`RETENTION-PLAYBOOK.md`](./RETENTION-PLAYBOOK.md).
- **Detection** — the literal query (PromQL on the metrics fleet, or SQL on the billing / audit / support read-replicas). Every query is parameterized by `tenant_id` so the signal can fire per tenant.
- **Action SLA** — wall-clock target from signal-fired to first human touch.
- **Source of truth** — the system that owns the underlying data. Cross-system signals are noted.
- **False-positive rate (estimated)** — qualitative `low/med/high`; pure heuristic — re-tune via quarterly review.

Signals **fire**, they do not **decide**. A CS triage step (see [`RB-CHURN-RISK-RESPONSE.md`](../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md) §3) gates every play.

Severity tiers map roughly to:

| Severity | Touch | First-response SLA | Authority |
|---|---|---|---|
| `high` | CEO / Founder touch | ≤ 24 h | Up to free tier upgrade or 50% / 3mo discount |
| `medium` | Account Executive / CSE touch | ≤ 72 h | Up to 25% / 3mo discount, escalate for more |
| `low` | Educational comms email (automated) | ≤ 7 d | No discount; content + check-in |

---

## Signal catalog

### S-01 — Usage drop > 50% week-over-week

- **Severity:** `high`
- **Status:** ⚠️ planned / not-yet-instrumented — `corelink_cas_bytes_total` has no emitter in the codebase yet.
- **What it means.** A tenant's weekly CAS PUT+GET byte volume has dropped by more than 50% compared with the trailing 4-week median. Strongest leading indicator of cancellation we have — confirmed by three Forge customer-zero retention windows.
- **Detection (PromQL):**
  ```promql
  (
    sum by (tenant_id) (
      rate(corelink_cas_bytes_total{op=~"put|get"}[7d])
    )
    /
    avg_over_time(
      sum by (tenant_id) (
        rate(corelink_cas_bytes_total{op=~"put|get"}[7d])
      )[28d:7d]
    )
  ) < 0.5
  ```
- **Action SLA:** 24 h to CEO touch.
- **Source of truth:** Prometheus metrics fleet (per-tenant labels, see `crates/corelink-metrics/src/lib.rs`).
- **False-positive rate:** medium — legitimate causes include CI rotation, holiday weeks, internal infra migration. Triage gate confirms.

### S-02 — Open support ticket > 7d unresolved

- **Severity:** `high`
- **What it means.** A ticket of any severity has been in the queue for more than 7 calendar days without resolution. SLA breach is itself the churn risk: customers do not renew when they perceive support as broken.
- **Detection (SQL on support read-replica):**
  ```sql
  SELECT tenant_id, ticket_id, opened_at, severity
  FROM support_tickets
  WHERE state IN ('open', 'investigating', 'waiting-customer')
    AND opened_at < NOW() - INTERVAL '7 days'
    AND tenant_tier IN ('starter', 'team', 'pro', 'enterprise');
  ```
- **Action SLA:** 24 h to CEO touch (Founder takes over the ticket personally if escalation has failed).
- **Source of truth:** support system (HubSpot Service Hub at GA per H-5).
- **False-positive rate:** low — by definition a real SLO breach. Triage validates ticket has not been wrongly stuck in `waiting-customer`.

### S-03 — Failed payment retry > 3

- **Severity:** `high`
- **What it means.** Stripe has attempted to charge the saved payment method 3+ times and all attempts failed. Indicates either an expired card (recoverable) or an intentional non-payment (churning).
- **Detection (SQL on billing read-replica):**
  ```sql
  SELECT tenant_id, subscription_id, last_failure_reason, attempts
  FROM stripe_payment_attempts
  WHERE state = 'failed'
    AND attempts >= 3
    AND last_attempt_at > NOW() - INTERVAL '14 days';
  ```
- **Action SLA:** 24 h to CEO touch + parallel automated dunning email (Stripe Smart Retries).
- **Source of truth:** Stripe webhook ingestion (`crates/corelink-billing-webhooks`).
- **False-positive rate:** low — Stripe Smart Retries already absorbs transient declines; surfaces here only after persistent failure.

### S-04 — Admin user removed without replacement

- **Severity:** `high`
- **What it means.** The tenant's last user with role `admin` has been removed from the seat list, OR the named primary admin (from onboarding) has been removed and not replaced within 72 h. Often signals organizational change (champion departure, restructure, M&A) that precedes cancellation.
- **Detection (SQL on tenant DB):**
  ```sql
  SELECT t.tenant_id,
         t.primary_admin_user_id,
         (SELECT COUNT(*) FROM tenant_seats s
          WHERE s.tenant_id = t.tenant_id AND s.role = 'admin'
            AND s.removed_at IS NULL) AS active_admins
  FROM tenants t
  WHERE t.tier IN ('starter', 'team', 'pro', 'enterprise')
    AND (
      (SELECT COUNT(*) FROM tenant_seats s
       WHERE s.tenant_id = t.tenant_id AND s.role = 'admin'
         AND s.removed_at IS NULL) = 0
      OR
      EXISTS (
        SELECT 1 FROM tenant_seats s
        WHERE s.user_id = t.primary_admin_user_id
          AND s.removed_at IS NOT NULL
          AND s.removed_at > NOW() - INTERVAL '72 hours'
      )
    );
  ```
- **Action SLA:** 24 h to CEO touch (champion-departure detection).
- **Source of truth:** tenant DB seat ledger (`crates/corelink-tier-selection/src/tenant.rs`).
- **False-positive rate:** medium — could be intentional admin rotation. Triage asks before reaching out.

### S-05 — Audit trail query rate drops to zero

- **Severity:** `medium`
- **Status:** ⚠️ planned / not-yet-instrumented — `corelink_audit_query_total` has no emitter in the codebase yet.
- **What it means.** A tenant who has historically queried the audit chain (via `corelink audit tail` / `corelink audit verify` / `corelink audit export`, or the `/v1/audit/*` API) has stopped entirely for 14 consecutive days. Indicates that internal compliance or governance interest in CoreLink has cooled — frequently a precursor to procurement reviewing the contract for cut.
- **Detection (PromQL):**
  ```promql
  (
    sum by (tenant_id) (
      increase(corelink_audit_query_total[14d])
    )
  ) == 0
  and on (tenant_id)
  (
    sum by (tenant_id) (
      increase(corelink_audit_query_total[90d] offset 14d)
    )
  ) > 0
  ```
- **Action SLA:** 72 h to AE / CSE touch.
- **Source of truth:** Prometheus (`corelink_audit_query_total{op=~"list|verify|export"}`).
- **False-positive rate:** medium — some tenants only query during audits (quarterly cadence). Triage compares against tenant's historical pattern.

### S-06 — SLO breach experienced by this tenant in last 30d

- **Severity:** `high`
- **Status:** ⚠️ planned / not-yet-instrumented — `corelink_slo_breach_total` has no emitter in the codebase yet.
- **What it means.** The tenant has been on the wrong side of at least one SLO breach in the past 30 days — measured per-tenant, not fleet-wide. A fleet-wide breach is bad; a customer-experienced breach is a renewal blocker.
- **Detection (PromQL):**
  ```promql
  max by (tenant_id) (
    increase(corelink_slo_breach_total{customer_impacted="true"}[30d])
  ) > 0
  ```
- **Action SLA:** 24 h to CEO touch + post-mortem hand-deliverable (per `specs/_runbooks/RB-POSTMORTEM-PROCESS.md`).
- **Source of truth:** SLO catalog instrumentation (`specs/05_quality/slo_catalog.md` + per-tenant breach counter wired in S-18).
- **False-positive rate:** low — breach detector is conservative by design.

### S-07 — Pricing-page revisit cluster

- **Severity:** `medium`
- **What it means.** Logged-in users from a tenant have visited `/explanation/pricing/` more than 5 times in a trailing 7-day window. This is a HubSpot Marketing tracking signal — interpreted as "they are re-evaluating, possibly to negotiate or to compare against an alternative."
- **Detection (SQL on HubSpot CRM export):**
  ```sql
  SELECT tenant_id, COUNT(*) AS page_visits
  FROM hubspot_page_views pv
  JOIN tenant_user_emails tue ON pv.email = tue.email
  WHERE pv.page LIKE '/explanation/pricing/%'
    AND pv.viewed_at > NOW() - INTERVAL '7 days'
  GROUP BY tenant_id
  HAVING COUNT(*) > 5;
  ```
- **Action SLA:** 72 h to AE / CSE touch.
- **Source of truth:** HubSpot CRM (H-5).
- **False-positive rate:** high — also fires when a tenant is upsizing. Triage joins against actual usage trend before reaching out.

### S-08 — Quarterly business review (QBR) declined / no-show

- **Severity:** `medium`
- **What it means.** Team-tier and above tenants get a scheduled QBR each quarter. A declined or no-show QBR — particularly a second consecutive miss — is a high-quality cooling signal because the customer is opting out of the success motion.
- **Detection (SQL on CRM):**
  ```sql
  SELECT tenant_id, qbr_scheduled_at, status, miss_count
  FROM crm_qbr_history
  WHERE status IN ('declined', 'no-show')
    AND qbr_scheduled_at > NOW() - INTERVAL '180 days'
    AND miss_count >= 1;
  ```
- **Action SLA:** 72 h to AE / CSE touch.
- **Source of truth:** HubSpot CRM (H-5).
- **False-positive rate:** medium — schedule conflicts happen; weight increases per consecutive miss.

### S-09 — Seat utilization < 30% of tier cap for 60 d

- **Severity:** `medium`
- **What it means.** Tenant is on a paid tier with N seats included but is consistently using less than 30% of them for 60+ days. They are paying for capacity they do not need; at renewal they will downgrade or churn.
- **Detection (SQL on tenant DB joined with metrics):**
  ```sql
  SELECT t.tenant_id, t.tier, t.included_seats,
         active_seats.cnt AS active_seats
  FROM tenants t
  JOIN LATERAL (
    SELECT COUNT(*) AS cnt
    FROM tenant_seats s
    WHERE s.tenant_id = t.tenant_id
      AND s.removed_at IS NULL
      AND s.last_active_at > NOW() - INTERVAL '60 days'
  ) active_seats ON true
  WHERE t.tier IN ('team', 'pro')
    AND active_seats.cnt::float / t.included_seats < 0.3;
  ```
- **Action SLA:** 72 h to AE / CSE touch (proactive downgrade offer is cheaper than churn).
- **Source of truth:** tenant DB.
- **False-positive rate:** medium — could be seasonal team scaling; triage compares against trailing 12-month pattern.

### S-10 — Egress / export activity spike

- **Severity:** `high`
- **Status:** ⚠️ planned / not-yet-instrumented — `corelink_cas_export_bytes_total` has no emitter in the codebase yet.
- **What it means.** Tenant has used `corelink cas export` (the documented escape-hatch — see [`OBJECTION-HANDLING.md`](../sales/OBJECTION-HANDLING.md) Obj-1) at a volume more than 10× their trailing 90-day baseline. This is the most literal possible churn signal: they are exfiltrating their own data, likely to a competitor or to a self-hosted alternative.
- **Detection (PromQL):**
  ```promql
  (
    sum by (tenant_id) (increase(corelink_cas_export_bytes_total[24h]))
    /
    avg_over_time(
      sum by (tenant_id) (increase(corelink_cas_export_bytes_total[24h]))[90d:1d]
    )
  ) > 10
  ```
- **Action SLA:** 24 h to CEO touch — Founder must understand why they are leaving (or scaling).
- **Source of truth:** Prometheus (`corelink_cas_export_bytes_total`).
- **False-positive rate:** medium — legitimate causes include disaster-recovery rehearsals or scheduled archival exports.

### S-11 — NPS detractor response (score ≤ 6)

- **Severity:** `medium`
- **What it means.** Tenant responded to the quarterly NPS survey with a 0–6 (detractor band). A detractor score from a paying tenant is a renewal red flag, regardless of usage health.
- **Detection (SQL on CRM):**
  ```sql
  SELECT tenant_id, score, comment, submitted_at
  FROM nps_responses
  WHERE score <= 6
    AND submitted_at > NOW() - INTERVAL '90 days';
  ```
- **Action SLA:** 72 h to AE / CSE touch (with the survey verbatim).
- **Source of truth:** HubSpot NPS module.
- **False-positive rate:** low — by definition the customer signaled dissatisfaction.

### S-12 — Documentation thrash without ticket

- **Severity:** `low`
- **What it means.** Tenant users have viewed the same documentation page more than 10 times in 7 days without opening a support ticket. Suggests stuck-but-not-asking — common in mid-market customers who eventually churn quietly because they "could not get it working."
- **Detection (SQL on docs analytics):**
  ```sql
  SELECT tenant_id, page_path, COUNT(*) AS views
  FROM docs_page_views
  WHERE viewed_at > NOW() - INTERVAL '7 days'
  GROUP BY tenant_id, page_path
  HAVING COUNT(*) > 10
     AND NOT EXISTS (
       SELECT 1 FROM support_tickets st
       WHERE st.tenant_id = docs_page_views.tenant_id
         AND st.opened_at > NOW() - INTERVAL '7 days'
     );
  ```
- **Action SLA:** 7 d to educational-comms email (automated).
- **Source of truth:** docs site analytics (CF Web Analytics).
- **False-positive rate:** high — could be a single user bookmarking a reference page. Aggregate-only signal; do not over-personalize the comms.

### S-13 — Cache hit ratio degradation > 20 pp

- **Severity:** `medium`
- **Status:** ⚠️ planned / not-yet-instrumented — `corelink_cache_hit_ratio` has no emitter in the codebase yet.
- **What it means.** Tenant's 7-day rolling cache hit ratio has dropped by more than 20 percentage points compared with their trailing 28-day baseline. Hit ratio is the headline value metric for CoreLink — degradation directly erodes ROI and triggers renewal scrutiny.
- **Detection (PromQL):**
  ```promql
  (
    avg_over_time(corelink_cache_hit_ratio[7d])
    -
    avg_over_time(corelink_cache_hit_ratio[28d] offset 7d)
  ) < -0.2
  ```
- **Action SLA:** 72 h to AE / CSE touch with technical diagnostic attached (region pin mismatch? retention policy too short? CI config drift?).
- **Source of truth:** Prometheus (`corelink_cache_hit_ratio` per-tenant).
- **False-positive rate:** medium — sometimes signals a benign workload change (new project, new artifact size distribution).

### S-14 — Contract auto-renewal opt-out flag set

- **Severity:** `high`
- **What it means.** Tenant admin has clicked "do not auto-renew" in the billing dashboard. Most literal possible signal — they have already decided. Save play has the highest urgency and the broadest authority.
- **Detection (SQL on billing DB):**
  ```sql
  SELECT tenant_id, subscription_id, opt_out_at, current_term_end
  FROM stripe_subscriptions
  WHERE auto_renew = false
    AND opt_out_at > NOW() - INTERVAL '30 days'
    AND current_term_end > NOW();
  ```
- **Action SLA:** 24 h to CEO touch.
- **Source of truth:** Stripe + billing DB (`crates/corelink-tier-selection/src/stripe.rs`).
- **False-positive rate:** very low — explicit customer action.

### S-15 — BYOK kill-switch armed

- **Severity:** `high`
- **Status:** ⚠️ planned / not-yet-instrumented — `corelink_byok_killswitch_armed` has no emitter in the codebase yet.
- **What it means.** (Enterprise only.) Tenant has armed the BYOK kill-switch (per [`specs/_runbooks/RB-DPA-CHANGE.md`](../../specs/_runbooks/RB-DPA-CHANGE.md) and the BYOK chaos drill cadence). The kill-switch is a legitimate operational primitive, but arming it outside a scheduled drill window indicates either a security incident OR a deliberate "we are getting ready to leave."
- **Detection (PromQL):**
  ```promql
  max by (tenant_id) (
    corelink_byok_killswitch_armed{drill="false"}
  ) == 1
  ```
- **Action SLA:** 24 h to CEO touch + parallel Security / DPO notification per [`specs/_runbooks/RB-DPO-ESCALATION.md`](../../specs/_runbooks/RB-DPO-ESCALATION.md).
- **Source of truth:** Prometheus (`corelink_byok_killswitch_armed` gauge with `drill` label).
- **False-positive rate:** low — drill-vs-real labeling is enforced at arming time.

---

## Signal severity distribution

| Severity | Count | Examples |
|---|---|---|
| `high`   | 8 | S-01, S-02, S-03, S-04, S-06, S-10, S-14, S-15 |
| `medium` | 6 | S-05, S-07, S-08, S-09, S-11, S-13 |
| `low`    | 1 | S-12 |
| **Total** | **15** | (target ≥ 12) |

---

## Operational notes

- **Signals are AND'd at triage, not OR'd at fire time.** Multiple concurrent signals on the same tenant escalate severity automatically — see [`RB-CHURN-RISK-RESPONSE.md`](../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md) §4.
- **Every signal is DESIGNED as a Prometheus alerting rule OR a HubSpot workflow OR a SQL-on-cron job — no manual scraping.** As of this writing the six Prometheus-based signals (S-01, S-05, S-06, S-10, S-13, S-15, see per-signal Status notes above) are not yet instrumented; the SQL/HubSpot-based signals are the ones actually queryable today. The owning team is on the signal in [`RB-CHURN-RISK-RESPONSE.md`](../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md) §2.
- **Quarterly tuning.** See [`quarterly-review-template.md`](./quarterly-review-template.md) for the rolling KPI dashboard that drives signal addition / removal / threshold adjustment. Default policy: a signal with >70% false-positive rate over a quarter is retired or re-thresholded; a missed-churn that did not fire any signal in 30 d window prior triggers a new candidate signal.
- **Privacy boundary.** No signal exports tenant content. All signals operate on metadata (counts, ratios, timestamps, billing state). The audit chain query rate (S-05) is itself observed via audit-on-audit metrics — we observe that an export *happened*, not what was exported.

---

## Cross-references

- Per-signal response plays: [`RETENTION-PLAYBOOK.md`](./RETENTION-PLAYBOOK.md)
- Customer-facing cancellation flow design: [`CANCELLATION-FLOW.md`](./CANCELLATION-FLOW.md)
- Quarterly KPI review template: [`quarterly-review-template.md`](./quarterly-review-template.md)
- Internal operations runbook: [`../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md`](../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md)
- Lighthouse engagement playbook: [`../lighthouse-kit/CUSTOMER-PLAYBOOK.md`](../lighthouse-kit/CUSTOMER-PLAYBOOK.md)
- Sales objection handling (renewal context): [`../sales/OBJECTION-HANDLING.md`](../sales/OBJECTION-HANDLING.md)
- GA post-launch wave: [`../../ROADMAP-TO-GA.md`](../../ROADMAP-TO-GA.md) §8
