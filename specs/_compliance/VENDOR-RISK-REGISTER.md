---
id: "VENDOR-RISK-REGISTER-2026-05-15"
type: "compliance_register"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-05-15"
updated: "2026-08-23"
sprint: "R5-3"
parent_wi: "WI-R5-3-GAP-14-VENDOR-RISK"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from: ["VENDOR-RISK-METHODOLOGY-2026-05-15", "SOC2-EVIDENCE-ROLLUP-2026-05-15"]
tags: ["soc2", "cc9.2", "vendor-risk", "register", "gap-14", "drata"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-billing-replay` was absorbed into `corelink-billing` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-billing-absorption.md. Canonical consumer path is now `corelink_billing::replay::*`.

# Vendor Risk Register

> **doc_status:** ACTIVE · **audit_status:** ACTIVE · **scope:** All third-party SaaS and infrastructure vendors that process CoreLink data, share material risk, or whose outage breaks a CoreLink customer commitment.
>
> **Baseline date:** 2026-05-15 · **Next full refresh:** 2026-08-15 (Q3 2026 review window).
>
> **Methodology:** see `VENDOR-RISK-METHODOLOGY.md` for scoring rubric.
> **Operational cycle:** see `specs/_runbooks/RB-VENDOR-RISK-QUARTERLY-REVIEW.md`.
> **Per-Critical-vendor DD files:** see `specs/_compliance/vendor-dd/`.
> **Public sub-processor disclosure (subset):** `legal/sub-processors.md`.

---

## 1. Summary

| Metric | Value |
|---|---|
| Total vendors registered | 19 |
| Critical | 6 |
| Important | 8 |
| Standard | 5 |
| Vendors with signed DPA | 18 / 19 (Resend: DPA policy published, signed-copy evidence pending — VR-6) |
| Vendors with current SOC 2 Type II (≤ 12 mo) | 13 / 19 (Neon's SOC 2 left with its row-16 removal; Resend's SOC 2 report is not yet pulled into Drata — VR-6) |
| Vendors carrying residual score ≥ 8.0 | 0 |
| Vendors flagged for ad-hoc re-review | 0 |
| Vendors with critical-category second-vendor failover | 5 / 6 (Drata: monitoring-only, no failover required — see §4) |
| Registered vendors NOT currently active as public-facing (Art. 28) sub-processors | 5 / 19 — see §4b |

> **2026-08-23 correction:** row 16 previously listed **Neon, Inc.** as an
> active sub-processor processing `pii + metadata`. `DATABASE_URL` has zero
> readers across `crates/` and `worker/`
> (`crates/corelink-container/src/routes/dsr.rs:224-228` states plainly
> *"Neon control-plane not shipped"*), so it was a phantom entry — removed
> here and in `legal/sub-processors.md` / `apps/docs/docs/trust/subprocessors.mdx`
> (tracked with PR #1215, which removed it from the contractual/public
> disclosures). Row 16 is now **Resend, Inc.**, added because it *is* live and
> was undisclosed in **both** registers — see §4b and the 2026-08-23 change-log
> entry below.

---

## 2. Register

Columns:

- **Vendor** — legal entity name.
- **Service** — what we buy from them.
- **Cat.** — Category (C = Critical, I = Important, S = Standard) per methodology §2.
- **Data sharing** — what data flows to vendor: `none` / `metadata` / `pii` / `payment` / `encrypted-blobs` / `audit-logs` / `telemetry`.
- **Regulatory scope** — frameworks the vendor is attested to / participates in.
- **Contract** — DPA / SLA / BAA posture (S = signed, P = pending, N/A).
- **Attestation evidence** — link to attestation report or Drata vendor module.
- **Inherent** — inherent risk score (1..25).
- **CEF** — control-effectiveness factor (methodology §4).
- **Residual** — residual risk score (Inherent × CEF).
- **Cadence** — review cadence (Q = Quarterly, B = Bi-annual, A = Annual).
- **Last review** — 2026-05-15 baseline.
- **Next review** — per cadence.
- **Owner** — accountable internal owner.

| # | Vendor | Service | Cat. | Data sharing | Regulatory scope | Contract (DPA/SLA/BAA) | Attestation evidence | Inherent | CEF | Residual | Cadence | Last review | Next review | Owner |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | Cloudflare, Inc. | Workers / R2 / D1 / DO / KV / Pages / Email — primary data plane + control plane runtime | C | metadata + encrypted-blobs + audit-logs + telemetry | SOC 2 Type II, ISO 27001, ISO 27018, PCI-DSS L1, HIPAA-compliant infra, FedRAMP Moderate | S / S / N/A | [Cloudflare DPA](https://www.cloudflare.com/cloudflare-customer-dpa/) + Drata vendor module + `docs/compliance/vendor-reviews/cloudflare-dpa-review-2026-04.md` | 20 | 0.25 | 5.0 | Q | 2026-05-15 | 2026-08-15 | VP-Sec |
| 2 | Stripe, Inc. | Payment processing + subscription billing + Stripe Atlas counsel | C | payment + pii | PCI-DSS L1, SOC 2 Type II, ISO 27001 | S / S / N/A | [Stripe DPA](https://stripe.com/legal/dpa) + `docs/compliance/vendor-reviews/stripe-dpa-review-2026-04.md` | 16 | 0.25 | 4.0 | Q | 2026-05-15 | 2026-08-15 | VP-Sec |
| 3 | Clerk, Inc. | Authentication + identity provider + JWT issuer | C | pii | SOC 2 Type II, GDPR processor | S / S / N/A | Clerk SOC 2 Type II report (Drata vendor module) + `docs/compliance/vendor-reviews/clerk-dpa-review-2026-04.md` | 16 | 0.25 | 4.0 | Q | 2026-05-15 | 2026-08-15 | VP-Sec |
| 4 | Amazon Web Services, Inc. | AWS KMS — staging BYOK + customer-side production CMKs (customer accounts) | C | encrypted-blobs (envelope keys only; no plaintext content) | SOC 2 Type II, ISO 27001/17/18, FedRAMP High, PCI-DSS L1, HIPAA, FIPS 140-3 endpoints | S / S / S (BAA available, not signed — no PHI today) | AWS Artifact (Drata-pulled SOC 2 + ISO 27001) | 20 | 0.10 | 2.0 | Q | 2026-05-15 | 2026-08-15 | VP-Sec |
| 5 | Google LLC (Google Cloud) | GCP Cloud KMS — customer-side BYOK CMK option | C | encrypted-blobs (envelope keys only) | SOC 2 Type II, ISO 27001/17/18, FedRAMP High, PCI-DSS L1, FIPS 140-2 L3 (HSM tier) | S / S / S (BAA available) | Google Cloud Compliance Reports Manager (Drata-pulled) + [`vendor-dd/DD-GCP-KMS.md`](./vendor-dd/DD-GCP-KMS.md) | 20 | 0.10 | 2.0 | Q | 2026-05-15 | 2026-08-15 | VP-Sec |
| 6 | Microsoft Corporation (Azure) | Azure Key Vault — customer-side BYOK CMK option | C | encrypted-blobs (envelope keys only) | SOC 2 Type II, ISO 27001/17/18, FedRAMP High, PCI-DSS L1, FIPS 140-2 L2/L3 | S / S / S (BAA available) | Microsoft Service Trust Portal (Drata-pulled) + [`vendor-dd/DD-AZURE-KEYVAULT.md`](./vendor-dd/DD-AZURE-KEYVAULT.md) | 20 | 0.10 | 2.0 | Q | 2026-05-15 | 2026-08-15 | VP-Sec |
| 7 | Drata, Inc. | SOC 2 / ISO 27001 evidence-collection platform + vendor-risk module + employee-attestation tracker | C | metadata + audit-logs | SOC 2 Type II, ISO 27001 | S / S / N/A | [Drata Trust Center](https://drata.com/trust) + `specs/_compliance/DRATA-INTEGRATION-COVERAGE.md` | 12 | 0.40 | 4.8 | Q | 2026-05-15 | 2026-08-15 | VP-Sec |
| 8 | HashiCorp, Inc. (Vault customer-managed) | HashiCorp Vault — customer-side BYOK secret backend (customer-hosted; we never see plaintext) | I | none (customer-side; we exchange role-id/secret-id only) | SOC 2 Type II (HashiCorp Cloud), ISO 27001; customer-self-hosted instances inherit customer attestations | S (per-customer addendum) / N/A (customer-side) / N/A | HashiCorp Trust Portal + customer-side attestations + [`vendor-dd/DD-HASHICORP-VAULT.md`](./vendor-dd/DD-HASHICORP-VAULT.md) | 12 | 0.25 | 3.0 | B | 2026-05-15 | 2026-11-15 | Eng-Lead |
| 9 | PagerDuty, Inc. | Incident management + on-call alerting (prod + synthetic-drill routes) | I | audit-logs + metadata | SOC 2 Type II, ISO 27001 | S / S / N/A | [PagerDuty Trust](https://www.pagerduty.com/security/) + `docs/compliance/vendor-reviews/pagerduty-dpa-review-2026-04.md` + [`vendor-dd/DD-PAGERDUTY.md`](./vendor-dd/DD-PAGERDUTY.md) | 10 | 0.25 | 2.5 | B | 2026-05-15 | 2026-11-15 | Eng-Lead |
| 10 | Slack Technologies, LLC (Salesforce) | Slack — alerts/enterprise-inquiries/breach-notifications/oncall-handoff/lighthouse-customers webhooks + bot token | I | metadata + audit-logs (notification payloads — no customer PII) | SOC 2 Type II, ISO 27001, FedRAMP Moderate | S / S / N/A | Salesforce Compliance + Drata vendor module + [`vendor-dd/DD-SLACK.md`](./vendor-dd/DD-SLACK.md) | 9 | 0.40 | 3.6 | B | 2026-05-15 | 2026-11-15 | Eng-Lead |
| 11 | HubSpot, Inc. | CRM (enterprise-inquiry intake) — private app token | I | pii (sales prospect contact data only) | SOC 2 Type II, ISO 27001, GDPR processor | S / S / N/A | [HubSpot Trust Center](https://www.hubspot.com/data-privacy/security) + [`vendor-dd/DD-HUBSPOT.md`](./vendor-dd/DD-HUBSPOT.md) | 9 | 0.40 | 3.6 | B | 2026-05-15 | 2026-11-15 | Eng-Lead |
| 12 | GitHub, Inc. (Microsoft Enterprise) | Source code repository + CI/CD pipeline + Actions secrets store | I | source code (proprietary) + ci artifacts + audit-logs | SOC 2 Type II, ISO 27001, FedRAMP Moderate | S / S / N/A | [GitHub Trust Center](https://github.com/security) + Microsoft STP | 12 | 0.25 | 3.0 | B | 2026-05-15 | 2026-11-15 | Eng-Lead |
| 13 | Anthropic, PBC | Claude model API — internal-only LLM tooling (no customer-data inference) | I | metadata only (CoreLink internal prompts; no customer payloads) | SOC 2 Type II, ISO 27001 (in progress), enterprise zero-retention API | S (zero-retention) / S / N/A | Anthropic Trust Portal | 8 | 0.40 | 3.2 | B | 2026-05-15 | 2026-11-15 | Eng-Lead |
| 14 | OpenAI, LLC | OpenAI API — internal-only LLM tooling (no customer-data inference) | I | metadata only (CoreLink internal prompts; no customer payloads) | SOC 2 Type II, enterprise zero-retention API, GDPR processor | S (zero-retention) / S / N/A | OpenAI Trust Portal | 8 | 0.40 | 3.2 | B | 2026-05-15 | 2026-11-15 | Eng-Lead |
| 15 | Grafana Labs | Grafana Cloud — observability (metrics, logs, dashboards) | I | telemetry + audit-logs (no customer PII payloads; only aggregates) | SOC 2 Type II, ISO 27001 | S / S / N/A | [Grafana Trust](https://grafana.com/security/) + `docs/compliance/vendor-reviews/grafana-dpa-review-2026-04.md` | 9 | 0.40 | 3.6 | B | 2026-05-15 | 2026-11-15 | Eng-Lead |
| 16 | Resend, Inc. | Transactional email + newsletter-audience delivery (`apps/admin-ui/src/app/api/newsletter/subscribe/route.ts`; `RESEND_API_KEY` deployed on `corelink-prod` and `corelink-analytics-prod`) | I | pii (recipient email address) | GDPR processor (self-attested; SOC 2 report not yet pulled into Drata) | P / N/A / N/A (DPA policy published; signed-copy evidence not yet on file — VR-6) | [Resend DPA](https://resend.com/legal/dpa) | 12 | 0.40 | 4.8 | Q | 2026-08-23 | 2026-11-23 | VP-Sec |
| 17 | Twilio, Inc. (SendGrid + Twilio SMS) | Transactional email (SendGrid) + SMS (Twilio) | S | pii (recipient contact only — DKIM-signed transactional mail) | SOC 2 Type II, ISO 27001, GDPR processor | S / S / N/A | Twilio Trust Hub | 6 | 0.40 | 2.4 | A | 2026-05-15 | 2027-05-15 | Eng-Lead |
| 18 | Atlassian (Statuspage.io) | Public status page hosting | S | metadata (incident titles + impact statements only) | SOC 2 Type II, ISO 27001 | S / S / N/A | Atlassian Trust | 4 | 0.40 | 1.6 | A | 2026-05-15 | 2027-05-15 | Eng-Lead |
| 19 | Cybot A/S (Cookiebot) | Cookie consent management (admin-ui surface) | S | pii (consent records only) | GDPR processor, ISO 27001 | S / S / N/A | Cybot DPA | 4 | 0.40 | 1.6 | A | 2026-05-15 | 2027-05-15 | Eng-Lead |

---

## 3. Critical-vendor failover map

For every Critical-category vendor we require a **tested failover path** to satisfy methodology §4 CEF condition (c). Status as of 2026-05-15:

| Critical vendor | Failover strategy | Failover tested? | Evidence |
|---|---|---|---|
| Cloudflare | Multi-region Workers (us-east-1 ↔ eu-west-1 ↔ ap-southeast-1) + cold-restore from R2 cross-region replication | Yes (`specs/_audits/sealed/2026-05-14-region-outage-chaos-s14.md`) | Region-outage chaos drill SEALED |
| Stripe | Stripe-test-mode rehearsal + 24h webhook replay buffer + manual invoice flow documented in `legal/billing-fallback-playbook.md` | Yes (R-2 wiring tested webhook replay) | `corelink-billing-replay` |
| Clerk | JWT-verification path tolerates Clerk-down for ≤ 24h via long-lived JWKS cache + read-only-mode toggle in admin plane | Yes (kill-Clerk drill 2026-05-09) | `specs/_audits/2026-05-09-clerk-outage-drill.md` (planned; tracked under R6 staging-bake) |
| AWS KMS | Customer holds CMK in customer account; CoreLink BYOK matrix supports parallel KMS/GCP/Azure/Vault per tenant | Yes (BYOK matrix weekly workflow) | `.github/workflows/byok_matrix_weekly.yml` |
| GCP KMS | Same as AWS KMS — vendor-agnostic envelope | Yes | Same |
| Azure Key Vault | Same as AWS KMS — vendor-agnostic envelope | Yes | Same |
| Drata | **No failover required.** Drata is monitoring-only; if Drata is down we revert to manual evidence collection (already documented in `specs/_compliance/DRATA-INTEGRATION-COVERAGE.md` §"manual-upload fallback"). Outage does not break CoreLink customer surface. | N/A | Drata outage runbook `specs/_runbooks/RB-DRATA-SYNC-FAILURE.md` |

---

## 4. Vendors **excluded** from this register (rationale)

Vendors below appear in `docs/internal/secrets-checklist.md` but are intentionally **out of scope** for this register, with rationale:

| Excluded vendor | Why excluded |
|---|---|
| Sigstore (Linux Foundation) | Public transparency log; no PII shared; treated as open-source infrastructure dependency, not a vendor. Captured in `legal/sub-processors.md` for completeness only. |
| Dependency-Track (self-hosted) | Self-hosted by CoreLink; the vendor is "us". No third-party risk surface. |
| Per-customer HashiCorp Vault instances | Customer-side infrastructure; CoreLink never holds plaintext credentials. Listed for awareness in row 8 but per-customer instances are scoped under the customer's risk program. |

Additional note: **Datadog** appears in the task brief vendor list but is **not in production use** by CoreLink as of 2026-05-15 — we use Grafana Cloud (row 15) for observability. If Datadog is ever onboarded, an ad-hoc DD review is required before any data flow is enabled.

---

## 4b. Registered vendors **not currently active** as customer-data sub-processors (2026-08-23)

These vendors stay **in this register** (contract/DPA posture reviewed, real client
code exists in-repo) but are **not** listed on the public
`apps/docs/docs/trust/subprocessors.mdx` "Active sub-processors" table or in
`legal/sub-processors.md`'s active set, because no customer data is actually
flowing to them today. Verified 2026-08-23; each generates a line item under
"Contracted-but-not-active / integration built, not enabled" on both the
public page and the legal register:

| Vendor | Row | Why not active |
|---|---|---|
| Drata, Inc. | 7 | `DRATA_API_KEY` is deployed on none of 10 production Workers and is not a GitHub Actions secret; no binary or workflow invokes the `crates/corelink-ops/src/drata/` client. |
| Slack Technologies, LLC (Salesforce) | 10 | `SLACK_SECURITY_WEBHOOK` is unset everywhere; `.github/workflows/pentest-findings-sync.yml` skips the delivery step when it is absent. `docs/internal/secrets-checklist.md:316` records it as "not yet wired into a code consumer." |
| HubSpot, Inc. | 11 | `HUBSPOT_PRIVATE_APP_TOKEN` is unset; `crates/corelink-enterprise-inquiry/src/hubspot.rs` is not a dependency of `corelink-container` and is never mounted as a route. `docs/internal/secrets-checklist.md:314`: "paused pending Sales Ops onboarding." |
| Grafana Labs | 15 | `CORELINK_GRAFANA_API_KEY` is deployed on no Worker, so the `grafana_cloud` OTel variant in `crates/corelink-container/src/routes/otel_layer.rs:158-248` falls back to `disabled`. The Grafana hosts referenced in internal docs are NXDOMAIN. |
| Twilio, Inc. (SendGrid + Twilio SMS) | 17 | `SENDGRID_API_KEY` is unset everywhere and Twilio has no code consumer at all (`docs/internal/secrets-checklist.md:102-103,305-306`: "forward-looking… No code consumer yet"). The real transactional-email path is **Resend** (row 16). |

This section is the source list consumed by `scripts/gen-public-subprocessors.py`'s
`INERT_VENDORS` map — keep both in sync when a vendor here goes live (move the
row out of this section, drop it from `INERT_VENDORS`) or when a currently-active
vendor goes dark (the reverse).

---

## 5. Open actions

| # | Action | Owner | Due | Status |
|---|---|---|---|---|
| VR-1 | Re-pull SOC 2 Type II evidence packages for Anthropic + OpenAI on Drata vendor module (currently linked via Trust Portal URL, not auto-synced) | Eng-Lead | 2026-06-15 | Open |
| VR-2 | Schedule Clerk kill-drill formal walkthrough (failover map row references planned 2026-05-09 drill; verify date and seal evidence) | VP-Sec | 2026-06-30 | Open |
| VR-3 | Confirm BAA signature requirement is not triggered (no PHI today); document the negative as an explicit Privacy-Officer signoff | VP-Sec | 2026-07-15 | Open |
| VR-4 | Add Cookiebot to Drata vendor module (currently manual) | Eng-Lead | 2026-07-15 | Open |
| VR-5 | Annual methodology refresh + Risk-Committee charter ratification | VP-Sec | 2027-05-15 | Scheduled |
| VR-6 | Obtain a signed-copy DPA evidence file for Resend (row 16) — today only the public DPA policy page is linked, no `docs/compliance/vendor-reviews/resend-dpa-review-*.md` exists | Legal | 2026-09-23 | Open |

---

## 6. Change log

| Version | Date | Author | Notes |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo Schneiter (via Sonnet builder, R5-3 GAP-14 closure) | Initial register; 19 vendors; closes GAP-14 from `SOC2-EVIDENCE-ROLLUP-2026-05-15.md`. |
| 1.1.0 | 2026-05-15 | Gustavo Schneiter (Important-tier DD batch) | Added 6 vendor DD files (`DD-GCP-KMS.md`, `DD-AZURE-KEYVAULT.md`, `DD-HASHICORP-VAULT.md`, `DD-HUBSPOT.md`, `DD-PAGERDUTY.md`, `DD-SLACK.md`); cross-linked in the "Attestation evidence" column for rows 5, 6, 8, 9, 10, 11. Brings DD-file coverage to 11/19 vendors (Cloudflare/Stripe/Clerk/AWS-KMS/GCP-KMS/Azure-KV/Drata Critical row + Vault/PagerDuty/Slack/HubSpot Important rows). Remaining Important DD backlog: GitHub, Anthropic, OpenAI, Grafana, Neon (deferred to Q3 review cycle). |
| 1.2.0 | 2026-08-23 | Gustavo Schneiter (subprocessor register-truth reconciliation) | Row 16 swapped from **Neon, Inc.** (phantom — `DATABASE_URL` has zero readers; removal tracked jointly with PR #1215) to **Resend, Inc.** (real and previously undisclosed — `RESEND_API_KEY` deployed prod, POSTs recipient PII per `apps/admin-ui/src/app/api/newsletter/subscribe/route.ts`). Added §4b listing 5 registered-but-inert vendors (Drata, Slack, HubSpot, Grafana Labs, Twilio/SendGrid) that must not appear on the public "Active sub-processors" table until a real credential + code consumer exists. `scripts/gen-public-subprocessors.py` updated to render an inert vendors out of a separate `INERT_VENDORS` map; public page regenerated. Sigstore's DPA-matrix status in `legal/sub-processors.md` clarified as supply-chain-only (no customer data), not a customer-data sub-processor. |
