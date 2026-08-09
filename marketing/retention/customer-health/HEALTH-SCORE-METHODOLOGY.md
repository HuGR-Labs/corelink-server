---
id: "RETENTION-HEALTH-SCORE-METHODOLOGY"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "VPMkt + CS Lead (dual)"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "ROADMAP-TO-GA.md §R-prep (post-GA retention)"
tags:
  - "marketing"
  - "retention"
  - "customer-health"
  - "scoring"
  - "methodology"
  - "nps"
  - "post-ga"
  - "wt-r-prep-customer-health"
---

# Customer Health Score — Methodology

> **Purpose:** define a **deterministic, reproducible** algorithm that converts six signals from a tenant's last 30 days of activity into a single integer `health_score ∈ [0, 100]` and a categorical `health_tier ∈ {Healthy, At-Risk, Critical}`.
> **Audience:** CS team (consumers), VPMkt + CS Lead (owners), Eng/Data (implementers), Founder (review).
> **Companion:** `CHURN-RISK-SIGNALS.md` (sibling worktree `wt-churn-retention`) enumerates the raw risk signals; this doc weights them into the score. `HEALTH-DASHBOARD-SPEC.md` consumes the output; `NPS-SURVEY-SCHEDULE.md` feeds one of the six inputs; `CSM-PLAYBOOK.md` acts on the tier transitions.
> **Hard rule:** **the score MUST be reproducible from public-to-CS inputs alone**. No subjective "vibes" inputs. If the algorithm ever has to be tuned, the tuning is a versioned spec change (`version: 1.1.0`) with rationale + before/after distribution check.
> **Instrumentation status.** This is a **design methodology, not a live pipeline**. The `health_score_audit` table (§1 rule 8, §7 Implementation contract) does **not exist in the D1 schema today** — no nightly job computes or writes scores yet. CS should not expect a live health score / dashboard until the implementation contract in §7 is actually built and deployed.

---

## 1. Design principles

1. **Deterministic** — given the same 30-day input window, the score MUST be identical regardless of who runs the calculation.
2. **Transparent** — every CS rep can hand-compute the score for any tenant from the dashboard's "inputs" panel. No hidden ML.
3. **Bounded inputs** — every input is normalised to `[0, 100]` before weighting. No raw counts in the formula.
4. **Recency-weighted** — the 30-day window is fixed; longer trends surface via the `trajectory` input (slope of last 30d vs prior 30d).
5. **Tier transitions are sticky** — to prevent flapping between tiers, a tenant entering `At-Risk` or `Critical` requires **2 consecutive daily scores** below the threshold (anti-flap); leaving requires **3 consecutive daily scores** above the threshold.
6. **No surprise downgrades** — if a tenant drops by >15 points day-over-day, CS Lead receives an alert (see `HEALTH-DASHBOARD-SPEC.md` §"Action queue").
7. **Versioned** — every change to weights, normalisation, or thresholds bumps `version:` in front matter and is logged in §9 (Change log).
8. **Auditable** — every nightly score computation writes one row to `health_score_audit` (tenant_id, score, tier, six normalised inputs, weights_version, computed_at). Retention 18 months. **⚠️ Planned / not-yet-instrumented: this table does not exist yet — see Instrumentation status note above.**

---

## 2. Inputs (6 signals, weighted)

| # | Input | Weight | Source | Normalised range |
|---|---|---:|---|---|
| 1 | **Usage trajectory** | **30%** | `cas_put_total + cas_get_total + ac_get_total` over last 30d vs prior 30d | 0 = collapse, 100 = >2× growth |
| 2 | **Feature adoption breadth** | **20%** | distinct product surfaces touched (CAS, AC, audit query, DSR, BYOK, dashboards, CLI subcommands) in last 30d | 0 = 1 surface, 100 = ≥6 surfaces |
| 3 | **Support friction** | **20%** | weighted ticket volume + SLA breaches over last 30d (inverted) | 0 = heavy friction, 100 = zero friction |
| 4 | **Payment reliability** | **15%** | invoice-on-time rate + dunning depth over last 90d | 0 = chargeback / unrecoverable dunning, 100 = all on-time |
| 5 | **Engagement** | **10%** | distinct human logins to dashboard + audit-chain queries over last 30d | 0 = no human activity, 100 = ≥3 humans active weekly |
| 6 | **Explicit NPS score** | **5%** | most recent NPS response in last 90d (see `NPS-SURVEY-SCHEDULE.md`) | 0 = NPS detractor 0, 100 = NPS promoter 10 |

