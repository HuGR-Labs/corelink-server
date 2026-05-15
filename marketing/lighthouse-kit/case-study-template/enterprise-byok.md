---
id: "LIGHTHOUSE-KIT-CASE-STUDY-ENTERPRISE-BYOK"
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
tags: ["lighthouse", "marketing", "case-study", "template", "enterprise", "byok", "schrems-ii", "dpa", "r5-2"]
---

# Case Study — Enterprise BYOK (ICP) — MARKETING TEMPLATE

> **Slot:** `LH-ENT-BYOK-01` · **Tier:** Enterprise BYOK
> **Pairing template (spec canonical):** `specs/_lighthouse/case-study-templates/enterprise-byok.md`
> **This file** is the **marketing-ready instantiation**: dual-version (NDA-internal + public-sanitized) ready to ship.
> **Audience:** NDA-internal version for prospect sales conversations; public-sanitized version for marketing site.
> **CRITICAL:** all identifying details (customer name, region, vertical specifics, KMS provider) require customer sign-off + Legal review before publication in either version.

---

## Two-version workflow

This template produces TWO published outputs:

1. **NDA-internal version** — used in sales conversations under NDA. May name the customer with their sign-off. Distributed as PDF; not on public site.
2. **Public-sanitized version** — published on marketing site. Customer name redacted; vertical described generically; metrics rounded or rendered as ranges; KMS provider family disclosed at most.

DevRel maintains both versions side-by-side. Customer countersigns BOTH versions independently.

---

## Auto-prefill instructions (for DevRel)

```
corelink lighthouse export-case-study --slot LH-ENT-BYOK-01 --format markdown \
  --sanitized=false > /tmp/ent-internal-prefill.md
corelink lighthouse export-case-study --slot LH-ENT-BYOK-01 --format markdown \
  --sanitized=true  > /tmp/ent-public-prefill.md
```

The `--sanitized=true` flag rounds metrics, redacts region to coarse geography ("EU"), redacts KMS to family ("cloud KMS"), and omits any field marked `sensitive=true` in the lighthouse record.

---

## Hero block

| Field | NDA-internal value | Public-sanitized value |
|---|---|---|
| Customer | `{{interview:customer_name}}` | "A {{interview:vertical_generic}} enterprise customer" |
| Tier | Enterprise BYOK | Enterprise BYOK |
| Slot | `LH-ENT-BYOK-01` | (omitted) |
| Region | `{{db:region}}` | `{{interview:region_coarse}}` |
| KMS provider | `{{db:byok_provider}}` | `{{interview:byok_family}}` (e.g. "cloud KMS") |
| Observation window | `{{db:observation_started_at}}` → `{{db:observation_completed_at}}` | "30-day attested window in {{interview:quarter}}" |
| Attestation signed | `{{db:attested_at}}` | "Attestation signed at end of window" |

---

## 1. Problem statement (≤ 200 words)

### NDA-internal version
`{{interview:problem_statement_internal}}`

Suggested skeleton:
> `{{interview:customer_name}}`, a `{{interview:vertical_specific}}` operating across `{{db:region}}`, required a build/CAS infrastructure provider that could honor:
> 1. **Crypto-sovereignty via BYOK** — customer-managed keys in `{{db:byok_provider}}`, no provider-held copies, kill-switch operational ≤ 5 min.
> 2. **Schrems II TIA-backed DPA** — addressing `{{interview:regulatory_regime}}` (e.g. EBA + GDPR Article 28 + FedRAMP-Moderate ambitions).
> 3. **Sub-300ms p99 cache GET latency** at `{{interview:scale_specific}}` (e.g. "X TB stored, Y operations/day").
> 4. **Verifiable erasure attestations** — Ed25519-signed audit-chain entries acceptable to internal compliance.
>
> Their incumbent stack `{{interview:incumbent_generic}}` could not satisfy `{{interview:gap_specific}}`.

### Public-sanitized version (≤ 200 words)
> A `{{interview:vertical_generic}}` enterprise customer in `{{interview:region_coarse}}` required a CAS / build-cache provider meeting four hard constraints:
> 1. Customer-managed encryption (BYOK).
> 2. Schrems II-aligned data processing terms.
> 3. Sub-300ms p99 GET latency at multi-TB scale.
> 4. Signed audit-chain proofs of erasure.
>
> Their incumbent stack could not satisfy the BYOK requirement under the customer's compliance constraints.

---

## 2. Before-state requirements (sanitized table)

| Requirement | Pre-CoreLink state (sanitized) |
|---|---|
| **Crypto-sovereignty (BYOK)** | Customer-managed `{{interview:byok_family}}` in customer account |
| **Data residency** | `{{interview:residency_generic}}` (e.g. "EU-only, no US transit") |
| **DPA + Schrems II TIA** | Required; in negotiation with prior vendor |
| **Erasure attestations** | Ed25519-signed required for audit |
| **Build / CAS scale** | `{{interview:scale_generic}}` (e.g. "tens of TB stored, six-figure ops/day") |
| **SLA expected** | 99.9% availability + sub-300ms p99 GET |
| **Compliance regime** | `{{interview:compliance_regime_generic}}` |

---

## 3. After-state — 30d attested observation

