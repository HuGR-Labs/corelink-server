---
id: "RB-CONSENT-TAMPERING"
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
tags: ["runbook", "rb", "consent", "tampering", "fm-452", "s11", "stub", "planned"]
---

# RB-CONSENT-TAMPERING — Consent Record Tampering Detected

> **Status: PLANNED stub (Lote 10.11.0-bis-prime cycle 14)** — full SOP a ser drafted pré-PRR HIGH_RISK por Privacy Officer + SecLead.
> **FM:** [FM-452](../../03_architecture/failure_modes.md) (consent ledger fork; race condition cross-region) | **INV:** INV-CONSENT-PROOF-VERIFIABLE CRITICAL + INV-AUDIT-APPEND-ONLY CRITICAL | **CTRL:** CTRL-PRIV-CONSENT-003 | **SLA:** detect ≤ 1h, mitigate ≤ 6h, customer notification ≤ 72h se confirmed PII compromise

## Pré-condições

- S-11 consent ledger live (Neon canonical pós Lote 10.11.0-bis).
- HMAC-SHA256 per-record signature via tenant-scoped HKDF info=`corelink/v1/consent-hmac`.
- Audit chain verification (PAT-AUDIT-VERIFY-001) daily.

## Detecção

### Sinais primários

- HMAC signature mismatch em consent_ledger ou consent_revocation row durante verify.
- `notice_text_hash` field não bate com privacy_model.md notice version corresponding.
- Append-only chain hash break (Lamport clock anomaly cross-region).
- Métrica `corelink_consent_signature_verify_fail_total` > 0.

### Detection queries

```sql
-- Neon: consent_ledger HMAC verify (replay HMAC com tenant-scoped key)
SELECT consent_id, subject_id, purpose, hmac_actual, hmac_expected
FROM consent_ledger
WHERE hmac_actual != hmac_expected
  AND tenant_id = $1
  AND created_at > NOW() - INTERVAL '24 hours';
```

## Step 1: Triage (≤ 1h)

1. Verify HMAC mismatch is real (re-compute HMAC vs stored value via SecLead-controlled key).
2. Determine scope: single record, single tenant, single purpose, or cross-tenant.
3. Page SecLead + Privacy Officer + Compliance via PagerDuty `corelink-breach-sev1` se confirmed tampering.

## Step 2: Mitigation (≤ 6h)

1. **Quarantine affected records**: mark `consent_status='quarantined'` + stop processing baseado em them.
2. **Investigate**: forensic analysis — was tampering external (compromised key) or internal (rogue admin)? Cross-reference audit log + access patterns.
3. **Restore from canonical source**: re-derive consent state from notice_text_hash + audit events (CTRL-PRIV-CONSENT-003 immutability).

## Step 3: Customer notification (≤ 72h se confirmed)

- Trigger RB-BREACH-NOTIF se PII compromise confirmado (LGPD Art. 48 + GDPR Art. 33).
- Customer-facing email per WI-S11-005 sub-processor broadcast template (translated 3 locales).

## Post-incident

- Post-mortem mandatório CRITICAL.
- Root cause analysis: was Lamport clock cross-region race, HMAC key compromise, or admin privilege abuse?
- Update PAT-AUDIT-VERIFY-001 frequency se gap identified.
- Trigger KEY rotation se HMAC key suspected compromised (RB-KEY-COMPROMISE).

## References

- `failure_modes.md` FM-452 (consent ledger fork).
- `invariant_registry.md` INV-CONSENT-PROOF-VERIFIABLE CRITICAL §3.12 L168.
- `privacy_model.md` §5.6 CTRL-PRIV-CONSENT-001..006.
- `WI-S11-003` consent ledger Neon symmetric schema.
- LGPD Art. 8 + GDPR Art. 7 (consent integrity).