**Weights sum to 100%.** Total weight allocation rationale:

- **30% Usage trajectory** — strongest single predictor of renewal. Customers ramping usage almost never churn within 90 days.
- **20% Feature adoption breadth** — a tenant using only CAS is 3× more likely to churn than one using CAS + audit + AC (empirical from lighthouse + Forge customer zero observations; will be re-baselined at v1.1 once we have ≥20 paid tenants).
- **20% Support friction** — recurring tickets and SLA breaches predict frustration → churn. Inverted because lower friction = higher score.
- **15% Payment reliability** — invoice / dunning state is a **late** signal but a **certain** one. Weighted lower because by the time it moves, churn is often already decided.
- **10% Engagement** — human-level activity (logins + audit queries) catches "the build pipeline is still pushing artifacts but no one is watching" scenarios — common pre-churn pattern.
- **5% Explicit NPS** — small weight because (a) NPS response rates are typically 20–40% so the signal is sparse, and (b) survey-fatigue + courtesy-promoter effects make NPS noisy at low n. NPS is more useful as a **trigger** (low NPS → CSM outreach) than as a score component.

> If NPS is missing for a tenant in the last 90d, **redistribute the 5% weight proportionally across the other 5 inputs** (each gets +1% absolute) and flag the score as `nps_missing: true` in the audit row. Do not impute NPS.

---

## 3. Normalisation — exact formulas

Each input is computed over a fixed 30-day window ending at the score computation timestamp (typically nightly 02:00 UTC).

### 3.1 Usage trajectory `U` ∈ [0, 100]

Let `cur` = sum of `cas_put_total + cas_get_total + ac_get_total` events in `[t-30d, t)`.
Let `prev` = same sum in `[t-60d, t-30d)`.

```
ratio = cur / max(prev, 1)   # avoid div-by-zero; new tenants get ratio based on cur alone
```

Piecewise linear mapping:

| Ratio | U |
|---|---|
| ≤ 0.25 | 0 |
| 0.25 → 0.50 | linear 0 → 25 |
| 0.50 → 0.80 | linear 25 → 50 |
| 0.80 → 1.20 | linear 50 → 75 (stable band) |
| 1.20 → 2.00 | linear 75 → 100 |
| ≥ 2.00 | 100 |

**New tenants (<30d old):** `U = 60` (neutral-positive) for the first 30 days. Switches to the formula above on Day 31. Rationale: too noisy to score; default to "in observation".

### 3.2 Feature adoption breadth `B` ∈ [0, 100]

Count of distinct **product surfaces** with at least 1 event in last 30d. Surfaces:

1. `cas` — any CAS read/write event (`corelink put|get|stat|ls`, `corelink cas get|export`)
2. `ac` — any action-cache event
3. `audit` — any audit read event (`corelink audit tail|export|verify`, or `/v1/audit/*`)
4. `dsr` — any DSR endpoint hit (own DSR, not third-party)
5. `byok` — any BYOK key reference (Enterprise only)
6. `dashboard` — any human login to admin-ui
7. `cli` — any CLI subcommand invocation outside the `cas`/`ac` surfaces (e.g., `corelink config`, `corelink doctor`, `corelink tenant export`)

Map count → `B`:

| Distinct surfaces | B |
|---|---|
| 0 | 0 (impossible if tenant is live; flag data error) |
| 1 | 20 |
| 2 | 40 |
| 3 | 60 |
| 4 | 75 |
| 5 | 90 |
| ≥ 6 | 100 |

**Team-tier tenants** are capped at 5 surfaces (no BYOK). Their max `B` is 90, but the threshold mapping above is applied as-is — i.e., a Team tenant touching all 5 of its available surfaces scores `B = 90`, not 100. This is intentional: Enterprise BYOK customers using BYOK should score higher on breadth.

### 3.3 Support friction `S` ∈ [0, 100]

Compute friction index `F` over last 30d:

```
F = 1.0 * P3_count
  + 3.0 * P2_count
  + 9.0 * P1_count
  + 27.0 * P0_count
  + 5.0 * sla_breach_count
  + 2.0 * reopen_count
```

(Severity weights are exponential ×3 — a single P0 dominates a stack of P3s, by design.)

Map `F` → `S`:

