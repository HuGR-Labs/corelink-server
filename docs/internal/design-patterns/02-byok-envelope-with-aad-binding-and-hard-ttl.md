# Design Pattern 02 — BYOK Envelope Encryption with Triple AAD-Binding and Hard-Capped DEK Cache

**Location:** `crates/corelink-byok-core/src/{envelope,dek_cache}.rs`
**Invariants protected:** `INV-BYOK-CRYPTO-SOVEREIGNTY`, `INV-CONF-AT-REST`
**Threats addressed:** Wrapped-DEK swap attacks, post-revocation read-after-revoke window, AAD-stripping

---

## 1. Problem

Customer-held-key envelope encryption (BYOK) is widely advertised but often shallowly implemented. Two subtle attack classes survive naive implementations:

### 1.1 Wrapped-DEK swap attack

Tenant A has a legitimate `WrappedDek` produced by their own KMS CMK. Tenant A also has read access to tenant B's `ciphertext` blob (e.g., via a misconfigured ACL, a compromised D1 row, or a forensic dump). A naive envelope scheme:

```
unwrap_dek(wrapped_dek_A) → DEK_A
aes_gcm_decrypt(ciphertext_B, DEK_A) → ??? (gibberish, usually)
```

…fails only because AES-GCM authentication catches the mismatch *probabilistically*. But if the attacker has any wrapped DEK that was ever used to encrypt this specific blob (e.g., re-encryption history, cache poisoning, a legacy backup), the decrypt succeeds. Worse: in schemes where the AAD only binds `tenant_id` (not `blob_hash`), an attacker who is the legitimate owner of a *different* blob in the same tenant can swap wrapped DEKs and recover the wrong blob's plaintext — a cross-blob isolation break.

### 1.2 Post-revocation read window

When a customer revokes their CMK (the canonical kill-switch), the service must stop being able to read any of that customer's data. If the service caches unwrapped DEKs indefinitely, the kill-switch is **advertised but not real**. The customer believes revocation = data goes dark; the reality is "data goes dark whenever the cache expires, which is whenever."

## 2. Solution

Three coupled controls, each load-bearing:

### 2.1 Triple AAD-binding (`envelope.rs:88, 219-224`)

```rust
fn build_aad(tenant_id: &str, blob_hash: &str) -> serde_json::Value {
    serde_json::json!({
        "tenant_id": tenant_id,
        "blob_hash": blob_hash,
    })
}
```

The AAD passed to `KmsProvider::wrap_dek(..., Some(&aad))` includes **both** the tenant identifier **and** the blob hash. KMS encodes this AAD into the wrapped-DEK ciphertext (AWS KMS `EncryptionContext`, GCP KMS `additionalAuthenticatedData`, Azure Key Vault wrapping context, Vault Transit `context`). On unwrap, the AAD presented MUST match the AAD at wrap time, or KMS itself refuses to unwrap.

### 2.2 AAD-mismatch detection BEFORE KMS unwrap (`envelope.rs:135-157`)

```rust
// Validate AAD field present.
if blob.wrapped_dek.encryption_context.is_none() {
    return Err(BYOKError::EncryptionContextMissing);
}

// Verify AAD matches expected tenant/blob binding.
let expected_aad = build_aad(tenant_id, blob_hash);
let stored_aad = blob.wrapped_dek.encryption_context.as_ref()?;
if stored_aad != &expected_aad {
    return Err(BYOKError::AadMismatch);
}

// Only THEN call KMS.
let fresh_dek = self.provider.unwrap_dek(&blob.wrapped_dek).await?;
```

The local AAD pre-check defends in depth and prevents wasted KMS calls on attack traffic. The KMS itself is the canonical authority — but the local check rejects **AAD-stripping attempts** where an attacker submits a `WrappedDek` with `encryption_context: None` (some KMS providers accept context-less unwrap in legacy modes).

### 2.3 Hard 5-minute TTL cap on DEK cache (`dek_cache.rs:31-32, 84-91`)

```rust
const MAX_TTL_SECONDS: u64 = 300;

pub fn new(ttl_seconds: u64) -> Result<Self, BYOKError> {
    if ttl_seconds > MAX_TTL_SECONDS {
        return Err(BYOKError::DekCacheTtlExceeded { got: ttl_seconds });
    }
    // ...
}
```

The constructor **rejects any TTL > 300 s** with no override path, no advisory mode, no operator escape hatch. This is what makes the **customer kill-switch SLA** (≤6 min p99 global detection + eviction after CMK revocation) into a **structural invariant** rather than a marketing claim.

Coupled eviction paths:

| Trigger | Behavior |
|---|---|
| TTL expiry | Entry silently dropped on next access |
| CMK revocation event | `DekCache::evict_all_for_key(&KmsKeyId)` — atomic clear of all entries for that key; each evicted `Dek` calls `ZeroizeOnDrop` |
| Capacity | Bounded LRU at 10 000 entries per Worker |
| Process termination | All `Dek` zeroized via `ZeroizeOnDrop` |

## 3. Why each piece is load-bearing

