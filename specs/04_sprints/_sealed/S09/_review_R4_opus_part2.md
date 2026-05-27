---
id: "AUDIT-R4-OPUS-S09-PART2"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "FRAMEWORK-00"
  - "OBSERVABILITY-MODEL"
  - "SLO-CATALOG"
  - "INVARIANT-REGISTRY"
tags: ["audit", "review", "r4", "opus", "s09", "observability", "adversarial", "lote-10.9", "part2", "wi-005-to-007"]
---

# R4 (Opus) — S-09 Adversarial Review · Part 2 (WI-S09-005..007)

> **Reviewer persona:** R4 — Opus 4.7 1M-context architectural reviewer
> **Scope:** S-09 work items 5..7 (12 Grafana dashboards-as-code, multi-burn-
> rate SLO alerts + PagerDuty, synthetic canary 3 regions + runbook dry-run)
> **Companion:** `_review_R4_opus_part1.md` covers WIs 1..4
> **Cross-cut:** Sonnet-persona testability review in `_review_R5_sonnet_part{1,2}.md`
> **Framework citations:** Google SRE Workbook Ch 5 Table 4 (multi-burn-rate
> 14.4×/6×/3×/1×); Ch 8 alert discipline; PagerDuty Events API v2 §dedup_key
> idempotency; `slo_catalog.md §4`; `observability_model.md §10` Nível-3
> canonical dashboards.

---

## 0. Executive summary

**Aggregate SOTA score (WI-005..007): 8.4 / 10.** Slightly below Part 1's
8.7. The dashboards-as-code piece (WI-S09-005) is genuinely the *strongest*
single WI in the S-09 corpus — 12 canonical JSON dashboards aligned with
`observability_model.md §10` Nível-3, panel discipline ≥ 8/dashboard,
template-variable canonical set (`region` / `tenant_tier` / `tenant`),
`cardinality_budget_used` annotation inheriting from WI-001, freshness CI
hook via `validate_dashboards.py` — this is what dashboards-as-code SHOULD
look like and what I rarely see in audit. The PRR-S09 §8 "calibrated
praise" is deserved.

The two weaker WIs are S09-006 (multi-burn-rate alerts) and S09-007
(synthetic canary). Both are correct on the *core math* — Google SRE
Workbook Ch 5 Table 4 multipliers are pinned by `prop_alert_decision_
canonical_table4` 10k iter; canary digest match precedence is pinned by
`prop_canary_digest_correctness`. The weaknesses are in the **integration
surface** — flapping detector calibration is hand-tuned without sensitivity
analysis (P1-B), PagerDuty service-key cardinality discipline (3 services per
Lote 10.9bis P0-F correction from 5) has a residual cross-region tenant edge
case (P2-D), synthetic canary's "12_960 loops/72h" target (Lote 10.9bis
P0-A corrected from triple-counted 38_880) has a freshness budget vs alert-
fire SLA tension that is not analytically resolved (P1-E).

Headline P0/P1 cluster:

1. **P1-A — AdminCtx-RBAC drill-down ACL is host-side-only.** The 12
   dashboards ship with template variables `region` / `tenant_tier` /
   `tenant` for AdminCtx-aware drill-down, but the Grafana datasource
   permission layer that *enforces* the ACL (preventing a Pro-tier tenant
   admin from seeing Enterprise-tenant detail rows in DASH-TENANT) is NOT
   shipped at SEAL — it depends on Grafana datasource permissions wired
   into the production tenant. WI-005 §6 (per the title text) says "AdminCtx
   RBAC datasource permissions" but the underlying provisioning is forward-
   debt.
2. **P1-B — Alert flapping detector calibration ↔ ticket SLO interaction.**
   WI-006 §0 "auto-quarantine flapping alerts > 3×/week" is the right
   discipline, but the *interaction* with the multi-burn-rate 4-window
   ticket arm is unspecified: a Slow24h ticket that flap-and-rearms 4× in
   a week (legitimate SLO miss + recovery + miss + recovery) would auto-
   quarantine, suppressing the *real* SLO breach signal. Need explicit
   carve-out: quarantine applies to **page** arm only, not ticket arm.
3. **P1-C — Terraform provisioning gap for 12 dashboards + 3 PagerDuty
   services.** WI-S09-005 title cites
   "Terraform-provisioned via `cloudflare/terraform-provider-grafana`" but
   no Terraform module ships at SEAL. WI-S09-006 cites "3 PagerDuty services
   per environment" but no Terraform module for PagerDuty either. The
   trait-abstraction-defer pattern is appropriate for the runtime Rust
   crates, but Terraform IaC is the **provisioning** surface — its absence
   means a destructive Grafana restore cannot recreate the 12 dashboards
   from canonical source. This is a SRE-readiness gap, not a build-system
   gap.