| F | S |
|---|---|
| 0 | 100 |
| 1–3 | 90 |
| 4–8 | 75 |
| 9–18 | 50 |
| 19–40 | 25 |
| > 40 | 0 |

### 3.4 Payment reliability `P` ∈ [0, 100]

Compute over last **90 days** (longer window because billing is monthly).

```
P_base = 100 * on_time_invoice_count / total_invoice_count
```

Then apply dunning depth penalty:

| Dunning state (current) | Penalty |
|---|---:|
| None | 0 |
| Dunning-1 (1st reminder sent, < 7d overdue) | -20 |
| Dunning-2 (7–14d overdue) | -40 |
| Dunning-3 (14–30d overdue) | -70 |
| Pre-suspension (30–45d overdue) | -90 |
| Chargeback / unrecoverable | -100 (forces `P = 0`) |

```
P = clamp(P_base + penalty, 0, 100)
```

**Lighthouse customers** (free 6 months) and **trial tenants** (no invoice yet): `P = 80` (neutral-positive) until their first invoice cycle.

### 3.5 Engagement `E` ∈ [0, 100]

Count distinct human users (= non-service-account principals) who performed any of:

- Dashboard login
- Audit chain query (`corelink audit tail|export`, or `/v1/audit/*`)
- CLI invocation against a tenant-scoped command

over last 30 days, and the **weeks-active** ratio:

```
humans_30d = distinct principals
weeks_active = number of distinct weeks (of last 4) with ≥ 1 human action
E_humans = min(humans_30d * 25, 100)     # 4 humans → 100
E_weeks = weeks_active * 25              # 4 weeks → 100
E = 0.5 * E_humans + 0.5 * E_weeks
```

### 3.6 Explicit NPS `N` ∈ [0, 100]

Take the most recent NPS response in the last 90 days. NPS responses are integers 0–10.

```
N = NPS_score * 10
```

If multiple responses exist in window, take the **most recent** (not the average — recency matters more for retention).

If no NPS in window: see §2 missing-NPS rule (redistribute weights).

---

## 4. Composition — final score

```
health_score = round(
    0.30 * U
  + 0.20 * B
  + 0.20 * S
  + 0.15 * P
  + 0.10 * E
  + 0.05 * N
)
```

If NPS missing:

```
health_score = round(
    0.31 * U
  + 0.21 * B
  + 0.21 * S
  + 0.16 * P
  + 0.11 * E
)
```

(Sums to 1.00; weights inflated by `5/95` ≈ 5.26% each → rounded to nearest 1% above.)

Result clamped to `[0, 100]`.

---

## 5. Tiering

| Tier | Score range | Meaning | CSM action default |
|---|---|---|---|
| **Healthy** | 70–100 | On track. Likely to renew. | Quarterly check-in only (`CSM-PLAYBOOK.md` §3.1) |
| **At-Risk** | 40–69 | Mixed signals. Some friction or stagnation. | Within 7d: CSM outreach + investigation per `CSM-PLAYBOOK.md` §3.2 |
| **Critical** | 0–39 | Multiple failures. Churn likely in next 60d without intervention. | Within 48h: CSM Lead + Founder loop-in per `CSM-PLAYBOOK.md` §3.3 |

### 5.1 Anti-flap rules

- **Entering At-Risk** from Healthy requires **2 consecutive daily scores ≤ 69**. A single dip is recorded but does not move the tier label.
- **Entering Critical** from At-Risk requires **2 consecutive daily scores ≤ 39**.
- **Leaving At-Risk → Healthy** requires **3 consecutive daily scores ≥ 70**.
- **Leaving Critical → At-Risk** requires **3 consecutive daily scores ≥ 40**.
- **Skipping a tier** (Healthy → Critical or vice versa in one day) requires **5 consecutive days** at the destination tier — defensive against data outages or input bugs.

### 5.2 Day-over-day drop alert

If `score(today) − score(yesterday) ≤ -15`, fire alert `health.score.large_drop` regardless of tier transition. Alert routes to CS Lead Slack `#cs-alerts`. Does not change tier label.

---

## 6. Worked example

Tenant `acme-corp` on 2026-05-15 (hypothetical):

| Input | Raw | Normalised |
|---|---|---:|
| Usage trajectory | cur=12 200 events, prev=8 100 → ratio 1.51 | U = 87 |
| Feature adoption | CAS, AC, audit, dashboard, CLI → 5 surfaces | B = 90 |
| Support friction | F = 1×2 + 3×0 + 9×1 + 27×0 + 5×0 + 2×1 = 13 | S = 50 |
| Payment | 3/3 on-time, no dunning | P = 100 |
| Engagement | 6 humans, 4 weeks active | E = (100 + 100)/2 = 100 |
| NPS | latest response 9 (promoter), 18 days ago | N = 90 |

