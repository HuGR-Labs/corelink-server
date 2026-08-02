---
id: "LIGHTHOUSE-KIT-CASE-STUDY-TEAM-A-FORGE"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S20-004"
tags: ["lighthouse", "marketing", "case-study", "template", "team-tier", "forge", "customer-zero", "r5-2"]
---

# Case Study — Team Tier A (Forge / Customer-Zero) — MARKETING TEMPLATE

> **Slot:** `LH-FORGE` · **Tier:** Team · **Customer-zero per spec contract §5.1**
> **Pairing template (spec canonical):** `specs/_lighthouse/case-study-templates/team-tier-1-forge.md`
> **This file** is the **marketing-ready instantiation**: prefilled where it can be filled objectively from `corelink-lighthouse-tracker` D1 tables, placeholders elsewhere.
> **Usage:** instantiate by copying to `marketing/case-studies/2026-MM-DD-forge.md`, replace `{{...}}` placeholders, run review cycle, publish.

---

## Auto-prefill instructions (for DevRel)

Fill the bracketed `{{db:...}}` markers before the customer interview, reading slot `LH-FORGE` out of the `lighthouse_customers` and `lighthouse_sla_samples` D1 tables (migration `0042_lighthouse_customers.sql`). There is **no export-and-prefill tool yet** — this is a manual pass today. Manual placeholders `{{interview:...}}` and `{{quote:...}}` require the interview transcript.

---

## Hero block

**Customer:** Forge (HuGR internal engineering organization — Customer-Zero)
**Tier:** Team
**Slot:** `LH-FORGE`
**Region:** `{{db:region}}`
**Observation window:** `{{db:observation_started_at}}` → `{{db:observation_completed_at}}` (30 days fixed)
**Attestation signed:** `{{db:attested_at}}`
**Case study published:** `{{db:case_study_signed_at}}`

---

## TL;DR (≤ 60 words, replaceable)

> Forge runs CoreLink as its primary content-addressable cache for Bazel-based CI across `{{interview:engineer_count}}` engineers and `{{db:monthly_cache_ops}}` ops/month. Over a 30-day attested observation window, p99 cache GET latency held at `{{db:p99_get_latency_ms}}`ms against a 300ms SLO; cache hit ratio reached `{{db:cache_hit_ratio_pct}}%`. Customer-zero validation gated GA Evidence Gate §1.6.

---

## 1. Problem statement (≤ 200 words)

Pre-CoreLink, Forge's build infrastructure suffered three concrete pains:

1. **Bazel remote cache fragility** — the standalone `bazel-remote-cache` instance had `{{interview:bazel_remote_uptime_pct}}%` availability over the prior 90 days, with `{{interview:n_outages}}` outages each lasting `{{interview:avg_outage_min}}` minutes on average.
2. **Cold builds dominating CI time** — `{{interview:cold_build_ratio_pct}}%` of CI runs hit a cold cache, costing `{{interview:avg_cold_build_min}}` extra minutes per run.
3. **No audit trail on cache contents** — when a poisoned cache entry was suspected (which happened `{{interview:n_poison_events}}` times in the year prior), there was no signed audit chain to inspect.

Forge needed a content-addressable cache with: SLA-backed availability, sub-300ms p99 GET latency, blake3-verified content addressing, and a queryable signed audit chain.

---

## 2. Before-state baseline (objective)

| Metric | Pre-CoreLink (90d) | Source |
|---|---|---|
| Cache backend uptime | `{{interview:bazel_remote_uptime_pct}}%` | Forge internal monitoring |
| Cache hit ratio | `{{interview:pre_cache_hit_ratio_pct}}%` | Bazel BES export |
| Avg CI run wall time | `{{interview:pre_avg_ci_min}}` min | GitHub Actions reporting |
| p99 cache GET latency | `{{interview:pre_p99_get_ms}}`ms | bazel-remote-cache `/metrics` |
| Audit trail on cache contents | none | n/a |
| Poison events tolerated | `{{interview:n_poison_events}}` (no detection mechanism) | Postmortems |