| SLO | Target | Measured (30d) — NDA-internal | Public-sanitized rendering |
|---|---|---|---|
| `cache-get-p99-latency` | ≤ 300 ms | `{{db:p99_get_latency_ms}}` ms | "Comfortably under 300ms" |
| `cache-availability` | ≥ 99.9% | `{{db:availability_pct}}%` | "Met 99.9% target" |
| `byok-kill-switch-rtt` | ≤ 5 min | `{{db:byok_kill_switch_p99_sec}}` s | "Under 5 min target" |
| `byok-key-health` | true | `{{db:byok_key_health_ok}}` | "Healthy throughout" |
| `audit-append-latency-p99` | ≤ 500 ms | `{{db:p99_audit_ms}}` ms | "Met 500ms target" |

**SLO violations during window:** `{{db:slo_violation_total}}` (must equal 0 for `Attested`)
**BYOK chaos drills run during window:** 4 (weekly cadence per program); all kill-switch RTTs within target.

---

## 4. Integration story (≤ 500 words)

### NDA-internal version
`{{interview:integration_story_internal}}`

Should cover:
- Schrems II TIA process timeline (typically the longest-duration item, D+1..D+25).
- BYOK provider integration specifics: which `{{db:byok_provider}}` API surface used, IAM policy boundary, key rotation procedure agreed.
- Pre-production validation: kill-switch drill walkthrough, key-revocation test, audit-chain entry verification.
- Cut-over: when primary cache flipped, what monitoring during cut-over.

### Public-sanitized version (≤ 300 words)
`{{interview:integration_story_public}}`

Same arc with: no specific provider, no specific timeline numbers, no quotable mention of incumbent vendor.

---

## 5. Quotes (require sign-off; INTERNAL version may be more specific)

### NDA-internal quotes

> "`{{quote:ciso_quote_internal}}`"
> — `{{interview:ciso_name_or_title}}`, `{{interview:customer_name}}`

> "`{{quote:platform_lead_quote_internal}}`"
> — `{{interview:platform_lead_name_or_title}}`, `{{interview:customer_name}}`

### Public-sanitized quotes

> "`{{quote:ciso_quote_public}}`"
> — Head of Information Security, `{{interview:vertical_generic}}` enterprise

> "`{{quote:platform_lead_quote_public}}`"
> — Platform Engineering Lead, `{{interview:vertical_generic}}` enterprise

Quote-approval procedure: each quote separately approved in writing for the version it appears in. Customer has unilateral right of refusal on any specific phrasing.

---

## 6. BYOK-specific evidence (NDA-internal only)

A dedicated section that does not appear in the public version. Captures the BYOK-specific posture:

- **KMS provider:** `{{db:byok_provider}}`
- **Key location:** `{{interview:key_location_specific}}` (account/project ARN/URI)
- **Key rotation:** `{{interview:key_rotation_cadence}}`
- **Kill-switch drill records:** 4 drills during window; RTTs: `{{db:byok_drill_rtt_list}}`; all under 5 min target.
- **DSR test:** simulated erasure DSR run on day `{{interview:dsr_test_day}}`; erasure complete + signed attestation emitted within `{{interview:dsr_completion_min}}` minutes.
- **Audit-chain proof:** Ed25519 signatures verified against published trust anchor; chain integrity verified end-to-end.

This section is the load-bearing evidence in enterprise sales conversations.

---

## 7. What didn't work — honest section (both versions)

`{{interview:friction_log}}`

Suggested skeleton (NDA-internal):
> During the 30-day window, `{{interview:customer_name}}` flagged `{{interview:n_friction_items}}` concerns:
> - `{{interview:friction_item_1}}` — `{{interview:friction_resolution_1}}`
> - `{{interview:friction_item_2}}` — `{{interview:friction_resolution_2}}`

Public-sanitized version uses generic phrasing for each item without leaking customer specifics.

Including this section is a hard requirement for both versions. No published Enterprise case study omits it.

---

## 8. Attestation reference

This case study summarizes a customer engagement governed by the signed SLA attestation at:

`specs/_audits/{{db:attestation_doc_path}}`

(NDA-internal version cites the specific filename; public-sanitized version cites only that an attestation exists, with `audit_status: ACTIVE`.)

---

## 9. Publication metadata

- **NDA-internal audience:** prospective enterprise buyers under NDA. PDF distribution; tracked recipient log.
- **Public audience:** marketing site, fully sanitized version.
- **Confidentiality:** NDA-protected for internal version; sanitized version cleared for public after customer + Legal review.
- **Reference-call commitment:** `{{interview:customer_name}}` available for up to 2 reference calls per quarter for 12 months. Reference calls run under NDA between prospect and customer.

---

## Template integrity check (DevRel + Legal)

Before EITHER version ships:

- [ ] All `{{db:...}}` placeholders replaced from sanitized + unsanitized CLI exports.
- [ ] All `{{interview:...}}` placeholders filled from interview transcript.
- [ ] All `{{quote:...}}` placeholders have explicit per-quote sign-off in writing — separately for internal and public.
- [ ] §6 BYOK evidence section: present in NDA-internal, ABSENT from public-sanitized.
- [ ] §7 "what didn't work" section is non-empty in BOTH versions.
- [ ] Legal review completed on the public-sanitized version.
- [ ] Customer countersigns the public-sanitized version with explicit confirmation that no identifying details remain.
- [ ] State machine row for `LH-ENT-BYOK-01` is in `CaseStudySigned`.

---

**Fim ENTERPRISE-BYOK template.**
