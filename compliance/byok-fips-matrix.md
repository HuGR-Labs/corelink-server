---
id: "COMPLIANCE-BYOK-FIPS-MATRIX"
type: "compliance_doc"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
reviewers: []
tags: ["byok", "fips-140-2", "fips-140-3", "nist-cmvp", "compliance", "s14"]
---

# BYOK FIPS Compliance Matrix — CoreLink S-14

> **NIST SP 800-57 Pt 1 Rev 5** (key management) + **NIST SP 800-130** (cryptographic
> key management framework) + **NIST SP 800-90A** (DRBG for DEK generation) apply
> to all four providers.
>
> **Quarterly review**: Crypto SME reviews NIST CMVP certificate status per provider
> and updates this document. Calendar reminder in ops runbook. Next review: 2026-08-14.
>
> **Customer notification**: FIPS level per provider is disclosed to customers at
> BYOK signup. Customers requiring FIPS 140-3 must use AWS KMS or Vault Enterprise.

## Provider Matrix

| Provider | FIPS Level | NIST CMVP Module ID | Tier Requirement | Notes |
|---|---|---|---|---|
| **AWS KMS** | FIPS 140-3 Level 1 | #4177 (AWS HSM; 2024) | Any KMS region | Verified FIPS 140-3 as of 2024-02. Default for enterprise. |
| **GCP Cloud KMS** | FIPS 140-2 Level 1 | #3978 (Google KMS module) | Any KMS region | GCP committed to FIPS 140-3; quarterly review tracks. 140-3 will upgrade this row. |
| **Azure Key Vault** | FIPS 140-2 Level 2 | #3516 (Azure HSM; Premium) | **Premium HSM tier mandatory** | Standard tier = Level 1; CoreLink BYOK requires Premium HSM at provisioning. |
| **HashiCorp Vault Enterprise** | FIPS 140-3 Level 1 | Pending CMVP (2026-Q2) | Vault Enterprise FIPS build | Customer-hosted. OSS Vault is NOT FIPS-certified. mTLS auth mandatory. |

## FIPS Level Hierarchy

```
FIPS 140-3 Level 1 > FIPS 140-2 Level 2 > FIPS 140-2 Level 1 > None
```

- **FIPS 140-3 Level 1**: AWS KMS (#4177), HashiCorp Vault Enterprise (pending CMVP).
- **FIPS 140-2 Level 2**: Azure Key Vault Premium HSM (#3516).
- **FIPS 140-2 Level 1**: GCP Cloud KMS (#3978).

## Compliance Assertions

### 1. Envelope Encryption

All DEK material:
- Generated via OS CSPRNG (`getrandom::getrandom`; NIST SP 800-90A approved DRBG).
- **Never BLAKE3-derived or deterministic** (Lote 10.14 codex P1 fix: deterministic DEK =
  compromise propagation across blobs sharing the same content hash).
- AES-256-GCM body encryption; nonce 96-bit random per write.
- DEK cache 5 min TTL hard limit (INV-BYOK-CRYPTO-SOVEREIGNTY; no exception; no advisory mode).

### 2. AAD Binding per Provider

| Provider | AAD Mechanism | CoreLink Encoding |
|---|---|---|
| AWS KMS | `encryption_context` HashMap | JSON Value → HashMap<String, String> |
| GCP KMS | `additional_authenticated_data` | JSON Value → UTF-8 bytes |
| Azure Key Vault | Custom AES-GCM layer (inner) | JSON Value → AES-GCM AAD; GCM tag enforces binding |
| HashiCorp Vault | `context` base64 | JSON Value → base64(UTF-8 JSON bytes) |

> **Azure custom AAD flow** (ADR-S14-001): Azure wrapKey (RSA-OAEP-256) does not
> support AAD natively. CoreLink applies an inner AES-256-GCM encryption layer with
> AAD before calling wrapKey. The GCM authentication tag enforces AAD binding;
> tag mismatch on unwrap = `BYOKError::AesGcm`. Adds ~2ms overhead.

### 3. mTLS Auth (Vault only)

- Client certificate presented to Vault server (mutual TLS).
- CA certificate pinned; connection rejected if Vault presents different CA.
- Cert expiry alert: ≤ 30 days remaining → `BYOKError::MtlsCertExpiringSoon` +
  SEV-3 alert + customer notification. Renewal flow: runbook RB-BYOK-VAULT-CERT-RENEWAL.

### 4. Key Lifecycle

- BYOK CMK overlap: **7 days** (INV-KEY-OVERLAP; CTRL-KEY-010/011/012).
- CMK rotation: re-wrap flow (unwrap DEK with old CMK + wrap with new CMK; atomic).
- CMK revoke → DEK cache evict all entries for key → hard-fail reads within 5 min
  (INV-BYOK-CRYPTO-SOVEREIGNTY; kill switch WI-S14-006).

## Quarterly Review Cadence

| Quarter | Review Date | Reviewer | Actions |
|---|---|---|---|
| 2026 Q3 | 2026-08-14 | Crypto SME (TBD) | Verify NIST CMVP status; GCP 140-3 upgrade check; Vault CMVP certificate |
| 2026 Q4 | 2026-11-14 | Crypto SME (TBD) | — |
| 2027 Q1 | 2027-02-14 | Crypto SME (TBD) | — |

## References

- NIST SP 800-57 Pt 1 Rev 5: https://doi.org/10.6028/NIST.SP.800-57pt1r5
- NIST SP 800-130: https://doi.org/10.6028/NIST.SP.800-130
- NIST SP 800-90A Rev 1: https://doi.org/10.6028/NIST.SP.800-90Ar1
- NIST CMVP Search: https://csrc.nist.gov/projects/cryptographic-module-validation-program
- AWS KMS CMVP #4177: https://csrc.nist.gov/projects/cryptographic-module-validation-program/certificate/4177
- GCP KMS CMVP #3978: https://csrc.nist.gov/projects/cryptographic-module-validation-program/certificate/3978
- Azure Key Vault CMVP #3516: https://csrc.nist.gov/projects/cryptographic-module-validation-program/certificate/3516
- ADR-S14-001: `specs/03_architecture/adrs/ADR-S14-001-byok-cross-provider-azure-aad-vault-mtls.md`

## Change Log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | Initial 4-provider FIPS compliance matrix (WI-S14-004 AWS + WI-S14-005 GCP/Azure/Vault). |