```
health_score = round(0.30*87 + 0.20*90 + 0.20*50 + 0.15*100 + 0.10*100 + 0.05*90)
             = round(26.1 + 18.0 + 10.0 + 15.0 + 10.0 + 4.5)
             = round(83.6)
             = 84
```

Tier: **Healthy**.

---

## 7. Implementation contract

| Layer | Behaviour |
|---|---|
| Job schedule | Nightly cron at 02:00 UTC. SLA: complete within 30 min for ≤ 10k tenants. |
| Idempotency | Re-running for the same `(tenant_id, computed_at_date)` MUST produce identical output and be a no-op write. |
| Input source | Postgres read-replica `analytics.events_30d_agg`; ticket queue API; billing service API. |
| Output table | `cs.health_score_audit` — columns `(tenant_id, computed_at, score, tier, U, B, S, P, E, N, nps_missing, weights_version)`. Append-only. Retention 18 months. **⚠️ Planned / not-yet-instrumented — table does not exist in the D1 schema today.** |
| Failure mode | If any input is unavailable, **do not score**. Write a row with `score = NULL, tier = NULL, error = '<reason>'` and alert `health.score.compute_failed`. Last-known-good tier is preserved on the dashboard until next successful run. |
| Backfill | A weights or formula change triggers a backfill of last 30 days under the new version. Both `weights_version` rows coexist; the dashboard reads the latest version. |
| Privacy | Score and inputs are tenant-internal data, not customer-visible by default. Lighthouse + Enterprise customers can request to see their own score quarterly (see `CSM-PLAYBOOK.md` §6 Transparency-on-request). |

---

## 8. Calibration & re-baselining

The v1.0 weights are **expert-designed, not data-fit** — we don't have enough paying tenants yet to fit weights statistically. The plan to re-baseline:

| Milestone | Action |
|---|---|
| ≥ 10 paid tenants for ≥ 90d | Distribution check: plot score histogram. Expect mode in 70–85. If mode is < 50 or > 95, formula or weights are off — investigate before tuning. |
| ≥ 20 paid tenants for ≥ 180d AND ≥ 2 churn events | Logistic regression: predict `churned_within_90d` from the 6 inputs. Compare model weights to v1.0. If discrepancy > 10pp on any single input, propose v1.1 with rationale. |
| ≥ 50 paid tenants for ≥ 365d | Full re-baseline: re-derive weights from data. Sunset v1.0 weights after running v1.0 + v1.1 in parallel for 30d. |

Every re-baseline:
1. Bumps `version:` in front matter (1.0.0 → 1.1.0, etc.).
2. Adds an entry to §9 Change log with before/after distributions.
3. Replays last 90d under new weights to confirm no surprise tier flips for healthy tenants.

---

## 9. Change log

| Version | Date | Author | Change | Rationale |
|---|---|---|---|---|
| 1.0.0 | 2026-05-15 | VPMkt + CS Lead | Initial expert-designed weights (30/20/20/15/10/5). | No production data yet; weights informed by lighthouse + Forge experience. |

---

## 10. Cross-references

- **`CHURN-RISK-SIGNALS.md`** (sibling worktree `wt-churn-retention`): enumerates raw signals; subset feeds this score (specifically support friction, payment reliability, engagement). Tier transitions trigger signals listed there.
- **`NPS-SURVEY-SCHEDULE.md`**: feeds input §3.6 (NPS).
- **`HEALTH-DASHBOARD-SPEC.md`**: consumes `cs.health_score_audit` output (planned / not-yet-instrumented — see status note above).
- **`CSM-PLAYBOOK.md`** §3: tier-keyed playbook actions.
- **`specs/_runbooks/RB-CUSTOMER-SUPPORT-T-90.md`** §3 (severity matrix): provides the P0–P3 + SLA-breach + reopen counters consumed by §3.3.
- **`marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`** §Comms protocol: provides comms-tier overrides for lighthouse customers (they have named CS engineers regardless of score).
- **`ROADMAP-TO-GA.md`** §R-prep: this doc is part of the post-GA retention preparation package.

---

**Fim HEALTH-SCORE-METHODOLOGY.**