| Remove this piece | Attack that becomes possible |
|---|---|
| AAD binding to `blob_hash` | Cross-blob DEK-swap within a tenant; legacy wrapped-DEK replay |
| Local AAD-mismatch pre-check | AAD-stripping via legacy KMS unwrap modes; attack traffic burns KMS quota |
| Hard 5-min TTL cap | Indefinite post-revocation read window; customer kill-switch becomes a lie |
| `ZeroizeOnDrop` on `Dek` | Plaintext DEK persists in freed heap pages; cold-boot / process-dump attacks |
| `OsRng`-based DEK generation | Deterministic-DEK attacks (KDF-derived DEKs are weaker against side channels) |

## 4. Implementation discipline

### 4.1 `getrandom` via OS CSPRNG, NIST SP 800-90A DRBG (`envelope.rs:202-216`)

```rust
fn generate_dek() -> Result<Dek, BYOKError> {
    let mut bytes = [0u8; 32];
    getrandom(&mut bytes)?;
    Ok(Dek { bytes })
}
```

Every DEK is freshly random — **never KDF-derived deterministically**. Same for the 96-bit AES-GCM nonce (`generate_nonce`). Property tests assert 1 000-sample distinctness.

### 4.2 No `unsafe` (`envelope.rs:19, dek_cache.rs:17`)

```rust
#![forbid(unsafe_code)]
```

Crate-level forbidden. Combined with the workspace-level lint policy (no `unwrap`/`expect`/`panic`/`todo`/`dbg`), this means **every byte of cryptographic code in this module passes the strictest Rust lints** in the workspace.

### 4.3 KMS provider abstraction with FIPS attestation

```rust
trait KmsProvider {
    fn fips_level(&self) -> FipsLevel;
    async fn wrap_dek(..., encryption_context: Option<&serde_json::Value>) -> Result<WrappedDek, BYOKError>;
    async fn unwrap_dek(...) -> Result<Dek, BYOKError>;
    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError>;
}
```

Four implementations: `corelink-byok-aws`, `corelink-byok-gcp`, `corelink-byok-azure`, `corelink-byok-vault`. Each carries a `FipsLevel` attestation (`Fips140_3_L1`, etc.) so compliance dashboards can surface the actual posture per CMK.

### 4.4 Access status surfacing

`KmsAccessStatus` enumerates `Ok`, `Revoked`, `PermissionDenied`, `KeyDisabled`, `KeyDeleted` — drives the kill-switch eviction loop. Polled by a background worker that calls `provider.check_access` on a 60-second cadence per active key; on transition to non-`Ok`, fires `DekCache::evict_all_for_key`.

## 5. Anti-patterns

| Anti-pattern | Why rejected |
|---|---|
| TTL > 300 s (any opt-in path) | Breaks customer kill-switch SLA — INV-BYOK-CRYPTO-SOVEREIGNTY |
| KDF-derived deterministic DEK | Weakens against side-channel + invalidates `getrandom`-based proofs |
| AAD = `tenant_id` only | Allows intra-tenant cross-blob swap |
| AAD = `blob_hash` only | Allows cross-tenant decrypt if both tenants share a blob hash (CAS dedup case!) |
| Skipping local AAD pre-check | AAD-stripping; KMS quota burn under attack |
| Storing `Dek` without `ZeroizeOnDrop` | Plaintext key persists in freed pages |
| Operator override flag on TTL | "Operator under duress" attack surface |

## 6. Verification

| Property | Test |
|---|---|
| Encrypt-decrypt roundtrip per provider | `test_encrypt_decrypt_roundtrip` |
| Cache hit on second decrypt avoids KMS call | `test_second_decrypt_uses_cache` |
| Cross-tenant AAD attempt rejected | `test_aad_mismatch_rejects` |
| Nonce uniqueness over 1 000 samples | `test_nonce_uniqueness_sample` |
| DEK uniqueness over 1 000 samples | `test_dek_uniqueness_sample` |
| TTL > 300 s rejected at constructor | `DekCache::new(301).is_err()` |
| Revocation eviction zeroizes all DEKs | (integration test in `corelink-byok-revocation`) |

## 7. Posture this enables

The hard 5-min cap + AAD-mismatch rejection + Zeroize discipline together mean CoreLink can make the following commitments **without weasel words** in customer-facing compliance documentation:

> "When you revoke your CMK at your KMS provider, CoreLink will be cryptographically unable to read your data within ≤6 minutes globally, with no operator-side override capable of restoring access. The wrapped DEKs stored alongside your blobs become inert."

That single sentence — backed by structural invariants rather than process controls — is the **selling argument** for regulated industries (financial, healthcare, defense) where "we promise we'll lock you out" is insufficient and "we are mathematically incapable of reading after revoke" is the bar.

## 8. References

- `specs/03_architecture/security_model.md §6.1` — BYOK threat model
- `specs/03_architecture/compliance_matrix.md` — SOC2 / ISO27001 mapping
- NIST SP 800-90A — DRBG / `getrandom` rationale
- NIST SP 800-38D — AES-GCM construction
- AWS KMS `EncryptionContext` documentation
- GCP Cloud KMS `additionalAuthenticatedData`
- Azure Key Vault wrapping documentation
- HashiCorp Vault Transit `context` parameter
- Kocher, P. (1996). "Timing Attacks on Implementations of Diffie-Hellman, RSA, DSS, and Other Systems." (Side-channel context)
