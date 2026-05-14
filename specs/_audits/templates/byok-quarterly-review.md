---
id: "TEMPLATE-BYOK-QUARTERLY-REVIEW"
type: "audit_template"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
tags: ["byok", "crypto-sme", "quarterly-review", "fips", "nist-cmvp", "template"]
---

# BYOK Quarterly Crypto SME Review — Template

> **Usage**: Copy this template for each quarterly review. Fill in the review date,
> reviewer name, and findings. Commit to `specs/_audits/<date>-byok-quarterly-review-q<N>.md`.
>
> **Cadence**: Every 3 months. Calendar reminder in ops runbook.
> **Reviewer**: Crypto SME (mandatory; may not be waived).

## Review Metadata

| Field | Value |
|---|---|
| Review Date | _YYYY-MM-DD_ |
| Quarter | _YYYY Q1/Q2/Q3/Q4_ |
| Reviewer | _Name (Crypto SME)_ |
| Reviewed by | _Name (Architect or Security Lead)_ |
| Status | _PASS / CONDITIONAL / FAIL_ |

## 1. NIST CMVP Status Check

For each provider, verify the CMVP certificate is still active and the module
is not on the Historical or Revoked list.

| Provider | FIPS Level | CMVP Module ID | CMVP Status | Action Required |
|---|---|---|---|---|
| AWS KMS | FIPS 140-3 L1 | #4177 | _Active / Historical / Revoked_ | — |
| GCP Cloud KMS | FIPS 140-2 L1 | #3978 | _Active / Historical / Revoked_ | — |
| Azure Key Vault Premium HSM | FIPS 140-2 L2 | #3516 | _Active / Historical / Revoked_ | — |
| HashiCorp Vault Enterprise | FIPS 140-3 L1 | _Pending_ | _Active / Pending / Historical_ | — |

**GCP 140-3 upgrade check**: Has GCP published a FIPS 140-3 CMVP certificate for
Cloud KMS since the last review? If yes, update `compliance/byok-fips-matrix.md`
and `corelink-byok-gcp/src/lib.rs` `fips_level()` return value.

## 2. Algorithm Review per Provider

| Provider | Algorithm | Standard | Still Approved? | Notes |
|---|---|---|---|---|
| GCP KMS | AES-256 symmetric key wrap | NIST SP 800-38F | _Yes / No_ | — |
| Azure Key Vault | RSA-OAEP-256 (wrapKey) + AES-256-GCM (inner) | NIST SP 800-131A | _Yes / No_ | — |
| Vault Transit | AES-256 (transit encrypt) | NIST SP 800-38A | _Yes / No_ | — |
| All providers | AES-256-GCM (DEK body encryption) | NIST SP 800-38D | _Yes / No_ | — |
| All providers | CSPRNG DEK generation | NIST SP 800-90A | _Yes / No_ | — |

## 3. Azure Custom AAD Flow Review

- [ ] AES-256-GCM inner layer still recommended for AAD binding.
- [ ] Azure wrapKey AAD support status: has Azure added native AAD to wrapKey? (If yes, evaluate migration; update ADR-S14-001.)
- [ ] GCM tag length (128-bit) still meets requirements.
- [ ] Inner key generation (32-byte CSPRNG) still compliant.

**Findings**: _None / See below_

## 4. Vault mTLS Review

- [ ] mTLS protocol version: TLS 1.3 preferred; TLS 1.2 minimum. Current: _____
- [ ] Client cert algorithm: RSA-2048+ or ECDSA P-256+. Current: _____
- [ ] CA cert pinning still active in production.
- [ ] No certificates due for expiry in next quarter.
- [ ] Cert renewal process tested in last year.

**Findings**: _None / See below_

## 5. Matrix Test Review

- [ ] Weekly staging matrix (16 cells) has been green for all 13 weekly runs since last review.
- [ ] Any cells that broke and were fixed: _list or "none"_.
- [ ] proptest 100k iter nightly green.
- [ ] Adversarial regression suite 16 scenarios green.

**Findings**: _None / See below_

## 6. Customer Notification Review

- [ ] FIPS level per provider still documented in customer-facing docs.
- [ ] Any provider FIPS level downgrade? (e.g., Azure HSM module revoked → alert customers.)
- [ ] Customer-facing compliance matrix (`compliance/byok-fips-matrix.md`) up to date.

## 7. Actions Identified

| # | Action | Owner | Due Date | Priority |
|---|---|---|---|---|
| 1 | _Description_ | _Owner_ | _YYYY-MM-DD_ | P0/P1/P2 |

## 8. Sign-off

| Role | Name | Date | Status |
|---|---|---|---|
| Crypto SME | _Name_ | _YYYY-MM-DD_ | _APPROVED / CONDITIONAL / REJECTED_ |
| Architect | _Name_ | _YYYY-MM-DD_ | _APPROVED / CONDITIONAL / REJECTED_ |

---

_Template version 1.0.0 — WI-S14-005_
