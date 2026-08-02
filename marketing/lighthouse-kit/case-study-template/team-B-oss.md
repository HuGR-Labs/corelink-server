---
id: "LIGHTHOUSE-KIT-CASE-STUDY-TEAM-B-OSS"
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
tags: ["lighthouse", "marketing", "case-study", "template", "team-tier", "oss", "bazel", "buck2", "r5-2"]
---

# Case Study — Team Tier B (External OSS Bazel/Buck2) — MARKETING TEMPLATE

> **Slot:** `LH-OSS-01` · **Tier:** Team
> **Pairing template (spec canonical):** `specs/_lighthouse/case-study-templates/team-tier-2-oss.md`
> **This file** is the **marketing-ready instantiation**: prefilled from `corelink-lighthouse-tracker` data where possible.
> **Audience:** public marketing site; OSS community visibility. Customer is named with sign-off.

---

## Auto-prefill instructions (for DevRel)

The `{{db:...}}` markers are filled by hand from slot `LH-OSS-01` in the `lighthouse_customers` / `lighthouse_sla_samples` D1 tables (migration `0042_lighthouse_customers.sql`) — there is no export-and-prefill tool yet. `{{interview:...}}` and `{{quote:...}}` are filled from the case-study interview transcript.

---

## Hero block

**Customer:** `{{interview:customer_name}}` (`{{interview:customer_oss_project}}`, an OSS `{{interview:customer_oss_domain}}` project)
**Tier:** Team
**Slot:** `LH-OSS-01`
**Region:** `{{db:region}}`
**Build tool:** `{{interview:build_tool}}` (Bazel `{{interview:bazel_version}}` / Buck2 / Pants — pick one)
**Observation window:** `{{db:observation_started_at}}` → `{{db:observation_completed_at}}`
**Attestation signed:** `{{db:attested_at}}`

---

## TL;DR (≤ 60 words, replaceable)

> `{{interview:customer_name}}` — `{{interview:customer_oss_project_desc}}` — migrated its CI build cache to CoreLink during a 30-day attested observation window. p99 cache GET latency held at `{{db:p99_get_latency_ms}}`ms; cache hit ratio reached `{{db:cache_hit_ratio_pct}}%`, cutting CI wall time on incremental builds by `{{interview:wall_time_savings_pct}}%`. Migration completed in `{{interview:integration_duration_days}}` calendar days with `{{interview:engineer_hours_total}}` engineer-hours of effort.

---

## 1. Why `{{interview:customer_name}}` chose CoreLink

`{{interview:why_corelink}}`

Suggested skeleton:
> `{{interview:customer_name}}` maintains `{{interview:customer_oss_project}}`, used by `{{interview:user_count}}` downstream projects. Their CI infrastructure runs across `{{interview:n_runners}}` GitHub Actions runners with `{{interview:monthly_ci_min}}` CI-minutes per month. Their prior cache backend — `{{interview:prior_cache_backend}}` — had `{{interview:prior_pain_point}}`. They evaluated `{{interview:n_alternatives_eval}}` alternatives before selecting CoreLink for `{{interview:selection_reason}}`.

---

## 2. The numbers — 30d attested observation

| Metric | Result | Detail |
|---|---|---|
| p99 cache GET latency | `{{db:p99_get_latency_ms}}` ms | SLO target: ≤ 300 ms |
| Cache availability | `{{db:availability_pct}}%` | SLO target: ≥ 99.9% |
| Cache hit ratio (informational) | `{{db:cache_hit_ratio_pct}}%` | Not an SLO; informational |
| p99 audit append latency | `{{db:p99_audit_ms}}` ms | SLO target: ≤ 500 ms |
| Total cache operations | `{{db:total_cache_ops}}` | Over 30d window |
| SLO violations | `{{db:slo_violation_total}}` | Must equal 0 for `Attested` |
| Build wall-time savings | `{{interview:wall_time_savings_pct}}%` | Self-reported by customer |
| CI cost savings | `{{interview:ci_cost_savings_usd}}` USD/mo | Self-reported by customer |

---

## 3. The migration story (≤ 500 words)

`{{interview:migration_narrative}}`

Suggested narrative arc:

1. **Discovery + scoping** (D+0..D+5): how they first heard about CoreLink; what their scoping call surfaced; the migration plan they chose (mirror mode vs cut-over).
2. **Mirror mode + onboarding** (D+5..D+10): CI integration template chosen, first PAT, first CAS PUT, first cache hit.
3. **Cut-over** (D+10..D+15): when they flipped primary, what they monitored, any rollback drills run.
4. **Observation** (D+10..D+40): typical day-in-the-life; any incidents and how they got handled; the weekly check-in cadence experience.
5. **Attestation** (D+40..D+45): their SRE's perspective on countersigning; what they cross-checked.

Include 1 verbatim technical decision quote (e.g. choice of region, retention policy, CAS-vs-AC behavior).

---

## 4. Quotes (require sign-off before use)

> "`{{quote:tech_lead_quote_1}}`"
> — `{{interview:tech_lead_name_and_title}}`

> "`{{quote:tech_lead_quote_2}}`"
> — same person, on a specific moment in the migration

> "`{{quote:sre_quote}}`"
> — `{{interview:sre_name_and_title}}`

Quote-approval procedure: every quote individually approved in writing during the interview review round. The customer has unilateral right of refusal on any specific phrasing.

---

## 5. What didn't work — honest section

`{{interview:friction_log}}`

Suggested skeleton:
> The integration wasn't friction-free. The team flagged `{{interview:n_friction_items}}` ergonomic concerns during weekly check-ins, of which `{{interview:n_friction_resolved}}` were resolved during the observation window. Specifically:
>
> - `{{interview:friction_item_1}}` — `{{interview:friction_resolution_1}}`
> - `{{interview:friction_item_2}}` — `{{interview:friction_resolution_2}}`

Including this section is a hard requirement. No published case study omits it.

---

## 6. Attestation reference

This case study summarizes a customer engagement governed by the signed SLA attestation at:

`specs/_audits/{{db:attestation_doc_path}}`

Countersigned by `{{interview:customer_name}}` SRE/ops + procurement and by CoreLink Customer Success, Engineering S-20 lead, and Legal Counsel.

---

## 7. About `{{interview:customer_name}}`

`{{interview:customer_about_paragraph}}` (provided by customer; 50–100 words).

---

## 8. Publication metadata

- **Audience:** public marketing site (`corelink.humangr.com/case-studies/oss-{{db:slug}}`).
- **Confidentiality:** none; customer-approved public publication.
- **Sanitization:** customer reviews all numbers + quotes before publication.
- **Reference-call commitment:** `{{interview:customer_name}}` available for up to 2 reference calls per quarter for 12 months post-publication.

---

## Template integrity check (DevRel)

- [ ] All `{{db:...}}` placeholders replaced from CLI export.
- [ ] All `{{interview:...}}` placeholders filled from interview transcript.
- [ ] All `{{quote:...}}` placeholders have explicit per-quote sign-off in writing.
- [ ] §5 "what didn't work" section is non-empty.
- [ ] Customer name + project URL approved.
- [ ] Attestation referenced in §6 exists at the cited path and is `audit_status: ACTIVE`.
- [ ] State machine row for `LH-OSS-01` is in `CaseStudySigned`.

---

**Fim TEAM-B-OSS template.**