4. **P1-D — Synthetic canary tenant_id exclusion from SLI is enforced by a
   PromQL recording rule.** WI-006 ships a recording rule filter excluding
   `tenant_id="00000000-0000-0000-0000-canary000000"` from SLI denominators.
   `prop_synthetic_tenant_excluded_from_sli` 10k iter pins the discipline at
   the Rust code level — but the **Prometheus recording rule itself** is
   defined in YAML; if a future operator edits the YAML and removes the
   filter, the property test does NOT catch the drift. Need a
   `validate_alert_rules.py` lint that enforces the tenant-exclusion clause
   in every SLI recording rule.

If the dev applies all 4 P1 findings, projected score for Part 2:
**9.0 / 10.**

## 1. Per-WI scores

| WI | Score | Strongest aspect | Headline weakness |
|---|---|---|---|
| **WI-S09-005** 12 Grafana dashboards-as-code | **9.2 / 10** | 12 canonical JSON dashboards aligned with `observability_model.md §10` Nível-3 (Lote 10.9-quaters NEW-P0-1); panel count discipline; template-variable canonical set; `cardinality_budget_used` annotation; `validate_dashboards.py` freshness CI gate; canonical-12 count discipline enforced structurally | **P1-A AdminCtx-RBAC ACL is host-side-only** (Grafana datasource permissions deferred); **P1-C Terraform provisioning gap**; INV drift triple (see Part 1 §3 P1-1) lives in WI-005-12 |
| **WI-S09-006** Multi-burn-rate SLO alerts + PagerDuty | **8.0 / 10** | Google SRE Workbook Ch 5 Table 4 multipliers 14.4×/6×/3×/1× pinned by `prop_alert_decision_canonical_table4` 10k iter; PagerDuty Events API v2 §dedup_key contract; 7 SLI × 4 windows = 28 alert rules + 7 recording rules canonical | **P1-B alert flapping ↔ ticket SLO interaction**; **P1-D recording rule lint missing**; `promtool test rules` CI gate deferred per charter; auto-quarantine via PagerDuty API + post-mortem mandatory has no shell |
| **WI-S09-007** Synthetic canary 3 regions + runbook dry-run | **8.0 / 10** | 3-region canonical (Enam + Weur + Apac per `data_model.md §2.1` R2 region hints — Lote 10.9bis P1 R4 P1-10 corrected from IATA colocodes); BLAKE3 digest match precedence pinned by `prop_canary_digest_correctness`; assertion ladder `cas_put_p99 ≤ 100 / cas_get_p99 ≤ 50 / ac_lookup_p99 ≤ 30` | **P1-E 12_960 loops/72h ↔ alert-fire SLA tension unresolved**; CF Workers cron-trigger 24/7 sustained 72h is the DoD ship gate; production binding deferred to staging-account provisioning |

**Aggregate (mean): 8.4 / 10.**

## 2. P0 Findings (must-fix Lote 10.9bis)

**None.** WIs 005..007 do not contain catastrophic-if-deployed defects. The
multi-burn-rate Table 4 math is pinned, the canary digest match precedence
is pinned, the dashboards JSON parses. P1 cluster is the actionable lot.

## 3. P1 Findings

### P1-A — AdminCtx-RBAC drill-down ACL is host-side-only

