---
version: "1.0.0"
last_updated: "2026-05-13"
notification_required: false
sub_processors:
  - id: "cloudflare"
    name: "Cloudflare, Inc."
    role: "Infrastructure provider (Workers, R2, KV, DO, D1, Pages, Email)"
    data_categories_processed:
      - "account_metadata"
      - "blob_content"
      - "audit_logs"
      - "telemetry"
    region: "Multi-region (tenant-pinned per tenant.primary_region)"
    certifications:
      - "SOC 2 Type II"
      - "ISO 27001"
      - "ISO 27018"
      - "PCI-DSS Level 1"
      - "HIPAA-compliant infra"
    dpa_url: "https://www.cloudflare.com/cloudflare-customer-dpa/"
    primary_jurisdiction: "United States (EU offices)"
    contract_signed_at: "2026-04-23"
    legal_review_evidence: "docs/compliance/vendor-reviews/cloudflare-dpa-review-2026-04.md"

  - id: "neon"
    name: "Neon Inc."
    role: "Postgres database hosting (control plane — dsr_tickets, account, tenant, billing)"
    data_categories_processed:
      - "billing_data"
      - "account_data"
    region: "US or EU (selectable per tenant)"
    certifications:
      - "SOC 2 Type II"
      - "ISO 27001"
      - "HIPAA-compliant"
    dpa_url: "https://neon.tech/dpa"
    primary_jurisdiction: "United States"
    contract_signed_at: "2026-04-23"
    legal_review_evidence: "docs/compliance/vendor-reviews/neon-dpa-review-2026-04.md"

  - id: "grafana-cloud"
    name: "Grafana Labs"
    role: "Observability platform (metrics, logs, dashboards)"
    data_categories_processed:
      - "telemetry"
      - "audit_logs"
      - "system_metrics"
    region: "EU or US (selectable)"
    certifications:
      - "SOC 2 Type II"
      - "ISO 27001"
    dpa_url: "https://grafana.com/legal/data-processing-addendum/"
    primary_jurisdiction: "United States"
    contract_signed_at: "2026-04-23"
    legal_review_evidence: "docs/compliance/vendor-reviews/grafana-dpa-review-2026-04.md"

  - id: "stripe"
    name: "Stripe, Inc."
    role: "Payment processing and subscription billing"
    data_categories_processed:
      - "billing_data"
      - "payment_information"
    region: "US and EU"
    certifications:
      - "PCI-DSS Level 1"
      - "SOC 2 Type II"
      - "ISO 27001"
    dpa_url: "https://stripe.com/legal/dpa"
    primary_jurisdiction: "United States"
    contract_signed_at: "2026-04-23"
    legal_review_evidence: "docs/compliance/vendor-reviews/stripe-dpa-review-2026-04.md"

  - id: "github"
    name: "GitHub, Inc."
    role: "Source code repository and CI/CD pipeline"
    data_categories_processed:
      - "source_code"
      - "ci_artifacts"
    region: "United States"
    certifications:
      - "SOC 2 Type II"
      - "ISO 27001"
    dpa_url: "https://docs.github.com/en/site-policy/privacy-policies/github-data-protection-agreement"
    primary_jurisdiction: "United States"
    contract_signed_at: "2026-04-23"
    legal_review_evidence: "docs/compliance/vendor-reviews/github-dpa-review-2026-04.md"

  - id: "sigstore"
    name: "The Linux Foundation (Sigstore)"
    role: "Software supply chain signing and transparency log (Rekor)"
    data_categories_processed:
      - "code_signing_metadata"
    region: "Global (transparency log)"
    certifications:
      - "Open Source (Linux Foundation)"
    dpa_url: "https://www.sigstore.dev/"
    primary_jurisdiction: "United States"
    contract_signed_at: "2026-04-23"
    legal_review_evidence: "docs/compliance/vendor-reviews/sigstore-review-2026-04.md"

  - id: "pagerduty"
    name: "PagerDuty, Inc."
    role: "Incident management and on-call alerting"
    data_categories_processed:
      - "operational_alerts"
      - "incident_metadata"
    region: "United States"
    certifications:
      - "SOC 2 Type II"
      - "ISO 27001"
    dpa_url: "https://www.pagerduty.com/privacy-policy/"
    primary_jurisdiction: "United States"
    contract_signed_at: "2026-04-23"
    legal_review_evidence: "docs/compliance/vendor-reviews/pagerduty-dpa-review-2026-04.md"