---

## 3. After-state — 30d attested observation

| SLO | Target | Measured (30d window) | Source | Status |
|---|---|---|---|---|
| `cache-get-p99-latency` | ≤ 300 ms | `{{db:p99_get_latency_ms}}` ms | `lighthouse_sla_samples` | `{{db:slo_status_get_latency}}` |
| `cache-availability` | ≥ 99.9% | `{{db:availability_pct}}%` | `lighthouse_sla_samples` | `{{db:slo_status_availability}}` |
| `audit-append-latency-p99` | ≤ 500 ms | `{{db:p99_audit_ms}}` ms | `lighthouse_sla_samples` | `{{db:slo_status_audit}}` |

**Cache hit ratio (informational, not SLO):** `{{db:cache_hit_ratio_pct}}%`
**Total operations during observation:** `{{db:total_cache_ops}}`
**Audit-chain entries written:** `{{db:total_audit_entries}}`
**SLO violations during window:** `{{db:slo_violation_total}}` (must equal 0 for `Attested`)

---

## 4. Integration story (≤ 500 words, narrative)

`{{interview:integration_story}}`

Suggested skeleton:
> Forge's engineering team migrated their bazel-remote-cache to CoreLink over `{{interview:integration_duration_days}}` days. The migration ran in mirror mode for the first `{{interview:mirror_days}}` days — CoreLink received all cache writes while Bazel reads still served from the legacy backend — before flipping primary cache to CoreLink on day `{{interview:flip_day}}`. No CI outages were attributed to the migration. The team's primary CI pipeline runs `{{interview:n_jobs}}` Bazel targets across `{{interview:n_workspaces}}` workspaces.

Include 1–2 specific technical decisions (e.g. region pinning rationale, retention policy choice, CAS-vs-AC routing).

---

## 5. Quotes (require sign-off before use)

> "`{{quote:engineering_lead_quote}}`"
> — `{{interview:engineering_lead_name_or_role}}`, Forge

> "`{{quote:sre_quote}}`"
> — `{{interview:sre_name_or_role}}`, Forge

Quote-approval procedure: every quote individually approved in the interview review round. Customer has unilateral right of refusal.

---

## 6. What we'd do differently (≤ 150 words)

`{{interview:learnings}}`

Suggested skeleton:
> The mirror-mode dual-write phase ran `{{interview:mirror_days}}` days, longer than the 5 days originally planned. In retrospect, `{{interview:learning_1}}`. The CoreLink team's decision to `{{interview:learning_2}}` allowed us to avoid `{{interview:learning_3}}`.

---

## 7. Attestation reference

This case study summarizes a customer engagement governed by the signed SLA attestation at:

`specs/_audits/{{db:attestation_doc_path}}`

The attestation form is countersigned by Forge engineering leadership and CoreLink (Customer Success, Engineering S-20 lead, Legal Counsel). It contains the full SLO measurement table, incident log, and any open issues at the close of the observation window.

---

## 8. Publication metadata

- **Audience:** internal-first (HuGR org self-attestation); excerpts re-usable in external marketing with caveat that Forge is the CoreLink customer-zero.
- **Confidentiality:** none (Forge is part of HuGR org; no NDA required between us).
- **Sanitization:** none required — all metrics publishable as-is.
- **Reference-call commitment:** Forge engineering leads available for up to 2 reference calls per quarter for 12 months post-publication.

---

## Template integrity check (DevRel)

Before publication confirm:

- [ ] All `{{db:...}}` placeholders replaced from CLI export.
- [ ] All `{{interview:...}}` placeholders filled from interview transcript.
- [ ] All `{{quote:...}}` placeholders have explicit per-quote sign-off in writing.
- [ ] Attestation referenced in §7 exists at the cited path and is `audit_status: ACTIVE`.
- [ ] State machine row for `LH-FORGE` is in `CaseStudySigned`.

---

**Fim TEAM-A-FORGE template.**
