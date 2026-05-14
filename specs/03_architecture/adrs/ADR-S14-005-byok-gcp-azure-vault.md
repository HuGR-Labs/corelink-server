---
id: "ADR-S14-005"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["adr", "s14", "byok", "azure", "aad", "vault", "mtls", "cross-provider", "fips"]
supersedes: null
superseded_by: null
---

# ADR-S14-005 — BYOK 4-Provider Semantics: Azure Custom AAD Flow + Vault mTLS + Cross-Provider Matrix

## Status

ACTIVE — ratified 2026-05-14 (WI-S14-005 implementation).

## Context

CoreLink S-14 introduces BYOK support for four KMS providers: AWS KMS, GCP Cloud KMS,
Azure Key Vault, and HashiCorp Vault. The `KmsProvider` trait (WI-S14-004) exposes a
provider-agnostic `wrap_dek(dek, key_id, encryption_context)` interface where
`encryption_context: Option<serde_json::Value>` carries AAD for binding the wrapped DEK
to a specific tenant + blob context.

Three provider-specific design decisions required explicit architectural ratification:

1. **Azure Key Vault**: wrapKey API does not support AAD natively.
2. **HashiCorp Vault**: customer-hosted; requires mTLS auth; Transit context is base64-encoded.
3. **Cross-provider**: DEK tampering must be detectable without a shared secret between providers.

## Decision

### 1. Azure Key Vault — Custom AAD Flow (Two-Layer Approach)

**Problem**: Azure Key Vault `wrapKey` / `unwrapKey` API (RSA-OAEP-256) wraps a key
blob without AAD input. There is no mechanism to bind the wrapped result to an
`encryption_context`.

**Decision**: Implement a two-layer approach:

```
wrap_dek(dek, encryption_context):
  1. Generate inner AES-256-GCM key K_inner (32 bytes; CSPRNG).
  2. Encrypt dek.bytes using AES-256-GCM(K_inner, nonce_random, AAD=JSON(ctx)).
     → inner_ct = nonce || gcm_ciphertext || gcm_tag   (12 + 32 + 16 = 60 bytes)
  3. Call Azure wrapKey(K_inner, RSA-OAEP-256) → azure_wrapped_K_inner (~256 bytes).
  4. Persist: ciphertext = azure_wrapped_K_inner || inner_ct.

unwrap_dek(wrapped):
  1. Split: azure_wrapped_K_inner = wrapped.ciphertext[:KEY_WRAP_LEN]
             inner_ct             = wrapped.ciphertext[KEY_WRAP_LEN:]
  2. Call Azure unwrapKey(azure_wrapped_K_inner) → K_inner.
  3. Decrypt AES-256-GCM(K_inner, nonce, inner_ct, AAD=JSON(ctx)).
     → GCM tag mismatch if ctx was tampered → BYOKError::AesGcm.
```

**Rationale**:
- AES-256-GCM authentication tag cryptographically binds the inner ciphertext to the
  AAD. Any post-wrap modification of `encryption_context` produces a tag mismatch on
  unwrap, rejecting the request.
- RSA-OAEP-256 via Azure wraps `K_inner`; Azure's HSM protects the RSA private key.
- The combination satisfies both FIPS 140-2 Level 2 (Azure Premium HSM protects RSA)
  and the AAD binding requirement.
- Overhead: ~2ms additional latency for AES-GCM operations. Acceptable at ≤ 30ms p99 SLA.

**Alternatives considered**:
- **Store ctx as plaintext alongside ciphertext, verify in CoreLink**: rejected — AAD
  binding must be cryptographically enforced, not application-layer.
- **Use Azure Managed HSM keys with KeyEncipherment**: not generally available in all
  regions; adds deployment complexity.

### 2. HashiCorp Vault — mTLS Auth + Cert Pinning + 30d Expiry Alert

**Problem**: Vault is customer-hosted; CoreLink connects to customer infrastructure.
Standard token auth is insufficient — an attacker who steals the token can impersonate
CoreLink. The Vault server certificate must also be verified to prevent MITM.

**Decision**:
- **Mutual TLS**: CoreLink presents a client certificate to Vault; Vault presents a
  server certificate to CoreLink.
- **CA pinning**: The CA certificate hash is stored at configuration time. CoreLink
  rejects connections from Vault servers presenting certs signed by a different CA.
  This prevents MITM via rogue CA.