**Severity:** P1 — the privacy claim ("Per-tenant labels in DASH-TENANT
redacted via Grafana datasource permissions (AdminCtx-gated)") cannot be
verified at SEAL because the datasource-permission provisioning is forward-
debt.

**Defect:** WI-S09-005 title cites "AdminCtx RBAC datasource permissions"
and PRR-S09 §2 row 10 (Privacy Officer waiver) cites
"Per-tenant labels in DASH-TENANT redacted via Grafana datasource
permissions (AdminCtx-gated)". The Grafana side provisioning for
datasource-permission rules (which gate `tenant` template-variable values
by AdminCtx role) is NOT in the dashboards JSON nor in a Terraform module.
The 12 dashboards JSON has the template variable definition but the ACL
that constrains its **values** lives in Grafana's `datasources/<id>/
permissions` API — which is unprovisioned at SEAL.

**Concrete remediation:**

1. Add Terraform module
   `infra/grafana/datasource_permissions.tf` provisioning
   `grafana_data_source_permission` resources for each AdminCtx role
   (TenantAdmin, RegionalAdmin, GlobalAdmin), constraining `tenant`
   variable values via Grafana's [variable query
   filtering](https://grafana.com/docs/grafana/latest/dashboards/variables/variable-syntax/).
2. Add a `validate_dashboards.py` lint that rejects any dashboard with a
   `tenant` template variable but no `datasource_uid` field constraining
   it to an AdminCtx-gated datasource.
3. Add S-13 admin plane unblocking dependency: the AdminCtx-RBAC mid-tier
   resolver (S-13 forward scope) feeds the Grafana datasource permission
   layer.

### P1-B — Alert flapping detector ↔ ticket SLO interaction unspecified

**Severity:** P1 — the "auto-quarantine flapping > 3×/week" rule from
WI-006 §0 + sprint contract §14.s09.2 risks suppressing the legitimate
slow-burn ticket signal.

**Defect:** SRE Workbook Ch 8 alert discipline says: flapping = same alert
fires + clears + re-fires repeatedly within a short window. The 4-window
multi-burn-rate canonical PRODUCES exactly this pattern on a long-running
slow burn (Slow24h fires; budget recovers slightly; Slow24h re-fires). On
a 7d sustained slow-burn, the same Slow24h alert can fire-and-clear 4× in
a week — which crosses the 3×/week quarantine threshold. The legitimate
ticket SLO signal gets quarantined.

The Google SRE Workbook explicitly carves out: flapping discipline applies
to **page** arm (immediate human action) only, not **ticket** arm
(business-hours triage). WI-006 §0 cites the discipline without the carve-
out.

**Concrete remediation:**

1. WI-006 §6: explicit carve-out — quarantine applies only to alert rules
   with severity ∈ {SEV-0, SEV-1} (page arms); SEV-2/SEV-3 (ticket arms)
   are exempt.
2. Add `prop_flapping_detector_carves_out_ticket_arm` 10k iter pinning
   the discipline.
3. Cross-reference SRE Workbook Ch 8 §"Tickets, Pages, and Alerts" in §6.

### P1-C — Terraform provisioning gap for 12 dashboards + 3 PagerDuty services

**Severity:** P1 — destructive Grafana restore / PagerDuty replay cannot
recreate the production observability state from canonical source.

**Defect:** WI-S09-005 title cites `cloudflare/terraform-provider-grafana`
provisioning; WI-S09-006 cites 3 PagerDuty services per environment. No
Terraform module ships at SEAL. The 12 dashboards JSON exists in
`dashboards/grafana/*.json` but a destructive Grafana restore needs a
`grafana_dashboard` resource per file to recreate them with proper UID +
folder + datasource UID assignment. Similarly, PagerDuty service config
(escalation policy, integration key, custom-action-on-incident) is
provisioning state, not application state.

**Concrete remediation:**

1. Add Terraform module `infra/grafana/dashboards.tf` with one
   `grafana_dashboard` resource per JSON file, sourcing the JSON via
   `file()`.
2. Add Terraform module `infra/pagerduty/services.tf` with 3 service
   resources (staging, prod-us, prod-eu) + escalation policy + integration
   key sourced from secrets manager.
3. Add a `validate_terraform.py` CI gate that diffs the Terraform-managed
   dashboard list against the canonical `dashboards/grafana/*.json` count
   (must be 12).
4. Forward-whitelist to S-19 onboarding (which owns customer-facing
   provisioning) is acceptable IF the gap is explicit in §1 NOTE block.

### P1-D — SLI recording rule lint missing for synthetic-tenant exclusion

**Severity:** P1 — the property test pins the Rust-code-level discipline,
but the actual SLI denominator is computed by a PromQL recording rule —
which is YAML and not covered by the property test.

**Defect:** WI-006 ships a recording rule of the form
`sli:cas_put_success:ratio_5m = sum(rate(cas_put_total{tenant_id!="00000000-0000-0000-0000-canary000000",result="success"}[5m])) / sum(rate(cas_put_total{tenant_id!="00000000-0000-0000-0000-canary000000"}[5m]))`.
The `tenant_id != "00000000-0000-0000-0000-canary000000"` clause is the
synthetic-tenant exclusion. `prop_synthetic_tenant_excluded_from_sli` 10k
iter pins the *intent* at the Rust code, but if a future operator edits
the YAML and removes the clause (or typos it as `00000000-0000-0000-0000-
canary00000` — one zero short), the recording rule silently inflates the
denominator with canary traffic and the SLI tilts.

**Concrete remediation:**

1. Add `scripts/validate_alert_rules.py` lint that scans all SLI recording
   rules in `dashboards/alerts/*.yml` and rejects any rule with
   `tenant_id` in label-set without the exact canary exclusion clause.
2. Pin the canary tenant UUID as a const in WI-006 §6 (Rust) AND in the
   YAML (require lint cross-reference).
3. Add a unit test that loads the YAML and asserts the clause appears in
   each SLI recording rule.

### P1-E — Synthetic canary 12_960 loops/72h ↔ alert-fire SLA tension

**Severity:** P1 — the 12_960 loops target (Lote 10.9bis P0-A corrected
from 38_880 triple-counted) implies a 1 loop per ~20s cadence across 3
regions. The DoD §4 row "MTTA < 5min synthetic 7d" implies that any canary
assertion failure must surface as a PagerDuty page within 5 minutes. At 20s
cadence with multi-burn-rate Fast1h alert (requires 14.4× burn over 5min
short window), a SINGLE canary loop failure (one assertion failed) over a
5min window represents 1/15 = 6.67% failure rate which is *insufficient* to
trigger Fast1h burn (assumes SLO target 99%, so 14.4× burn = burning 14.4 *
1% = 14.4% of the budget per hour; one failure in 15 samples in 5min = 6.67%
error rate = 6.67× burn rate over the 5min window — *just shy of 14.4×
threshold*).

This is the *correct* math — Google SRE Workbook Table 4 multipliers are
designed to avoid false positives. But the *consequence* for synthetic
canary is: a single canary failure does NOT page in 5min via SLO multi-burn-
rate. It pages via the *direct* canary alert
(`corelink_canary_assertion_failures_total{region,assertion}` SEV-2 alert on
3+ consecutive in same region) — which takes 3 consecutive failures × 20s
cadence = 60s + alert latency = 2-3min, OK for the 5min MTTA target. But
the WI-006 ↔ WI-007 interaction is not analyzed: the multi-burn-rate alert
is for SLI breach (production tenant traffic), while the canary alert is
for canary-tenant traffic. These should be *separate* alert paths, but
that distinction is not explicit in WI-006 §6.

**Concrete remediation:**

1. Add WI-006 §6 explicit separation: SLI multi-burn-rate alerts EXCLUDE
   canary tenant; canary alerts are a separate alert family
   (`corelink_canary_*`).
2. Document the canary cadence ↔ alert latency budget in WI-007 §6:
   "20s cadence × 3 consecutive failure → 60s detection + alert dispatch
   latency budget ≤ 4min → MTTA target 5min".
3. Add `prop_canary_alert_path_separate_from_sli_alert_path` 10k iter
   pinning the dual-path discipline.

## 4. P2 Findings

### P2-A — `validate_dashboards.py` panel count discipline is structural, not semantic

WI-S09-005 ships `validate_dashboards.py` enforcing panel count ≥ 8 per
dashboard. The count is structural — it does not verify that the panels are
*relevant*. A dashboard could ship 8 placeholder panels and pass. Suggest
adding panel-title canonical-pattern lint (e.g. DASH-CAS must have at least
one panel titled `^.*cas.*put.*$`).

### P2-B — Cross-region tenant on PagerDuty service-key cardinality

WI-S09-006 ships 3 PagerDuty services (staging / prod-us / prod-eu). The
3-canonical (Lote 10.9bis P0-F correction from 5) is correct for the
regional split, BUT a tenant spanning prod-us AND prod-eu (multi-region
Enterprise tenant) generates pages on BOTH service keys. The dedup_key
construction `{sli_slug}:{window_slug}:{tenant_id}` would treat
prod-us and prod-eu pages as DUPLICATES (same dedup_key, different service
key), which is the right behavior — but the SEV escalation discipline gets
weird (which region's on-call gets the page?). Suggest documenting in §6:
multi-region tenant pages → page BOTH regional on-calls, NOT dedup'd at
the dedup_key level (different service keys = different dedup spaces).

### P2-C — Dashboard freshness CI hook is binary (parses / not)

`validate_dashboards.py` enforces JSON parses + tag presence +
`lastUpdated` ISO 8601 annotation. The `lastUpdated` field is checked for
*format*, not for *recency*. A dashboard with `lastUpdated: 2024-01-01` is
still valid. Suggest adding a recency check: `lastUpdated` must be within
90 days of `created` for canonical dashboards, with explicit `frozen:
true` carve-out for stable dashboards.

### P2-D — Runbook dry-run automation drift

WI-S09-007 §1 ships `scripts/rb_fm_153_dry_run.sh` + `scripts/rb_obs_
cardinality_001_dry_run.sh` host-side. The "drift-detectable" claim is
appropriate, but the drift detection mechanism is unspecified — what
happens if the runbook MD adds a step but the dry-run script doesn't?
Suggest cross-validating the step count between the runbook markdown
(parse `## Step N` headers) and the script (count `echo "Step N"`
markers).

## 5. P3 Findings

### P3-A — `cardinality_budget_used` annotation depends on a metric WI-001 emits

DASH-CAS, DASH-TENANT, DASH-EXEC reference
`cardinality_budget_used{metric}` as a gauge annotation. WI-001 emits this
gauge. The dependency is correct but the *failure mode* is not documented:
what does the dashboard render if the gauge has no data (no isolate warm,
no traffic)? Suggest documenting "no data" → gray panel with "no traffic"
note.

### P3-B — Alert label cardinality not bounded by recording rules

The 35 alert rules (28 + 7 recording) have label sets `{sli, window,
tenant_id, region}`. If a future operator adds `decision` to the alert
label set without checking, the alert-side cardinality grows. Suggest a
recording-rule label allowlist + lint.

### P3-C — RB-OBS-CARDINALITY-001 dry-run injection of 25k synthetic series

The runbook dry-run injects 25k synthetic series to test cardinality
rejection. Per the 20k per-metric budget, this OVERSHOOTS by 5k. Suggest
clarifying in the runbook: 25k injection is intentional (validates the
rejection path, not the acceptance path) and label as RED-PATH-TEST in
the runbook narrative.

## 6. Cross-WI consistency claims verified

| Claim | Status | Notes |
|---|---|---|
| 12 canonical dashboards aligned with `observability_model.md §10` Nível-3 | **PASS** | Lote 10.9-quaters NEW-P0-1 correction landed; 12 JSON files in `dashboards/grafana/`; 3 legacy retained (DEDUP/RATE/MULTIPART) appropriately scoped. |
| 35 total alert rules canonical (28 + 7) | **PASS** | Lote 10.8-tris P1-NEW-1 rigorous count discipline; YAML parses; promtool deferred. |
| PagerDuty 3 services per environment | **PASS** | Lote 10.9bis P0-F correction from 5 → 3 (per sprint §5.5 R-S09-14 canonical 3). |
| 12_960 loops/72h target (canary) | **PASS** | Lote 10.9bis P0-A correction from 38_880 triple-counted; explicit in PRR-S09 §8. |
| 3-region canonical (Enam + Weur + Apac per R2 region hints) | **PASS** | Lote 10.9bis P1 R4 P1-10 correction from IATA colocodes; aligned with `data_model.md §2.1`. |
| Synthetic canary tenant_id exclusion from SLI | **PASS in Rust property test; FAIL in YAML recording rule lint** | See P1-D above. |
| Multi-burn-rate Google SRE Workbook Table 4 multipliers 14.4×/6×/3×/1× | **PASS** | `prop_alert_decision_canonical_table4` 10k iter; cross-validated. |
| AdminCtx-RBAC drill-down ACL | **FAIL (host-side only)** | See P1-A above. |
| Terraform IaC for dashboards + PagerDuty | **FAIL** | See P1-C above. |
| Dashboard freshness CI hook | **PARTIAL** | Binary parse check; recency lint missing (P2-C). |

## 7. Recommendation

**CONDITIONAL APPROVE** for WIs 005..007 → STAGING-STABLE *contingent* on:

- **Block S-20 GA** until P1-A (AdminCtx-RBAC ACL) provisioning lands;
- **Block S-20 GA** until P1-C (Terraform IaC for dashboards + PagerDuty)
  lands — these are SRE-readiness gaps;
- P1-B (flapping ↔ ticket interaction), P1-D (recording rule lint), P1-E
  (canary cadence ↔ alert SLA) may land in **Lote 10.9bis remediation
  wave** within ≤ 4 weeks post-SEAL.

Per Lote 10.9 review charter: this report is an AUDIT artifact (`type:
audit`, `doc_status: REVIEW`) and does NOT modify WI-S09-005..007 doc_status
(which remain FROZEN per the original SEAL). Remediation lands as a
separate Lote 10.9bis WI authored against this report + Part 1.

**Calibrated praise:** WI-S09-005 is the strongest dashboards-as-code piece
in the CoreLink corpus. The 12 canonical dashboards aligned with
`observability_model.md §10` Nível-3, with structural panel-count discipline,
template-variable canonical set, and the `cardinality_budget_used`
annotation inheritance — this is *production-grade* SRE observability and
deserves the 9.2/10. The two weaker WIs (006 / 007) are weaker on
**integration-edge**, not on **first principles** — the core math is
correct, and the property tests pin the right falsifiability targets.

---

**End R4 Opus Part 2 v1.0.0.**