---

# CoreLink — Sub-Processors Register

CoreLink processa dados pessoais via os seguintes sub-processadores conforme
**GDPR Art. 28.2** + **LGPD Art. 39** + **CTRL-PRIV-021**.

Lista completa abaixo. Versão e data de última atualização no frontmatter YAML acima.

## Right to Object

Per **GDPR Art. 28.2.b** + **LGPD Art. 39**, customers podem objetar a mudanças via:

- **Email**: privacy@hugr.dev
- **API**: `POST /v1/privacy/sub-processor-objection`
- **DPA escalation**: Privacy Officer + Legal review ≤ 14 dias úteis → accept-or-terminate decision.

Rate limit: 5 objections/day/subject (anti-DoS; S-08 inheritance).

## Sub-Processors

| ID | Nome | Função | Região | Certificações | DPA |
|---|---|---|---|---|---|
| cloudflare | Cloudflare, Inc. | Infrastructure (Workers, R2, KV, DO, D1, Pages, Email) | Multi-region (tenant-pinned) | SOC 2 Type II, ISO 27001, ISO 27018, PCI-DSS Level 1 | [DPA](https://www.cloudflare.com/cloudflare-customer-dpa/) |
| neon | Neon Inc. | Postgres database hosting | US or EU (tenant-selectable) | SOC 2 Type II, ISO 27001 | [DPA](https://neon.tech/dpa) |
| grafana-cloud | Grafana Labs | Observability (metrics, logs, dashboards) | EU or US (selectable) | SOC 2 Type II, ISO 27001 | [DPA](https://grafana.com/legal/data-processing-addendum/) |
| stripe | Stripe, Inc. | Payment processing and billing | US and EU | PCI-DSS Level 1, SOC 2 Type II, ISO 27001 | [DPA](https://stripe.com/legal/dpa) |
| github | GitHub, Inc. | Source code repository and CI/CD | United States | SOC 2 Type II, ISO 27001 | [DPA](https://docs.github.com/en/site-policy/privacy-policies/github-data-protection-agreement) |
| sigstore | Linux Foundation (Sigstore) | Supply chain signing + Rekor transparency log | Global | Open Source (Linux Foundation) | [Site](https://www.sigstore.dev/) |
| pagerduty | PagerDuty, Inc. | Incident management + on-call alerting | United States | SOC 2 Type II, ISO 27001 | [Privacy Policy](https://www.pagerduty.com/privacy-policy/) |

## Notification Policy

**All 5 canonical plans** (free/solo/team/business/enterprise) receive mandatory
sub-processor change notifications. This notification is based on **legal_obligation**
basis (privacy_model.md §5.6.1) — NOT opt-out-able via consent_revoke.

This corrects any prior tier-gated approach per **ADR-S11-008 v2** (Lote 10.11.0-bis):
GDPR Art. 28.2 + LGPD Art. 39 establish sub-processor transparency as a universal
right, NOT a premium feature.

## Change Process

1. Privacy Officer drafts change in a PR to this file.
2. Legal review per DPA + GDPR Art. 28.2 implications.
3. PR merge triggers CD pipeline:
   - Auto-generates `/privacy/sub-processors` Cloudflare Pages page.
   - Emits `dev.hugr.corelink.sub_processor.{published|changed}.v1` CloudEvent.
   - Seeds `sub_processor_broadcast_log` D1 table.
4. 30-day advance notice email broadcast to all subscribed customers (DKIM signed).
5. Customer may object via `POST /v1/privacy/sub-processor-objection`.
6. Privacy Officer + Legal review ≤ 14 days → accept-or-terminate decision.

**Note**: 30d countdown = calendar days (not business days), per EDPB 7/2020 §125 industry standard.