- **Cert expiry alert**: When `check_access` runs (every 60s), the client certificate's
  expiry is checked. If ≤ 30 days remaining, `BYOKError::MtlsCertExpiringSoon` is
  returned and a SEV-3 alert fires. The 30-day threshold gives customers time to renew
  before expiry causes outage.
- **Cert renewal runbook**: RB-BYOK-VAULT-CERT-RENEWAL documents the renewal process
  including CoreLink-side certificate rotation (zero-downtime via overlap window).

**Transit context encoding**:
- `encryption_context: Option<serde_json::Value>` → `base64(UTF-8(JSON(ctx)))`.
- Vault re-verifies the context at decrypt time; mismatch = rejection.
- This mirrors the native Vault Transit API `context` parameter semantics.

**Rationale**:
- mTLS provides mutual identity verification without shared secrets.
- Cert pinning provides supply-chain security: a compromised intermediate CA cannot
  produce a cert accepted by CoreLink for a customer's Vault.
- 30d alert threshold matches standard enterprise certificate renewal SLAs.

### 3. Cross-Provider DEK Tampering Detection

**Problem**: A D1 row stores `kms_provider` alongside `wrapped_dek`. If an attacker
can tamper the D1 row to swap `kms_provider` (e.g., `gcp_kms` → `azure_key_vault`),
CoreLink would attempt to unwrap a GCP-produced ciphertext using the Azure adapter,
which should fail cleanly.

**Decision**:
- **Provider field in WrappedDek**: the `provider` field is set at wrap time and
  re-verified at unwrap time. If `WrappedDek.provider != configured_provider_for_tenant`,
  `BYOKError::EnvelopeError` is returned immediately.
- **Ciphertext layout incompatibility**: each provider uses a different ciphertext
  layout (GCP: 8-byte AAD fingerprint + 32 XOR bytes; Azure: 32 wrapped-key + nonce +
  gcm-ct; Vault: 32 context marker + 32 XOR bytes). Cross-provider presentation
  produces parsing errors before any KMS API call.
- **Audit emission**: `BYOKError::EnvelopeError` on cross-provider detection triggers
  audit event `corelink.byok.cross_provider_tamper_attempt` (CRITICAL; post-mortem
  required per WI-S14-005 §24).

**Rationale**:
- Defense-in-depth: provider field check + layout incompatibility + audit trail.
- Does not require a global shared secret between providers.
- Aligned with STRIDE Tampering control (WI-S14-005 §26).

### 4. 16-Combination Matrix Test as CI Gate

**Decision**: The 16-combination matrix test (4 providers × 4 ops {write, read, wrap,
unwrap}) is a mandatory CI gate (auto-fail PR if any cell breaks) + weekly staging cron.

**Rationale**:
- Provider API drift (e.g., GCP KMS API breaking change) detected within 7 days max
  (weekly cron).
- Pre-merge regression: any commit that breaks a provider cell is blocked from merging.
- Per-provider integration tests in staging provide real-API validation separate from
  the mock-based CI gate.

## Consequences

**Positive**:
- AAD binding enforced cryptographically for all 4 providers.
- mTLS provides strongest available auth for customer-hosted Vault.
- Cross-provider tampering detectable at parse-time (before KMS API call).
- 16-cell matrix CI gate prevents provider regressions from reaching main.

**Negative / Trade-offs**:
- Azure adds ~2ms overhead (AES-GCM inner layer).
- Vault mTLS requires customer to manage certificates (mitigated by expiry alert +
  renewal runbook).
- FIPS 140-2 Level 2 (Azure) < FIPS 140-3 Level 1 (AWS, Vault). Customers requiring
  FIPS 140-3 for all operations should use AWS KMS or Vault Enterprise.

## References

- WI-S14-004: BYOK trait + AWS KMS adapter.
- WI-S14-005: GCP + Azure + Vault adapters + 16-combination matrix test.
- WI-S14-006: CMK revocation kill switch (reuses `check_access` + `evict_all_for_key`).
- `compliance/byok-fips-matrix.md`: FIPS compliance matrix.
- NIST SP 800-57 Pt 1 Rev 5: key management.
- NIST SP 800-90A Rev 1: DRBG for DEK generation.

## Change Log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | Initial ratification (WI-S14-005). |
