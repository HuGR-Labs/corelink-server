---
doc_id: "COMPLIANCE-BYOK-FIPS-MATRIX"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
status: "ACTIVE"
wi_scope:
  - "WI-S14-004"  # AWS row — SEALED this WI
  - "WI-S14-005"  # GCP / Azure / Vault rows — populated next WI
review_cadence: "quarterly"
next_review: "2026-08-14"
references:
  - "NIST FIPS 140-3 (CMVP)"
  - "NIST FIPS 140-2 (legacy — sunset 2026-09-22)"
  - "NIST SP 800-57 Pt 1 Rev 5"
  - "NIST SP 800-130"
  - "NIST FIPS 197 (AES)"
  - "NIST FIPS 186-5 (Ed25519)"
---

# BYOK FIPS Compliance Matrix

This document records the FIPS certification status for each KMS provider
supported in CoreLink's BYOK enterprise tier.

> **Quarterly review required.** Any change to a provider's NIST CMVP module
> status must trigger an alert and immediate update to this document.

---

## Summary table

| Provider | FIPS Level | CMVP Certificate | Notes | WI |
|---|---|---|---|---|
| **AWS KMS** | **FIPS 140-3 Level 1** | `#4523` | Default; no extra config. | WI-S14-004 ✓ |
| GCP Cloud KMS | FIPS 140-2 Level 1 | TBD (WI-S14-005) | 140-3 pending GCP certification. | WI-S14-005 |
| Azure Key Vault Premium | FIPS 140-2 Level 2 | TBD (WI-S14-005) | HSM-backed; L2 validated. | WI-S14-005 |
| HashiCorp Vault Enterprise | FIPS 140-3 Level 1 | TBD (WI-S14-005) | Vault Enterprise FIPS build. | WI-S14-005 |

---

## AWS KMS — FIPS 140-3 Level 1 (WI-S14-004)

**Status:** ACTIVE — certified.

| Field | Value |
|---|---|
| FIPS Standard | FIPS 140-3 |
| Security Level | Level 1 |
| NIST CMVP Certificate | `#4523` |
| Module Name | AWS Key Management Service HSM |
| Cryptographic Operations | AES-256 (FIPS 197), RSA-2048, EC P-256, HMAC-SHA-256 |
| Key wrapping algorithm | AES-256 symmetric key wrap |
| Region availability | All AWS commercial + GovCloud regions |
| Endpoint for FIPS | `kms.us-east-1.amazonaws.com` (standard) / `kms-fips.us-east-1.amazonaws.com` (explicit FIPS TLS endpoint) |
| CoreLink configuration | Standard SDK; FIPS 140-3 L1 is the default for all `kms:Encrypt` / `kms:Decrypt` |
| IAM scope | `kms:Encrypt`, `kms:Decrypt`, `kms:DescribeKey` — minimal |

### References

- AWS KMS FIPS 140-3 documentation:
  <https://docs.aws.amazon.com/kms/latest/developerguide/keystore-cloudhsm.html#fips-140-3>
- NIST CMVP module search: <https://csrc.nist.gov/projects/cryptographic-module-validation-program>
- AWS GovCloud FIPS endpoint: <https://docs.aws.amazon.com/general/latest/gr/kms.html>

### Envelope encryption algorithms (AWS KMS path)

| Layer | Algorithm | Standard |
|---|---|---|
| DEK generation | CSPRNG (OS `getrandom`; AES-CTR-DRBG or Hash-DRBG) | NIST SP 800-90A |
| Body encryption | AES-256-GCM | FIPS 197 + NIST SP 800-38D |
| Nonce | 96-bit random per-write | NIST SP 800-38D §8.2.1 |
| DEK wrap | AES-256 key wrap via AWS KMS | FIPS 197 + SP 800-38F |
| AAD | `{"tenant_id": "...", "blob_hash": "..."}` | Provider-enforced |

---

## GCP Cloud KMS — FIPS 140-2 Level 1 (WI-S14-005)

**Status:** PENDING — populated in WI-S14-005.

> GCP KMS is FIPS 140-2 Level 1 certified.  FIPS 140-3 certification is in progress
> as of 2026-Q1.  This row will be updated when WI-S14-005 is sealed with verified
> CMVP module ID.

---

## Azure Key Vault Premium — FIPS 140-2 Level 2 (WI-S14-005)

**Status:** PENDING — populated in WI-S14-005.

> Azure Key Vault Premium tier uses HSM-backed keys certified to FIPS 140-2 Level 2
> (HSM module).  Upgrade to 140-3 pending Azure CMVP resubmission.

---

## HashiCorp Vault Enterprise — FIPS 140-3 Level 1 (WI-S14-005)

**Status:** PENDING — populated in WI-S14-005.

> HashiCorp Vault Enterprise ships a dedicated FIPS build (`vault-enterprise-fips1402`)
> using a FIPS 140-3 validated cryptographic module.  Customer-hosted on FIPS-validated
> infrastructure; mTLS auth; transit secrets engine for DEK wrap.

---

## Waiver policy

Per `_spec_contract.md §19`, the following cannot be waived:

- FIPS documentation per provider is **non-waivable** before S-14 GA gate.
- AWS KMS FIPS 140-3 L1 documented here satisfies the WI-S14-004 gate.
- GCP / Azure / Vault rows must be populated before WI-S14-005 SEAL.

---

## Changelog

| Date | WI | Change |
|---|---|---|
| 2026-05-14 | WI-S14-004 | Initial creation — AWS KMS FIPS 140-3 L1 row added. |
