---
id: "RB-PRIVACY-NOTICE-LATE-PUBLICATION"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-28"
updated: "2026-04-28"
owner: "Privacy Officer"
final_approver: "Privacy Officer"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "rb", "privacy-notice", "late-publication", "wi-s11-004", "s11", "stub", "planned"]
---

# RB-PRIVACY-NOTICE-LATE-PUBLICATION — Privacy Notice Material Change Not Published in Time

> **Status: PLANNED stub (Lote 10.11.0-bis-prime cycle 14)** — full SOP a ser drafted pré-PRR por Privacy Officer + Legal externo.
> **WI:** WI-S11-004 (privacy notice versioning + 3 locales) | **CTRL:** CTRL-PRIV-CONSENT-005 (notice versioning + retranslation enforcement) | **SLA:** publish ≤ 24h after material change identified; customer comms ≤ 30d grace pre-effect

## Pré-condições

- S-11 privacy notice live em `legal/privacy-notice/v<M.m>.md` 3 locales (pt-BR/en-US/es-MX).
- Semver convention: major bump = material change → force re-consent (30d grace + 60d cap).
- CD pipeline auto-publish em PR merge.

## Detecção

### Sinais primários

- Material change identified em sub-processor list, data category, or purpose enum WITHOUT corresponding notice version bump.
- Customer/regulator inquiry about specific notice version not matching implementation.
- CI hook detected PR mudando PII handling sem notice update + DPIA filled.
- Métrica `corelink_privacy_notice_publish_lag_hours` > 24.

## Step 1: Triage (≤ 4h)

1. Confirm material change scope (Privacy Officer + Legal review).
2. Determine affected purposes + subject count (cross-link consent_ledger).
3. Decide: emergency notice publish OR rollback the underlying change.

## Step 2: Mitigation (≤ 24h)

- Force notice major version bump em git (`legal/privacy-notice/v<M+1>.0.md`) com 3 locales.
- Run native speaker + Legal local review (parallel for 3 locales).
- CD pipeline triggers WI-S11-003 stale_consent_check + 30d grace start.
- Customer notification batch (WI-S11-005 mjml infra).

## Step 3: Regulatory comms (case-by-case)

- ANPD/EDPB pre-notification se Privacy Officer judges material risk.
- Documented decision in audit chain (CTRL-PRIV-CONSENT-003).

## Post-incident

- Post-mortem: why was material change deployed without notice update? Root cause em CI hook gap, training gap, or process gap.
- Tighten CI hook scripts/validate_dpia.py + scripts/check_privacy_notice_version.py.

## References

- `WI-S11-004` privacy notice versioning + 3 locales + diff publication.
- `WI-S11-003` consent ledger stale_consent flow.
- `privacy_model.md` §5.6 CTRL-PRIV-CONSENT-005.
- GDPR Art. 13/14 + LGPD Art. 9.
- WP29 Guidelines on Transparency WP260rev01 endorsed by EDPB.
