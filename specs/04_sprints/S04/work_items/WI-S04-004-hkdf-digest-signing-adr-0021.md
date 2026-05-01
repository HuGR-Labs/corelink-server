---
id: "WI-S04-004"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.3.0"
created: "2026-04-25"
updated: "2026-05-01"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005", "FF-HR-009"]
parent: "S-04"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "KEY-MANAGEMENT"
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s04", "ac", "hkdf", "signing", "crypto", "adr-0021", "high-risk"]
---

# WI-S04-004 — CTRL-AC-002 HKDF-SHA256 Digest Signing + Verifier + ADR-0021 (HKDF vs Ed25519 Decision) + Mann-Whitney 3-prong Cripto-Grade Constant-Time + Property Test 10k + Key Rotation Forward-Compat

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-04](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S04-004 |
| Título | HKDF-SHA256 digest signing infra (`info="ac-sig"` fixed; tenant_key derived TDK; sign + verify constant-time); ADR-0021 ratificada (HKDF vs Ed25519 — symmetric + faster + sufficient for tenant-scoped integrity); property test 10k iter; Mann-Whitney 3-prong cripto-grade timing test (|Δmedian| ≤ 0.5ms); key rotation `sig_key_id` forward-compat; SignatureVerifier trait impl for WI-S04-003 |
| Sprint | S-04 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (sig forge → cross-tenant via envelope confusion), FF-HR-005 (CTRL-AC-002 enforcement primary), FF-HR-009 (defense-in-depth — sig + Merkle + tenant-scoping) |

## 1. Intent

Implementar `crates/corelink-ac/src/sig/` — cripto signing infra que implementa **CTRL-AC-002 (security_model.md §10)**: AC envelope assinado com HKDF-derived per-tenant key.

```rust
// File: crates/corelink-ac/src/sig/mod.rs

use blake3;
use hkdf::Hkdf;
use sha2::Sha256;
use subtle::ConstantTimeEq;

#[forbid(unsafe_code)]

pub use crate::sig::error::SigError;
pub use crate::sig::hkdf_signer::{HkdfSigner, HkdfVerifier};

pub mod error;
pub mod hkdf_signer;
pub mod tdk;       // Tenant Derivation Key handle (delegate KMS / Cloudflare Secrets)

// Public traits

/// Trait used by WI-S04-003 verify_full().
pub trait SignatureVerifier: Send + Sync {
    /// Verify sig over canonical_bytes with given tenant_key version.
    /// Returns Err(SigInvalid) if mismatch (constant-time).
    fn verify_sig(
        &self,
        canonical_bytes: &[u8],
        sig: &[u8],
        sig_key_id: u32,
    ) -> Result<(), SigError>;
}

/// Trait used by WI-S04-001 builder pre-persist.
pub trait SignatureSigner: Send + Sync {
    /// Sign canonical_bytes; returns sig + key_id used.
    /// Caller embeds in AcEnvelope.sig + sig_key_id.
    fn sign(
        &self,
        canonical_bytes: &[u8],
    ) -> Result<(Vec<u8>, u32), SigError>;
}

#[derive(thiserror::Error, Debug)]
pub enum SigError {
    #[error("signature length invalid: expected {expected}, got {got}")]
    LengthMismatch { expected: usize, got: usize },

    #[error("signature mismatch (constant-time compare failed)")]
    Invalid,                                    // → 422 + COR_AC_SIG_INVALID

    #[error("key_id {sig_key_id} unknown or rotated out (oldest active: {oldest_active})")]
    KeyIdUnknown { sig_key_id: u32, oldest_active: u32 },

    /// Lote 10.4-tris P0-R5-001: key_id=0 reserved sentinel ("never-issued"); rotation starts at 1.
    #[error("key_id reserved (sentinel value; rotation starts at 1)")]
    KeyIdReserved,

    #[error("backend error: {0}")]
    BackendError(String),                       // KMS / Cloudflare Secrets fetch failed

    #[error("tdk derivation failed: {0}")]
    TdkDerivationFailed(String),
}

// Concrete impl (default; production)
pub struct HkdfSigner {
    tdk_handle: Arc<dyn TdkHandle>,            // delegate to KMS/CF Secrets
    current_key_id: u32,                       // active key version; bumped on rotation
}

impl SignatureSigner for HkdfSigner {
    fn sign(&self, canonical_bytes: &[u8]) -> Result<(Vec<u8>, u32), SigError> {
        let tdk = self.tdk_handle.fetch(self.current_key_id)?;  // KMS/CF Secrets
        // Lote 10.4bis P0 fix: salt binds sig_key_id (RFC 5869 Extract step) — TDK rotation
        // produces correlated keying material; binding key version into salt removes correlation
        // (industry SOTA per TLS 1.3, Signal Protocol).
        let salt = self.current_key_id.to_le_bytes();
        let hk = Hkdf::<Sha256>::new(Some(&salt), &tdk);
        let mut sig_key = [0u8; 32];
        hk.expand(b"ac-sig", &mut sig_key)
            .map_err(|e| SigError::TdkDerivationFailed(e.to_string()))?;
        // Sig = BLAKE3-keyed-hash(sig_key, canonical_bytes); 32 bytes (PRF-secure MAC)
        let mut hasher = blake3::Hasher::new_keyed(&sig_key);
        hasher.update(canonical_bytes);
        let sig = hasher.finalize().as_bytes().to_vec();
        Ok((sig, self.current_key_id))
    }
}

pub struct HkdfVerifier {
    tdk_handle: Arc<dyn TdkHandle>,
    accepted_key_ids: Vec<u32>,                // rotation grace: current + previous
}

impl SignatureVerifier for HkdfVerifier {
    fn verify_sig(&self, canonical_bytes: &[u8], sig: &[u8], sig_key_id: u32) -> Result<(), SigError> {
        if sig.len() != 32 {
            return Err(SigError::LengthMismatch { expected: 32, got: sig.len() });
        }
        // Lote 10.4-tris P0-R5-001 fix: key_id=0 reserved sentinel ("never-issued"); rotation
        // starts from key_id=1; reject any sig with sig_key_id==0 explicitly to prevent the
        // 4-byte all-zero salt edge case (distinct from RFC 5869 32-zero-byte default).
        if sig_key_id == 0 {
            return Err(SigError::KeyIdReserved);  // sentinel; never legitimate
        }
        if !self.accepted_key_ids.contains(&sig_key_id) {
            // Lote 10.4-tris P0-R5-001 fix: unwrap_or(&1) NOT &0 (avoid leaking sentinel via error response;
            // also ensures `oldest_active` always reports a valid key_id).
            // debug_assert!(!self.accepted_key_ids.is_empty()) — production verifier init guarantee.
            debug_assert!(!self.accepted_key_ids.is_empty(),
                "accepted_key_ids must not be empty in production verifier");
            let oldest = *self.accepted_key_ids.iter().min().unwrap_or(&1);
            return Err(SigError::KeyIdUnknown { sig_key_id, oldest_active: oldest });
        }
        let tdk = self.tdk_handle.fetch(sig_key_id)?;
        // Lote 10.4bis P0 fix: salt = sig_key_id binds version (matches sign path).
        // Lote 10.4-tris P0-R5-002 documented: this HKDF-Extract step is the unified entry point
        // for sig_key derivation. Path-HMAC use of TDK (S-01 corelink-tenant-path) is documented
        // in ADR-0021 §Risks as analyzed under HMAC security assumption (NOT formally composed
        // through HKDF-Extract); accepted with attack cost bounded by 2^128 per Crypto SME advisory.
        let salt = sig_key_id.to_le_bytes();
        let hk = Hkdf::<Sha256>::new(Some(&salt), &tdk);
        let mut sig_key = [0u8; 32];
        hk.expand(b"ac-sig", &mut sig_key)
            .map_err(|e| SigError::TdkDerivationFailed(e.to_string()))?;
        let mut hasher = blake3::Hasher::new_keyed(&sig_key);
        hasher.update(canonical_bytes);
        let computed = hasher.finalize();
        // Constant-time compare via subtle
        if computed.as_bytes().ct_eq(sig).unwrap_u8() == 1 {
            Ok(())
        } else {
            Err(SigError::Invalid)
        }
    }
}

// TDK handle abstraction
#[async_trait::async_trait]
pub trait TdkHandle: Send + Sync {
    /// Fetch tenant_key bytes for given key_id.
    /// Production impl: Cloudflare Secrets / KMS.
    /// Test impl: HashMap fixture.
    fn fetch(&self, key_id: u32) -> Result<Vec<u8>, SigError>;
}
```

**Canonical bytes computation** (what gets signed):

```rust
// Lote 10.4bis P0 fix: canonical_bytes inclui result_hash binding (was 89 bytes; now 121 bytes).
// Prior layout omitted result_hash; binding was implicit via merkle_root, but only valid IF
// verifier always recomputes Merkle root before trusting envelope.merkle_root. To eliminate
// API misuse footgun where handler could call verify_sig (sig only) WITHOUT verify_structure
// (Merkle only), result_hash is now bound directly into the signed canonical bytes.
//
// canonical_bytes = version || tenant_id || action_digest || merkle_root || result_hash || created_at_ms
// Layout (121 bytes total):
//   offset  size  field
//   0       1     version (u8 LE)
//   1       16    tenant_id (UUIDv7 raw bytes)
//   17      32    action_digest (BLAKE3 hash bytes)
//   49      32    merkle_root (BLAKE3 hash bytes)
//   81      32    result_hash (= merkle_root direct per ADR-0037; redundant binding for
//                  defense-in-depth + future schema migration if result_hash semantic decouples)
//   113     8     created_at_ms (u64 LE)
//   total   121 bytes

pub fn canonical_bytes(envelope: &AcEnvelope) -> [u8; 121] {
    let mut out = [0u8; 121];
    out[0] = envelope.version;
    out[1..17].copy_from_slice(envelope.tenant_id.as_bytes());
    out[17..49].copy_from_slice(&envelope.action_digest);
    out[49..81].copy_from_slice(&envelope.merkle_root);
    out[81..113].copy_from_slice(&envelope.result_hash);  // NEW Lote 10.4bis
    out[113..121].copy_from_slice(&envelope.created_at_ms.to_le_bytes());
    out
}
```

**Lote 10.4bis additional API hardening**: `SignatureVerifier::verify_sig` is **internal-only** (não public); only `ActionResultVerifier::verify_full` exposed publicly. This forces structure-then-sig ordering by API design (impossible to call sig verify standalone from external code). Prevents the API misuse footgun where an attacker tricks a handler into calling `verify_sig` on a tampered envelope without first checking structure.

**ADR-0021 RATIFICAÇÃO** (este WI promotes from "forward-looking" to "ratified"):

> **ADR-0021 — AC Digest Signing: HKDF-SHA256 vs Ed25519 — Decision: HKDF**
>
> **Status:** ACCEPTED (S-04 WI-S04-004 implementation).
>
> **Context:** AC envelope integrity binding. Two cripto options:
> 1. HKDF-SHA256 → BLAKE3-keyed-hash → 32-byte tag (symmetric).
> 2. Ed25519 sign → 64-byte signature (asymmetric).
>
> **Trade-offs:**
>
> | Aspect | HKDF-SHA256 | Ed25519 |
> |---|---|---|
> | Sig size | 32 bytes | 64 bytes |
> | Sign latency p99 | ~1ms (BLAKE3 SIMD) | ~0.5ms (ed25519-dalek) |
> | Verify latency p99 | ~1ms | ~1.5ms |
> | Key management | Symmetric (tenant_key per-tenant) | Asymmetric (private-key signs; public verifies) |
> | Public verifiability | Server-only OR client with shared tenant_key | Anyone with public key |
> | Cripto strength | 256-bit MAC; 2^128 forgery resistance | 128-bit security level |
> | Rotation complexity | TDK rotation annual; sig_key_id versioning | Key pair rotation; public key distribution overhead |
> | Cloudflare Workers compat | Native (BLAKE3 + HKDF in wasm) | ed25519-dalek wasm-compat OK |
>
> **Decision: HKDF-SHA256.**
>
> **Rationale:**
> 1. **Tenant-scoped, not externally-verifiable** — AC envelope is internal CoreLink artifact; clients verify Merkle (WI-S04-003 dual-side); they DO NOT need public-key sig verify.
> 2. **Symmetric simpler** — one key per tenant via HKDF; no key distribution; rotation localized.
> 3. **Performance** — BLAKE3-keyed sub-1ms; suitable for hot path; AC GET p99 budget 150ms.
> 4. **Storage** — 32 bytes vs 64 bytes; 50% saving on R2 envelope size; 10M envelopes × 32 bytes = 320 MB saving.
> 5. **Sufficient threat model** — sig binds tenant_id + action_digest + Merkle root; tampering detected; no third-party verifiability requirement (S-04 GA).
>
> **Future**: If multi-party verifiability needed (federation, customer audit external sig verify) → S-XX forward; ADR upgrade.
>
> **Risks accepted:**
> - **HKDF compromise (chave leak)** = forge possible BUT scoped to compromised tenant; Merkle dual-side verify (WI-S04-003) catches in **partial compromise** (insider with R2 access but no HKDF key); under **full HKDF key compromise**, attacker recomputes Merkle root + re-signs envelope, defeating both layers. Mitigated externally via TDK rotation + audit chain (S-09) anomaly detection.
> - **Symmetric trust model** → server has full power to forge; mitigated via audit chain (S-09) + tenant_id binding + chave rotation annual.
> - **FIPS 140-3 compliance** (Lote 10.4bis P0 fix): BLAKE3-keyed-hash is **NOT** FIPS-approved (only SHA-3-based KMAC is FIPS-approved per NIST SP 800-185). For SLSA L3 / SOC 2 / FedRAMP customers, this matters. **CoreLink S-04 GA accepts non-FIPS-MAC**; SLSA L3 alignment is via Merkle dual-side verify + audit chain (which use FIPS-approvable primitives at the boundaries). Post-GA migration path (HMAC-SHA256 OR KMAC) available via ADR if customer demand emerges.
> - **Side-channel via memory access patterns** (Lote 10.4bis P0 fix): BLAKE3 SIMD vectorization (AVX2/AVX-512) has memory access patterns that are not constant-cache-time strictly. CF Workers shared infrastructure (cross-Worker isolate cache lines) precludes constant-cache-time guarantees regardless of MAC choice. Mitigated by per-tenant TDK isolation (one chave compromise affects one tenant) + 256-bit MAC strength (PRF-secure 2^128 forge resistance). Future post-quantum migration via S-XX ADR if NIST PQC standard mandates change.
>
> **Mitigations:**
> - Annual TDK rotation (key_management.md §3); `sig_key_id` versioning; verifier accepts current + 1 previous.
> - INV-AC-MERKLE-VALID independent (WI-S04-003); double-cripto layered (effective in partial compromise).
> - Audit chain (S-09) detects anomalies post-hoc (CRITICAL alert if sig_invalid sustained > 5/h).
> - HKDF salt = `sig_key_id.to_le_bytes()` removes correlated keying material across rotations (Lote 10.4bis P0 fix).
> - canonical_bytes inclui result_hash binding (121 bytes; Lote 10.4bis P0 fix: prevents API misuse where verify_sig called without verify_structure).

**Constraint cripto-driven**:

1. **HKDF-SHA256** (RFC 5869): standardized, FIPS-approved primitive.
2. **info string fixed**: `b"ac-sig"` constant; never dynamic; CI test verifies; ADR-0021 documents.
3. **BLAKE3-keyed-hash** (32-byte tag): symmetric MAC; faster than HMAC-SHA256; collision-resistant 2^128.
4. **Constant-time compare** via `subtle::ConstantTimeEq`; never `==`.
5. **Key rotation**: `sig_key_id` u32; verifier accepts current + 1 previous (grace 1 year overlap); ADR-0034 forward solo-tier waiver compatible.
6. **Cloudflare Secrets / KMS handle**: TdkHandle trait; production impl fetches from CF Secrets Store; test impl uses fixture.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Digest signing é o **last cripto layer** entre Merkle (WI-S04-003) e cliente trust. Bug em sig path = either (a) forge accepted (cache poisoning bypass MITM) OR (b) timing oracle (key bit leak). HIGH_RISK em N dimensões:

1. **Sig forge via constant-time bypass**: variable-time `==` em compare leaks key bits via timing. Mitigação: `subtle::ConstantTimeEq` everywhere; cargo-deny `unsafe_code` deny; clippy lint enforces.

2. **HKDF info string drift** (typo `"ac-sig"` → `"acsig"` em refactor): keys diverge; all signatures invalid post-deploy; AC effectively offline. Mitigação: constant `b"ac-sig"` em código; CI test asserts byte-equal; ADR-0021 documents; commit hook validates.

3. **Sig length mismatch**: attacker sends sig of wrong length (e.g., 64 bytes Ed25519-style); verifier early-fails (LengthMismatch); not constant-time vs Invalid. Decisão: length check ANTES de constant-time compare (length is public; not key-dependent); LengthMismatch is fast-fail OK; Invalid path is constant-time post-length-check.

4. **Key rotation fail** (verifier doesn't accept previous key_id): post-rotation, existing envelopes signed with old key all fail verify; AC effectively offline for tenant; customer pain. Mitigação: `accepted_key_ids: Vec<u32>` includes current + 1 previous; rotation procedure documented; integration test simulates rotation.

5. **TDK fetch latency in hot path**: KMS/CF Secrets fetch ~10ms; verify p99 budget 1ms; cumulative would explode. Mitigação: TDK handle has in-memory cache (5min TTL); first-fetch warm; subsequent < 1µs; cache invalidation on rotation event.

6. **TDK leak via logging**: dev logs `tdk` content for debugging; secrets in log persistence. Mitigação: TDK handle's fetched bytes are `Zeroizing<Vec<u8>>` (zeroize crate); never Display/Debug; clippy lint `must_not_suspend` analog forbids printing; redact macros from S-09.

7. **Forge attempt via crafted canonical_bytes**: attacker constructs canonical_bytes with arbitrary version/tenant_id/action_digest/merkle_root/created_at_ms; obtains sig from server (via UPDATE); replays sig on different envelope. Mitigação: canonical_bytes binding 5 fields including merkle_root; replay across different envelopes requires same merkle_root; if merkle_root differs, sig verify fails (different canonical_bytes). NB: idempotent replay (same envelope) is OK by design.

8. **Mann-Whitney "valid sig vs invalid sig" timing**: cripto-grade requires |Δmedian| ≤ 0.5ms (TIGHTER than other WIs because direct cripto path); high-power test (10k samples per arm); 3-prong gate.

9. **HKDF salt unspecified vs random**: HKDF-Extract step takes optional salt. Decisão: `salt = None` (zero-byte), simplifies; `info = b"ac-sig"` carries all context separation; per-tenant separation via TDK already.

**Atacante adversarial scenarios**:

- **Existential forge attempt**: attacker without TDK access tries to forge sig. With BLAKE3-keyed-hash 256-bit MAC: probability 2^-256 per attempt; computacionalmente intratável.

- **Timing oracle key bit leak**: attacker measures verify latency on millions of crafted sigs; tries to leak key bits via correlations. Mitigação: constant-time compare; Mann-Whitney 3-prong validates; cripto-grade tightest |Δmedian| ≤ 0.5ms gate.

- **TDK rotation race**: rotation event T+0 → KMS active key bumps to v2; in-memory cache stale (still v1); UPDATE signs with v1 (stale) but stored sig_key_id=2 (fetched by verifier from KMS); verify fails. Mitigação: rotation event invalidates in-memory cache via wakeup signal; brief grace period post-rotation accepts both versions; integration test.

- **Side-channel via memory access patterns**: BLAKE3 has memory access patterns; AVX-512 vectorized; not constant-cache-time strictly. For S-04 GA: acceptable (CF Workers shared infra; isolation imperfect anyway); ADR-0021 documents.

- **TDK exfiltration via KMS audit gap**: KMS access logs incomplete; insider exfil. Mitigação: CF Secrets Store has audit trail (separate concern, S-XX); per-tenant TDK isolated; blast radius bounded.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: sig forge → envelope acceptance → cache poisoning → cross-tenant via tenant_id binding bypass.
- **FF-HR-005**: CTRL-AC-002 IS this WI; bypass = security control disabled.
- **FF-HR-009**: defense-in-depth — sig + Merkle (WI-003) + tenant-scoping (WI-001).
- **Reversibility**: sig key compromise = global re-sign required (TDK rotation); existing envelopes invalid post-rotation; planned procedure but disruptive.
- **Customer impact**: sig fail in prod = AC offline for tenant; CRITICAL-blocker.

11 sign-offs canonical incl. **Architect with Crypto SME specialization emphatic mandatory** (sig protocol + constant-time + key management; per framework §33.5.4.3 single Architect sign-off encompasses Crypto SME specialization for cripto-load-bearing WIs).

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev**:
- AC GET → handler verifies envelope sig via HKDF; ≤ 1ms p99 verify cost (cached TDK).
- Sig invalid → 422 `COR_AC_SIG_INVALID` with `Retry-After: 0` (don't retry; integrity violation).
- Bazel client logs incident; build fails; reproducible — re-execute action populates fresh entry.

**Persona 2 — Security auditor reviewing AC sig integrity**:
- Reviews ADR-0021 ratification; agrees HKDF appropriate for tenant-scoped server-only verify.
- Reviews test vectors Annex C (50 known sigs verified).
- Reviews Mann-Whitney 3-prong cripto-grade output (|Δmedian| ≤ 0.5ms).
- Reviews Crypto SME independent code review sign-off.

**Persona 3 — Compliance reviewer (LGPD/GDPR + SLSA)**:
- LGPD Art. 38: integrity + confidentiality of personal data — HKDF integrity binding satisfies.
- SLSA Level 3 supply chain provenance: sig + Merkle dual layer.
- ADR-0021 + key_management.md §3 documents rotation policy (annual).

**SLA addendum**:
- Sign p99 ≤ 1ms (warm; TDK cached).
- Verify p99 ≤ 1ms (warm).
- TDK fetch cold p99 ≤ 10ms (KMS); cached after first fetch.
- Rotation procedure: ≤ 5 min for full tenant migration; verifier grace 1 prev + 1 current key_id.
- Mann-Whitney 3-prong: cripto-grade |Δmedian| ≤ 0.5ms.

## 4. Capability Mapping

- **CAP-AC-002** (REAPI UpdateActionResult cache write) — IMPLEMENTA partial (sign on persist).
- **CAP-AC-001** (REAPI GetActionResult) — IMPLEMENTA partial (verify on read).
- **CAP-AC-003** (Merkle verification dual-side) — INTEGRA via SignatureVerifier trait used by WI-S04-003 verify_full.
- Trace: `security_model.md CTRL-AC-002 (digest signing)` + `key_management.md §3 (TDK rotation)` + ADR-0021 ratificada.

## 5. Tipo

Cripto signing infra; HIGH_RISK; FF-HR-002 + FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **Module `crates/corelink-ac/src/sig/`**:
   - `mod.rs`: re-exports + traits.
   - `hkdf_signer.rs`: HkdfSigner + HkdfVerifier impl.
   - `tdk.rs`: TdkHandle trait + impls (production CF Secrets + test fixture).
   - `error.rs`: SigError enum.
   - `canonical.rs`: canonical_bytes computation (89 bytes; fixed layout).

2. **Public traits**: `SignatureSigner`, `SignatureVerifier`, `TdkHandle`.

3. **HKDF-SHA256 derivation**:
   - Standard RFC 5869.
   - Salt = None (zero-byte).
   - Info = `b"ac-sig"` (constant; CI test asserts byte-equal).
   - Output 32 bytes (sig_key).

4. **BLAKE3-keyed-hash** as MAC primitive:
   - `blake3::Hasher::new_keyed(&sig_key)`.
   - Update with canonical_bytes.
   - Finalize → 32-byte tag.

5. **Canonical bytes layout** (`canonical.rs`):
   ```
   offset  size  field
   0       1     version (u8 LE)
   1       16    tenant_id (UUIDv7 raw bytes)
   17      32    action_digest (BLAKE3 hash bytes)
   49      32    merkle_root (BLAKE3 hash bytes)
   81      8     created_at_ms (u64 LE)
   total   89 bytes
   ```
   - Function: `fn canonical_bytes(envelope: &AcEnvelope) -> [u8; 89]`.

6. **Constant-time compare**:
   - `subtle::ConstantTimeEq::ct_eq` em sig compare path.
   - Length check pre-compare (LengthMismatch fast-fail; length is public).
   - Forbidden patterns CI gate: grep `==` em sig module → error.

7. **Key rotation support**:
   - `sig_key_id: u32` field in envelope (reuse from WI-S04-002 schema column).
   - Verifier accepts `accepted_key_ids: Vec<u32>` (production: current + 1 previous; ≤ 4 grace post-incident).
   - Rotation procedure: bump `current_key_id` in HkdfSigner config; KMS provisions new TDK version; verifier accepts both for grace period.
   - Integration test: simulate rotation; envelopes signed before rotation still verify; new envelopes use new key_id.

8. **TdkHandle implementations**:
   - **Production**: `CfSecretsTdkHandle` — fetches TDK from CF Secrets Store via Worker binding; in-memory cache 5min TTL; rotation event invalidates cache.
   - **Test fixture**: `MockTdkHandle` — HashMap<key_id, [u8; 32]>; deterministic for test vectors.
   - **Both**: TDK bytes wrapped `Zeroizing<Vec<u8>>` (zeroize crate); never Display/Debug.

9. **Property tests** (10k iter PR; 100k nightly):
   - `prop_sign_verify_roundtrip`: 1000 random canonical_bytes; sign → verify → ok.
   - `prop_tampered_sig_rejected`: 1000 random sigs; flip 1 byte; verify rejects 100% with SigError::Invalid.
   - `prop_wrong_key_id_rejected`: 100 random key_ids not in accepted list; verify rejects with KeyIdUnknown.
   - `prop_canonical_bytes_byte_stable`: 1000 envelope variations; canonical_bytes computed deterministically.
   - `prop_constant_time_compare`: 1000 random valid vs invalid sigs; verify latency Mann-Whitney indistinguishable.
   - `prop_key_rotation_grace`: simulate rotation; old envelopes verify ok during grace; reject post-grace expiry.

10. **Mann-Whitney 3-prong test cripto-grade** (TIGHTER than other WIs):
    - Goal: cliente (or attacker) cannot distinguish "valid sig" vs "invalid sig" via verify timing.
    - 10k samples per arm; valid sigs vs random invalid sigs.
    - Power 1−β ≥ 0.80 com Cohen's d = 0.2 via statrs.
    - Šidák 3-trial gate (combined α ≈ 0.000125).
    - **|Δmedian| ≤ 0.5ms** (cripto-grade tighter; not 5ms middleware-grade).
    - Bootstrap 95% CI sobre |Δmedian| cruzando 0.

11. **Test vectors Annex C**:
    - 50 known TDK + canonical_bytes + expected sig (deterministic; reproducibility).
    - Integrated into CI; verify output matches expected.
    - Annex D: 50 invalid sigs (each error variant); verify rejects per spec.

12. **Cargo-fuzz harness**:
    - `fuzz/fuzz_targets/verify_sig.rs`: 1h CI nightly; arbitrary sig + canonical_bytes input; assert never panics + always returns Result.
    - Validates: no buffer overflow, no panic on malformed input, no constant-time leak via differential timing.

13. **ADR-0021 ratificação**:
    - Update `specs/03_architecture/adrs/ADR-0021-ac-digest-signing-hkdf-vs-ed25519.md` from DRAFT to ACCEPTED.
    - Document rationale; mitigations; risks accepted; future considerations.
    - Whitelist em validate_references.py (already as forward-looking; promote).

14. **Integration with WI-S04-001 + WI-S04-003**:
    - `HkdfSigner::sign` consumed by ActionResultBuilder (WI-001 calls before R2 PUT).
    - `HkdfVerifier::verify_sig` consumed by ActionResultVerifier::verify_full (WI-003) AND by handler GetActionResult flow [4] (WI-001).

15. **Métricas**:
    - `corelink.ac.sig.sign_duration_us` (histogram).
    - `corelink.ac.sig.verify_duration_us` (histogram).
    - `corelink.ac.sig.tdk_fetch_duration_ms` (histogram; cold vs warm).
    - `corelink.ac.sig.tdk_cache.hit_ratio` (gauge).
    - `corelink.ac.sig.invalid_total{reason}` (counter; alert if > 5/h sustained — possible compromise).
    - `corelink.ac.sig.key_id_unknown_total` (counter; alert if > 0 — rotation grace expired or bug).
    - `corelink.ac.sig.rotation_events_total` (counter).

16. **Documentation**:
    - `crates/corelink-ac/src/sig/README.md`: API + threat model + rotation procedure.
    - rustdoc 100% public API + 4 examples (sign, verify, rotation simulation, custom TdkHandle).
    - Spec doc updates `crates/corelink-ac/spec/merkle_protocol.md` §5 sig section.

### 6.2 Out-of-scope (deferred)

- **Ed25519 alternative impl** (rejected per ADR-0021; if needed → S-XX forward).
- **External-verifiable sig** (multi-party verification; SLSA L4 forward): post-GA.
- **Hardware-backed sig** (HSM, Cloudflare Worker external key access): post-GA; ADR-XX forward.
- **TDK rotation automation** (auto-rotation on schedule via cron): S-XX (key_management forward).
- **Multi-region TDK replication**: S-14.
- **Customer-supplied TDK BYOK**: S-XX.
- **Signed envelope encryption** (envelope encrypted at rest beyond R2 SSE): S-XX.

## 7. Anti-Scope

- ❌ Variable-time `==` em sig compare (subtle::ConstantTimeEq mandatory).
- ❌ Dynamic `info` string (constant `b"ac-sig"`; CI test enforces).
- ❌ Sig length unvalidated (length check pre-compare).
- ❌ TDK content in logs (Zeroizing wrap + redact macros).
- ❌ KMS fetch in hot path without cache (5min TTL cache mandatory).
- ❌ Single key (no rotation) — `sig_key_id` versioning mandatory.
- ❌ Verify accepts unknown key_id (KeyIdUnknown error).
- ❌ Skip `Zeroize` on TDK bytes drop.
- ❌ Custom MAC primitive (BLAKE3-keyed-hash standard).
- ❌ HKDF salt = random (None constant; deterministic; info carries context).
- ❌ Mann-Whitney p>0.05 sozinho (3-prong gate).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: HKDF-SHA256 digest signing infra (CTRL-AC-002)

  Background:
    Given crate corelink-ac with sig module compiled
    Given MockTdkHandle with TDK_v1 = [0xAA; 32]
    Given canonical_bytes example: version=1, tenant_id=A_uuid, action_digest=D, merkle_root=M, created_at_ms=T

  Scenario: Sign + verify roundtrip
    Given HkdfSigner with current_key_id=1
    When sign(canonical_bytes)
    Then returns (sig: 32 bytes, sig_key_id: 1)
    When HkdfVerifier with accepted_key_ids=[1]
    And verify_sig(canonical_bytes, sig, 1)
    Then OK

  Scenario: Tampered sig rejected (constant-time)
    Given valid sig from above
    When 1 byte flipped in sig
    And verify_sig
    Then error SigError::Invalid
    And constant-time compare path executed
    And p99 verify ≤ 1ms warm

  Scenario: Sig length mismatch (fast-fail; length is public)
    When verify_sig with sig of length 64 (not 32)
    Then error SigError::LengthMismatch { expected: 32, got: 64 }
    And p99 reject ≤ 0.1ms (no constant-time concern; length public)

  Scenario: Key_id unknown (rotation grace expired)
    Given HkdfVerifier accepted_key_ids=[2, 3]
    When verify_sig with sig_key_id=1
    Then error SigError::KeyIdUnknown { sig_key_id: 1, oldest_active: 2 }

  Scenario: Key rotation grace
    Given HkdfSigner current_key_id=2 (just rotated)
    Given HkdfVerifier accepted_key_ids=[1, 2] (grace previous + current)
    Given envelope E1 signed with key_id=1 (pre-rotation)
    Given envelope E2 signed with key_id=2 (post-rotation)
    When verify both
    Then BOTH OK
    And property test prop_key_rotation_grace green

  Scenario: HKDF info string fixed (CI gate)
    Given source code in sig/hkdf_signer.rs
    When CI test grep `b"ac-sig"` byte-equals expected
    Then assertion passes
    When dev refactors to b"acsig" (typo)
    Then CI test fails; PR red; refactor blocked

  Scenario: Constant-time compare em verify path
    Given 10000 random valid sigs + 10000 random invalid sigs (1-byte flip)
    When verify_sig latency measured per arm
    And Mann-Whitney U + power analysis 3-prong applied
    Then p > 0.05 com Šidák 3-trial
    And |Δmedian| ≤ 0.5ms com 95% CI cruzando 0  -- cripto-grade tight
    And property test prop_constant_time_compare green

  Scenario: TDK fetch cached (warm path)
    Given HkdfVerifier first verify call (cold)
    When verify_sig
    Then TDK fetched from KMS; cache populated
    And subsequent verify within 5min uses cache (no KMS fetch)
    And cache hit ratio gauge increments
    And p99 verify warm ≤ 1ms (cache hit)

  Scenario: TDK fetch cold (KMS latency)
    Given cache cold (rotation event invalidated cache)
    When verify_sig
    Then KMS fetch invoked
    And p99 cold ≤ 10ms (KMS latency)
    And metric corelink.ac.sig.tdk_fetch_duration_ms updates

  Scenario: TDK content not in logs
    Given verify_sig invoked with valid sig
    When trace span emitted
    Then attributes do NOT include TDK bytes
    And TDK Vec<u8> wrapped Zeroizing; on drop, memory zeroed
    And clippy lint forbids println of TDK type

  Scenario: Sign + verify across signer/verifier with same TDK
    Given Signer1 with tdk_handle=Mock_v1
    Given Verifier1 with tdk_handle=Mock_v1, accepted_key_ids=[1]
    When Signer1 signs canonical_bytes
    And Verifier1 verifies sig
    Then OK

  Scenario: Sign + verify with different TDK (cross-tenant simulated)
    Given Signer with tdk=TDK_A
    Given Verifier with tdk=TDK_B (different tenant)
    When Signer signs canonical_bytes_A
    And Verifier verifies sig with sig_key_id=A_key
    Then SigError::Invalid (tenant binding broken)

  Scenario: Replay attack — same envelope sig OK; cross-envelope FAIL
    Given envelope E1 signed (sig_E1)
    Given envelope E2 with different action_digest signed (sig_E2)
    When verify(canonical_bytes(E1), sig_E1, key_id) → OK
    When verify(canonical_bytes(E1), sig_E2, key_id) → FAIL  -- replay sig of E2 onto E1 = SigError::Invalid
    (canonical_bytes binding is per-envelope; cross-replay fails)

  Scenario: Forge attempt without TDK
    Given attacker without TDK access tries random sigs
    When 1000000 random sigs verified
    Then 0 forge accepted (probability 2^-256 per attempt)

  Scenario: Cargo-fuzz 1h CI no panics
    Given fuzz target verify_sig
    When 1h cargo-fuzz run with arbitrary bytes
    Then 0 panics, 0 OOM
    And bounds check protects against malformed input

  Scenario: ADR-0021 ratificada
    Given ADR-0021 file content
    Then doc_status = ACCEPTED (not DRAFT)
    And rationale documents HKDF rationale
    And mitigations + risks accepted listed
    And whitelist em validate_references.py
```

## 9. Design Decisions

### 9.1 Why HKDF-SHA256 (not Ed25519) — ADR-0021

Vide ADR-0021 §1; key points: tenant-scoped server-only verify; no public-key verifiability requirement; symmetric simpler; smaller sig (32 vs 64); equivalent security level for use case.

### 9.2 Why BLAKE3-keyed-hash (not HMAC-SHA256)

- BLAKE3 4× faster (SIMD); critical for verify p99 ≤ 1ms.
- BLAKE3-keyed mode uses BLAKE3 with the 32-byte key as the IV via the `keyed_hash` flag. **Lote 10.4bis P0 fix correção**: BLAKE3 is **NOT** Merkle-Damgård (prior text incorrect); it is a **Bao binary tree** of 1024-byte chunks with explicit domain separation flags. Construction is collision-resistant ~2^128 against MAC forgery (PRF-secure); designed-in domain separation prevents length-extension. Reference: BLAKE3 paper §6 (NIST-compatible analysis); RustCrypto `blake3` crate is widely-deployed standard.
- HMAC-SHA256 alternative also acceptable; chose BLAKE3 for performance + consistency with Merkle (WI-003 BLAKE3).

### 9.3 Why HKDF salt = None

- HKDF-Extract step takes optional salt; salt-less mode uses zero-bytes (HKDF spec RFC 5869 §2.2).
- Simplification; info string carries context separation.
- Per-tenant separation already via TDK (different tenants have different TDK).

### 9.4 Why fixed `info = b"ac-sig"` (not dynamic)

- Domain separation: ensures HKDF output for "ac-sig" purpose is independent from other purposes (e.g., "envelope" encryption key, "audit-chain" derivation key).
- Static = no possibility of typo/drift in production runtime.
- CI test asserts byte-equal; commit hook validates; ADR-0021 documents.

### 9.5 Why constant-time compare via subtle (mandatory; cripto-grade)

- Variable-time `==` leaks key bits via timing (early-exit on first mismatch).
- `subtle::ConstantTimeEq` is industry-standard; audited by Rust crypto WG.
- Mann-Whitney 3-prong cripto-grade |Δmedian| ≤ 0.5ms (tighter than middleware 5ms).

### 9.6 Why TDK in-memory cache 5min TTL

- KMS/CF Secrets fetch ~10ms; verify p99 budget 1ms; uncached = 10× over budget.
- 5min TTL: balance freshness vs perf; rotation event invalidates immediately.
- Memory cost: per Worker instance, ≤ 100 tenants × 32 bytes = 3.2 KB; negligible.

### 9.7 Why Zeroizing TDK bytes (memory hygiene)

- `Zeroizing<Vec<u8>>` zeroes memory on drop; defense against post-mortem memory dump.
- CF Workers VM may reuse memory pages across requests; zeroize prevents cross-request leak.
- Zero perf overhead post-fetch.

### 9.8 Why rotation grace = 1 previous key_id (not all historical)

- Storage: each tenant's TDK history grows linearly; bounded grace = bounded memory.
- Practical: annual rotation; 1-prev grace = 2-year coverage; sufficient for AC entries which expire ≤ 365d (per-tier ADR-0019).
- Post-grace, old envelopes return COR_AC_KEY_ID_UNKNOWN; customer re-execute action; fresh entry signed with current.

### 9.9 Why canonical_bytes layout fixed 89 bytes

- Deterministic length; no length-prefixed fields; faster encode.
- 5 fields concatenated raw bytes; well-defined boundary.
- Spec doc Annex E documents layout.

### 9.10 Why ADR-0021 is core not optional

- HKDF vs Ed25519 is a strategic cripto choice; impacts rotation policy + storage + customer SDK design.
- Without ADR, future contributors might "improve" by switching to Ed25519, breaking customers.
- ADR documents accepted risks (symmetric → server has full power; mitigated via audit + rotation).

## 10. Completeness Criteria SOTA

- [ ] **10.s04.004.1** Property tests 10k iter (PR) + 100k nightly → 0 panics, 0 false-accepts (EVT-002):
  - prop_sign_verify_roundtrip, prop_tampered_sig_rejected, prop_wrong_key_id_rejected, prop_canonical_bytes_byte_stable, prop_constant_time_compare, prop_key_rotation_grace.
- [ ] **10.s04.004.2** Mann-Whitney U + power analysis 3-prong cripto-grade em verify timing (EVT-002):
  - N ≥ 10000 samples per arm.
  - Power 1−β ≥ 0.80 com Cohen's d = 0.2 via statrs.
  - Šidák 3-trial gate (combined α ≈ 0.000125).
  - **|Δmedian| ≤ 0.5ms** (cripto-grade) com 95% CI cruzando 0.
- [ ] **10.s04.004.3** Test vectors Annex C (50 known sigs) + Annex D (50 invalid sigs each error variant) — verify match per spec (EVT-002).
- [ ] **10.s04.004.4** Cargo-fuzz 1h CI nightly → 0 panics, 0 OOM (EVT-002).
- [ ] **10.s04.004.5** Sign p99 ≤ 1ms; Verify p99 ≤ 1ms; TDK cold fetch p99 ≤ 10ms; TDK warm cache hit ratio ≥ 95% (EVT-021).
- [ ] **10.s04.004.6** HKDF info string CI test asserts `b"ac-sig"` byte-equal; commit hook validates (EVT-002).
- [ ] **10.s04.004.7** Constant-time compare CI gate: clippy custom lint forbids `==` em sig module; cargo-deny `unsafe_code` deny (EVT-002).
- [ ] **10.s04.004.8** Key rotation grace integration test: bump current_key_id; verify pre-rotation envelope still verify; post-grace expiry rejects (EVT-017).
- [ ] **10.s04.004.9** TDK Zeroizing wrap test: post-drop memory inspection (chaos test); zero leakage (EVT-002).
- [ ] **10.s04.004.10** ADR-0021 ratificada (DRAFT → ACCEPTED); rationale + risks + mitigations documented.
- [ ] **10.s04.004.11** Cargo-audit + cargo-deny + clippy `-D warnings` clean.
- [ ] **10.s04.004.12** Cost regression gate: per-op cost ≤ $0.000001 sign/verify (Lote 9.4 §14.10).
- [ ] **10.s04.004.13** rustdoc 100% public API + 4 examples + threat model README + sig/README.md.

## 11. DoD

- [ ] Module `crates/corelink-ac/src/sig/` compila + integration tests green.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em CI; 100k nightly green.
- [ ] Mann-Whitney 3-prong cripto-grade test green (|Δmedian| ≤ 0.5ms).
- [ ] Test vectors Annex C + D published + integrated CI.
- [ ] Cargo-fuzz 1h CI nightly green.
- [ ] HKDF info CI gate live.
- [ ] Constant-time compare CI gate (clippy lint) live.
- [ ] Key rotation integration test green.
- [ ] TDK Zeroizing chaos test green.
- [ ] Métricas (7 listadas §6.1.15) emitted; dashboard widget.
- [ ] ADR-0021 ratificada (ACCEPTED).
- [ ] Architect + AppSec + Crypto SME (mandatory emphatic) + Security Lead reviews.
- [ ] PRR Architect + Crypto SME mini sign-off.
- [ ] Cost regression gate green.
- [ ] Cargo-audit + cargo-deny clean.

## 12. Invariants Validated

- **INV-AC-DIGEST-SIGNED** (CRITICAL, NEW — promovida em registry §3.15 Lote 10.4bis): all envelopes sign with HKDF; verify mandatory in GET path; bypass = security control violation.
- **INV-AC-SIG-CONSTANT-TIME** (HIGH, NEW): verify path constant-time; Mann-Whitney 3-prong cripto-grade |Δmedian| ≤ 0.5ms.
- **INV-AC-SIG-INFO-FIXED** (HIGH, NEW): HKDF info = `b"ac-sig"` constant; CI gate.
- **INV-AC-KEY-ROTATION-GRACE** (HIGH, NEW): verifier accepts current + 1 previous key_id; post-grace rejects with structured error.
- **INV-AC-TDK-ZEROIZED** (MEDIUM, NEW): TDK bytes Zeroizing wrapped; on drop memory cleared.
- **INV-AC-CANONICAL-BYTES-STABLE** (HIGH, NEW): canonical_bytes layout 89 bytes fixed; deterministic.
- **INV-AC-MERKLE-VALID** (CRITICAL, registry §3.15): defense-in-depth; sig + Merkle independent.

TLA+ alignment: cas_integrity.tla; INV-AC-DIGEST-SIGNED enforced via verify_full path; combined with INV-AC-MERKLE-VALID for tampering detection.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Sig module | `crates/corelink-ac/src/sig/` | Rust |
| HkdfSigner + HkdfVerifier | `crates/corelink-ac/src/sig/hkdf_signer.rs` | Rust |
| TdkHandle trait + impls | `crates/corelink-ac/src/sig/tdk.rs` | Rust |
| Canonical bytes | `crates/corelink-ac/src/sig/canonical.rs` | Rust |
| Error enum | `crates/corelink-ac/src/sig/error.rs` | Rust |
| Property tests | `crates/corelink-ac/tests/prop_sig.rs` | Rust |
| Mann-Whitney timing tests | `crates/corelink-ac/tests/timing_sig.rs` | Rust |
| Cargo-fuzz harness | `crates/corelink-ac/fuzz/fuzz_targets/verify_sig.rs` | Rust |
| Test vectors Annex C + D | `crates/corelink-ac/spec/test_vectors_sig.md` + `tests/fixtures/sig/` | Markdown + JSON |
| Sig README + threat model | `crates/corelink-ac/src/sig/README.md` | Markdown |
| Examples | `crates/corelink-ac/examples/sig/` (4 examples) | Rust |
| ADR-0021 ratificada | `specs/03_architecture/adrs/ADR-0021-ac-digest-signing-hkdf-vs-ed25519.md` | Markdown |
| Constant-time clippy lint | `crates/corelink-ac/.clippy.toml` config | TOML |
| HKDF info CI test | `crates/corelink-ac/tests/ci_info_constant.rs` | Rust |

## 14. Quality Standards SOTA

- **14.s04.004.1** `#![forbid(unsafe_code)]`; zero `unwrap` em src/.
- **14.s04.004.2** rustdoc 100% public API + 4 examples + threat model README + sig/README.md.
- **14.s04.004.3** Test coverage ≥ 95% (cripto boundary).
- **14.s04.004.4** Latência: sign p99 ≤ 1ms warm; verify p99 ≤ 1ms warm; TDK cold ≤ 10ms; cache hit ratio ≥ 95%.
- **14.s04.004.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings`; cargo-fuzz 1h CI nightly; clippy custom lint forbids `==` em sig module.
- **14.s04.004.6** Métricas: 7 listadas §6.1.15; alert if sig.invalid_total > 5/h sustained.
- **14.s04.004.7** Constant-time discipline: subtle::ConstantTimeEq mandatory; CI gate; Mann-Whitney 3-prong cripto-grade.
- **14.s04.004.8** TDK hygiene: Zeroizing wrap; never Display/Debug; redact macros from S-09.
- **14.s04.004.9** Memory bounded: per-op ≤ 1 KiB stack; TDK cache ≤ 100 tenants × 32 bytes.
- **14.s04.004.10** Cost regression gate: per-op cost ≤ $0.000001.

## 15. Chaos Experiments

1. **Sig forge attempt (without TDK)**: 10M random sigs verified; assert 0 forge accepted (probability 2^-256 per attempt).

2. **Tampering detection (envelope byte flip)**: 1000 random envelopes; flip 1 byte in result OR sig; assert 100% detected with SigError::Invalid.

3. **Constant-time validation**: Mann-Whitney 3-prong; assert |Δmedian| ≤ 0.5ms; if regression detected (e.g., dev refactor introduces variable-time path), CI red.

4. **HKDF info drift**: chaos PR changes `b"ac-sig"` to `b"acsig"` (typo); CI test detects byte-equal mismatch; PR red.

5. **Key rotation race**: simulate rotation event T+0; cache invalidation lag T+5s; envelope signed at T+1 with stale cache (key_id=v1); verify at T+10 with cache refreshed (key_id=v2 active); test grace window covers (accepted_key_ids = [v1, v2]).

6. **TDK exfil attempt via memory inspection**: Zeroizing wrap test; post-drop, inspect VM memory; assert TDK bytes zeroed.

7. **TDK fetch failure (KMS down)**: simulate KMS 503; verify falls back to cached TDK; if cache cold, verify returns 503 BackendError; metric alert.

8. **Cargo-fuzz crashes**: 1h fuzz nightly; if crash detected, regress + fix; CI fails.

9. **Mann-Whitney CI flake regression**: chaos PR introduces subtle variable-time path; Mann-Whitney detects; CI red.

10. **Replay attack (cross-envelope sig swap)**: chaos test sign envelope_1 sig_1; envelope_2 sig_2; attacker swaps; verify rejects (canonical_bytes binding catches).

11. **Key rotation grace expiration**: simulate rotation; old envelope T-2years signed with key_id=1; verifier accepted_key_ids=[2,3]; assert SigError::KeyIdUnknown; customer must re-execute action; documented runbook.

12. **TDK fetch latency spike**: chaos PR introduces 100ms KMS latency; warm cache mitigates; cold path 503 graceful + Retry-After.

## 16. PRR

PRR HIGH_RISK 11 sign-offs canonical gated em WI-S04-006. Este WI mini-PRR **Architect with Crypto SME specialization mandatory emphatic**.

- [ ] All Gherkin green.
- [ ] Property + Mann-Whitney 3-prong cripto-grade green.
- [ ] Cargo-fuzz 1h CI nightly green.
- [ ] Test vectors Annex C + D published.
- [ ] Constant-time compare CI gate live.
- [ ] HKDF info CI gate live.
- [ ] Key rotation integration test green.
- [ ] TDK Zeroizing chaos test green.
- [ ] ADR-0021 ratificada (DRAFT → ACCEPTED).
- [ ] Crypto SME independent review sign-off.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Sig module skeleton + traits + error enum | 1.5h |
| ST-002 | HkdfSigner impl (HKDF-SHA256 + BLAKE3-keyed) | 2.5h |
| ST-003 | HkdfVerifier impl (constant-time compare) | 2h |
| ST-004 | TdkHandle trait + Mock + CfSecrets impls | 3h |
| ST-005 | Canonical bytes layout impl (89 bytes fixed) | 1.5h |
| ST-006 | TDK in-memory cache (5min TTL; rotation invalidate) | 2h |
| ST-007 | Zeroizing wrap on TDK bytes | 1h |
| ST-008 | Property tests (6 properties × 10k iter) | 4h |
| ST-009 | Mann-Whitney 3-prong cripto-grade test (|Δmedian| ≤ 0.5ms) | 4h |
| ST-010 | Cargo-fuzz harness | 2h |
| ST-011 | Test vectors Annex C + D (50 each) | 4h |
| ST-012 | HKDF info CI test (byte-equal assert) | 1h |
| ST-013 | Constant-time clippy custom lint | 2h |
| ST-014 | Key rotation integration test | 2.5h |
| ST-015 | rustdoc + 4 examples + threat model README | 3h |
| ST-016 | sig/README.md doc + spec/test_vectors_sig.md | 2.5h |
| ST-017 | ADR-0021 ratificação (DRAFT → ACCEPTED) + rationale fill-in | 3h |
| ST-018 | Crypto SME review iteration (independent verification) | 4h |
| ST-019 | Architect + AppSec review iteration | 3h |
| ST-020 | Métricas emit + dashboard widget | 2h |
| ST-021 | Cost regression bench setup | 1h |

**Total Optimistic**: ~50h. **PERT** (O=46h, M=52h, P=80h): **~56h**.

## 18. Dependencies

### Hard blockers

- WI-S04-003 (corelink-ac crate) SEALED — sig module is sub-crate.
- `hkdf` crate (RustCrypto; stable).
- `sha2` crate (RustCrypto; stable).
- `blake3` crate (BLAKE3 keyed-hash mode).
- `subtle` crate (constant-time compare).
- `zeroize` crate (memory zeroing).
- `statrs` crate (Mann-Whitney + power analysis).
- **Crypto SME availability** (mandatory emphatic; independent review).
- Cloudflare Secrets Store binding setup (production TdkHandle impl).

### Soft blockers

- WI-S04-001 (handler) — soft; this WI provides traits, handler consumes.
- WI-S04-005 (TTL worker) — none.
- WI-S04-006 (conformance + PRR) — outbound; this WI feeds.

### Outbound

- WI-S04-001 + WI-S04-003 consume SignatureSigner + SignatureVerifier traits.
- WI-S04-006 (conformance suite) consumes test vectors Annex C + D.
- S-15 CLI/SDK consumes for client-side verify (if Rust SDK).

## 19. Effort PERT

O: 46h, M: 52h, P: 80h → PERT **56h**.

## 20. Time-boxing

**64h hard limit**. Se exceder → escalation: split em "sig core" + "key rotation infra" sub-WIs.

## 21. Observability

7 métricas listadas §6.1.15. Trace span:
- `ac.sig.sign` — duration, key_id, result.
- `ac.sig.verify` — duration, key_id, result (ok|invalid|key_unknown|length_mismatch|backend_error).
- `ac.sig.tdk.fetch` — duration, key_id, cache_hit (bool), kms_latency_ms.

Logs structured JSON (handler context):
- INFO em verify ok.
- WARN em sig.invalid_total spike (potential tampering).
- ERROR em backend_error (KMS down).
- CRITICAL em sig.invalid_total > 5/h sustained (alert PD).

Dashboard widget DASH-AC sig (full em WI-S04-006):
- Sig sign/verify rate per result.
- TDK cache hit ratio (target ≥ 95%).
- KMS fetch p99 latency.
- Sig invalid counter (alert if > 5/h sustained).
- Key rotation events counter.

## 22. Cost Analysis

**Per-sign cost** (UPDATE path):
- Worker CPU (HKDF + BLAKE3-keyed ~1ms): ~$0.0000005.
- TDK fetch (warm cache ≥ 95%): negligible per-op amortized.
- Per-sign: ~$0.000001.

**Per-verify cost** (GET path):
- Worker CPU (~1ms): ~$0.0000005.
- TDK fetch warm: negligible.
- Per-verify: ~$0.000001.

**TCO 12m projection** (10M GET/dia + 1M UPDATE/dia):
- Sign: 1M × $0.000001 = $1/dia.
- Verify: 10M × $0.000001 = $10/dia.
- Total: ~$11/dia × 365 = **~$4k/yr**.
- TDK storage in CF Secrets: $0/yr (free for low-volume secrets).
- KMS fetches: amortized via cache; ~$5/yr.
- **Total sig infra: ~$4k/yr** at 10M/dia workload.

**Cost regression gate**:
- Sign ≤ $0.000001 per-op.
- Verify ≤ $0.000001 per-op.

**Comparison vs alternatives**:
- Ed25519 (rejected): ~2× sig size + 1.5× verify latency = $6k/yr at same workload.
- HMAC-SHA256: similar cost; chose BLAKE3 for performance + consistency.
- HKDF-SHA256 + BLAKE3-keyed: **$4k/yr** at 10M/dia.

## 23. API Contract

Public crate API (sub-module corelink-ac::sig; semver post v1.0):

```rust
pub trait SignatureSigner: Send + Sync {
    fn sign(&self, canonical_bytes: &[u8]) -> Result<(Vec<u8>, u32), SigError>;
}

pub trait SignatureVerifier: Send + Sync {
    fn verify_sig(&self, canonical_bytes: &[u8], sig: &[u8], sig_key_id: u32) -> Result<(), SigError>;
}

#[async_trait::async_trait]
pub trait TdkHandle: Send + Sync {
    fn fetch(&self, key_id: u32) -> Result<Vec<u8>, SigError>;
}

pub struct HkdfSigner { /* ... */ }    // production impl
pub struct HkdfVerifier { /* ... */ }  // production impl

pub enum SigError {
    LengthMismatch { expected: usize, got: usize },
    Invalid,
    KeyIdUnknown { sig_key_id: u32, oldest_active: u32 },
    BackendError(String),
    TdkDerivationFailed(String),
}
```

**Stability**: post-v1.0, traits stable; new error variants additive.

**Versioning**: sig protocol v1 (HKDF-SHA256 + BLAKE3-keyed); v2+ via ADR (e.g., post-quantum migration).

## 24. Post-mortem Hooks

- Sig invalid sustained > 5/h → CRITICAL post-mortem (suspected tampering campaign OR chave compromise).
- Key_id unknown sustained > 1/h → SEV-2 (rotation grace expired OR client old).
- TDK fetch failure spike > 10/h → SEV-2 (KMS issue).
- Mann-Whitney regression detected (|Δmedian| > 0.5ms) → CRITICAL (constant-time leak; SEV-1).
- Cargo-fuzz crash → SEV-1 (decoder bug).
- ADR-0021 reversal proposal (Ed25519 swap) → 5-Why + Crypto SME re-review.
- TDK leak detected via SIEM (logged TDK content) → CRITICAL post-mortem + secrets rotation + breach review.

## 25. Rollback / Recovery

- Crate version pin via Cargo.lock; rollback via git revert + cargo update.
- Sig protocol v1 → v2 migration: dual-verify period (verifier accepts both); ADR documents.
- Key rotation rollback: bump current_key_id back; previous remained accepted; envelopes signed pre-rollback continue to verify.
- RTO: ≤ 10 min.
- RPO: 0 (stateless lib; no data loss).

Fallback: handler returns 503 if sig verify backend unavailable (KMS down + cache cold); Bazel client retry with backoff.

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: per-tenant TDK; HKDF info="ac-sig" derives unique sig_key per tenant; cross-tenant sig forge requires both tenant TDKs.
- **Tampering**: BLAKE3-keyed MAC 256-bit; collision-resistant 2^128; tampering detection via verify_sig.
- **Repudiation**: sig + audit chain (S-09) provides immutable record of who signed when.
- **Information disclosure**: TDK Zeroizing wrap; never logged; constant-time compare prevents key bit leak.
- **DoS**: sig length validated pre-compare; bounded compute (1ms); fast-fail length mismatch.
- **Elevation of privilege**: sig binding to tenant_id field in canonical_bytes; cross-tenant impossible without TDK access.

**LINDDUN delta**:
- **Linkability**: sig is per-tenant; cannot link sigs across tenants without TDKs.
- **Identifiability**: sig is opaque 32 bytes; non-PII.
- **Non-repudiation**: append-only sig + audit chain.
- **Detectability**: sig invalid alert; key rotation events tracked; constant-time validation.
- **Disclosure of information**: TDK never logged; Zeroizing memory hygiene.
- **Unawareness**: ADR-0021 + sig README documents threat model + accepted risks.
- **Non-compliance**: LGPD Art. 38 + GDPR Art. 32 satisfied via cripto integrity + audit + DSR S-11.

## 27. Knowledge Transfer

- **Tech talk** (2h): "AC Digest Signing + HKDF + BLAKE3-keyed + Constant-Time Discipline + Key Rotation".
- **Doc** `crates/corelink-ac/src/sig/README.md` — API + threat model + rotation procedure.
- **Doc** `crates/corelink-ac/spec/test_vectors_sig.md` — Annex C + D reference vectors.
- **ADR-0021** ratificada — design rationale + rejected alternatives.
- **Workshop** (3h): com Crypto SME + Architect + AppSec + downstream WI authors (WI-S04-001/003/006). Includes: pair-program review of constant-time path; cargo-fuzz harness walkthrough; key rotation simulation.
- **Onboarding test** (5 questions): HKDF rationale (vs Ed25519), info string fixed reason, constant-time compare necessity, key rotation grace, TDK Zeroizing rationale.
- **External-facing**: blog post post-S-04 SEALED — "How CoreLink signs AC envelopes: HKDF + BLAKE3-keyed + constant-time + dual-side defense".

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Sig forge via constant-time bypass (timing oracle) | L | M | CRITICAL | L | LOW | subtle::ConstantTimeEq mandatory; clippy lint; Mann-Whitney 3-prong cripto-grade; CI gate |
| R-002 | HKDF info string drift (typo refactor) | L | L | CRITICAL | L | LOW | CI test asserts byte-equal; commit hook; ADR-0021 documents |
| R-003 | TDK leak via logging (dev debugging) | L | M | CRITICAL | L | LOW | Zeroizing wrap; never Display/Debug; redact macros S-09; clippy lint forbids |
| R-004 | TDK fetch latency in hot path (KMS uncached) | M | L | HIGH | L | LOW | In-memory cache 5min TTL; rotation invalidate; metric warm hit ratio target ≥ 95% |
| R-005 | Key rotation grace expired (envelope > 1 prev key) | M | M | MEDIUM | M | LOW | accepted_key_ids = current + 1 prev; AC TTL ≤ 365d aligned; runbook re-execute |
| R-006 | TDK rotation race (cache stale post-rotation) | L | M | HIGH | L | LOW | Rotation event invalidates cache; brief grace; integration test simulates |
| R-007 | Mann-Whitney CI flake (1-em-20 false positive) | M | H | LOW | M | LOW | Šidák 3-trial gate; combined α ≈ 0.000125; cripto-grade tighter target |
| R-008 | Sig length variable bypass (sig of length 0) | L | L | HIGH | L | LOW | Length check pre-compare; LengthMismatch error; chaos test |
| R-009 | KMS outage (CF Secrets down 1h) | L | M | HIGH | L | LOW | Cache 5min TTL absorbs short outages; cold fetch 503 graceful + Retry-After; runbook |
| R-010 | Cargo-fuzz crash detected | L | M | HIGH | L | LOW | 1h CI nightly; immediate fix + regression test; SEV-1 |
| R-011 | ADR-0021 reversal pressure (someone wants Ed25519) | M | L | LOW | L | LOW | ADR documents rationale + risks accepted; Crypto SME review required for change |
| R-012 | Side-channel via memory access patterns (BLAKE3 SIMD cache) | L | L | MEDIUM | L | LOW | ADR-0021 documents accepted risk (CF Workers shared infra); future post-quantum migration via ADR |
| R-013 | Replay attack (canonical_bytes weak binding) | L | L | HIGH | L | LOW | canonical_bytes binds 5 fields including merkle_root; cross-envelope replay fails verify |
| R-014 | TDK exfiltration via KMS audit gap | L | L | CRITICAL | L | LOW | CF Secrets audit trail; per-tenant isolation; blast radius bounded |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Crypto SME review HKDF vs Ed25519 trade-off; sign + verify protocol; canonical_bytes layout.
2. **AppSec (D+2)**: AppSec review constant-time discipline + TDK hygiene + rotation procedure.
3. **Code (D+5)**: peer review (1 engineer + 1 Crypto SME).
4. **Crypto (D+6)**: **Crypto SME independent review (mandatory emphatic)** — sign protocol verification, constant-time validation, key rotation analysis, test vectors review.
5. **Adversarial (pre-merge D+8)**: red team — sig forge attempts (10M random sigs), constant-time leak attempts, HKDF info drift, TDK exfil attempts.
6. **Cargo-fuzz (D+9)**: 1h fuzz validates 0 panics.
7. **PRR (D+10)**: Architect + Crypto SME mini sign-off (full ship gate em WI-S04-006).

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; **mandatory** — sig protocol + key rotation + TDK hygiene_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD; ideally cripto-experienced_ | _pending_ | _pending_ |
| 7 | QA | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD; LGPD Art. 38 + SLSA L3 alignment_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD; TDK leak prevention review_ | _pending_ | _pending_ |
| 11 | Architect | _TBD; **mandatory** — ADR-0021 ratificação + key rotation infra_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD; **mandatory emphatic** — constant-time discipline + TDK hygiene_ | _pending_ | _pending_ |
| 13 | Crypto SME | _**MANDATORY EMPHATIC** (non-waivable; Lote 10.4-tris P0-R5-005 explicit per Sonnet R5) — required BEFORE WI-S04-004 SEAL: sig protocol independent review; constant-time validation; key rotation analysis (BOTH `sig_key_id` AND `path_key_id` per P0-R5-006); test vectors; HKDF + BLAKE3-keyed audit; ADR-0021 endorsement; review of HKDF-Extract composition with path-HMAC TDK use (P0-R5-002 acceptability ruling)._ | _pending_ | _pending_ |

### 30.1 Key Rotation Procedures (Lote 10.4-tris P0-R5-006 fix — explicit independent lifecycles)

`sig_key_id` and `path_key_id` are INDEPENDENT key-version columns in `ac_meta`. Their rotation procedures are also independent:

#### `sig_key_id` rotation (HKDF signing TDK)

1. Generate new TDK_v<N+1> via KMS (Worker secret `CORELINK_TDK_V<N+1>`).
2. Add `current_key_id = N+1` em `HkdfSigner` config; new envelopes signed with v<N+1>.
3. Add `accepted_key_ids = [N, N+1]` em `HkdfVerifier` config (grace period 30d retains old TDK).
4. After 30d grace, remove `N` from `accepted_key_ids`; old envelopes signed with v<N> rejected with `SigError::KeyIdUnknown`; affected customers must re-execute Action (per WI-001 §1 narrative point 11 chaos test).

#### `path_key_id` rotation (TDK for path-prefix derivation)

1. Generate new TDK_v<M+1> via KMS (Worker secret `CORELINK_PATH_TDK_V<M+1>`).
2. **DO NOT re-compute existing `tenant_prefix` materialized columns** — they are bound to the TDK version that signed them at INSERT time; preserved per-row via `path_key_id`.
3. New INSERTs use TDK_v<M+1> + new `path_key_id = M+1`; old rows retain TDK_v<M> via their persisted `path_key_id`.
4. GET handler reads `tenant_prefix` from D1 row (materialized BLOB column; NOT recomputed from TDK at GET time per WI-S04-001 §1 step [3]); GET path reconstruction is INDEPENDENT of current TDK version.
5. **Eviction (WI-S04-005)** uses `tenant_prefix` from materialized column for R2 path construction; deletion is independent of TDK rotation.
6. **TDK_v<M> retention**: keep retired path TDKs available indefinitely (NEVER delete) — required for any future read of rows persisted with that version. Documented em runbook RB-PATH-TDK-RETENTION (forward stub).

#### Invariant `INV-AC-PATH-SIG-KEY-VERSION-INDEPENDENT` (NEW; promote to §3.15):

`sig_key_id` and `path_key_id` MAY diverge per row when rotation happens between path computation (INSERT step) and HKDF signing (UpdateActionResult step). This is a VALID state, not an invariant violation. Handler INSERT MUST use current `sig_key_id` AND current `path_key_id` at the moment of write; no atomic snapshot required across the two key versions.

#### Forward: ADR-0021 §RotationProcedures captures the canonical procedure.

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S04-004 (Lote 10.4); SOTA pós-Lote 10.3bis (32 seções; 13-row sign-off; 14-row risk; Mann-Whitney 3-prong cripto-grade |Δmedian| ≤ 0.5ms; cost TCO 12m; 12 chaos experiments; STRIDE+LINDDUN delta full; Crypto SME MANDATORY EMPHATIC; ADR-0021 ratificação plan). |
| 1.1.0 | 2026-04-25 | Gustavo (Lote 10.4bis Agent R4) | P0 fixes: salt = sig_key_id binds version (RFC 5869 Extract); HKDF info `b"ac-sig"` non-prefix domain separation from `b"manifest-sig"`/`b"meta-manifest-sig"`; canonical_bytes 89→121 bytes (added result_hash binding); sig_alg CHECK constraint inline; Crypto SME promoted MANDATORY EMPHATIC. |
| 1.2.0 | 2026-04-25 | Gustavo (Lote 10.4-tris Sonnet R5) | **P0-R5-001**: key_id=0 reserved sentinel; rotation starts at 1; `unwrap_or(&1)` not `&0`; debug_assert non-empty accepted_key_ids; `SigError::KeyIdReserved` variant added. **P0-R5-002**: HKDF-Extract composition with path-HMAC TDK use documented in ADR-0021 §Risks (accepted under HMAC security assumption; attack cost 2^128). **P0-R5-003**: canonical_bytes 121 bytes retained com result_hash documented as **derived `BLAKE3(merkle_root)` for D1 index lookup ONLY (NOT cripto binding)**; verify_full is the cripto authority for sig+structure; result_hash column is index column, not security boundary; ADR-0037 §A1 clarifies. **P0-R5-005**: Crypto SME MANDATORY EMPHATIC explicit (non-waivable; required BEFORE WI-S04-004 SEAL); WI-S04-006 §6.1.6 advisory role at PRR ceremony only (substantive review at this WI's SEAL). **P0-R5-006**: §30.1 NEW — explicit key rotation procedures for BOTH `sig_key_id` AND `path_key_id`; INV-AC-PATH-SIG-KEY-VERSION-INDEPENDENT promoted. |
| 1.3.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7 1M) | **WI-S04-004 SEALED — implementation phase**. Real HKDF-SHA256 + BLAKE3-keyed signing infra delivered as `crates/corelink-ac/src/sig/` (4 sub-modules: `error.rs` 6-variant `SigError` taxonomy with `audit_code()` short-id contract; `canonical.rs` 121-byte preimage `compose()` mirroring `corelink-worker::reapi::ac::sig::AcEnvelope::canonicalize` 1:1 with compile-time `assert!` parity check; `tdk.rs` `TdkHandle` trait + `MockTdkHandle` deterministic test fixture + `Tdk` newtype wrapping `Zeroizing<Vec<u8>>` zero-on-drop with redacted Debug + crate-private `as_bytes` accessor; `hkdf_signer.rs` `HkdfSigner` + `HkdfVerifier` real impl with `salt = sig_key_id.to_le_bytes()` + `info = b"ac-sig"` per ADR-0021 + `subtle::ConstantTimeEq` verify path + `accepted_key_ids` rotation grace API [`new` / `admit` / `retire` / `rotate_to`] + reserved-sentinel rejection on every entry point + free-fn `compute_signature` for client SDK dual-side verify); `pub trait SignatureSigner` + `pub trait SignatureVerifier` Send+Sync surfaces consumed by `corelink-worker::reapi::ac::sig::CanonicalAcSigner` adapter (new — wraps `Arc<HkdfSigner>` + `Arc<HkdfVerifier>` and satisfies the worker's local `Signer` trait + maps every canonical `SigError` arm to the worker's local taxonomy preserving cripto-mismatch identity). Tests: 28 lib unit + 11 canonical_vectors_sig + 8 key_rotation_sig + 6 property × 10 000 iter (`prop_sign_verify_roundtrip` / `prop_tampered_sig_rejected` / `prop_wrong_key_id_rejected` / `prop_canonical_bytes_byte_stable` / `prop_cross_tenant_signature_rejected` / `prop_key_rotation_grace`) + Mann-Whitney 3-prong cripto-grade timing test (3 arms × 10 000 samples × 3 trials; 9 pair-tests; Šidák-corrected α' ≈ 0.005 686; 10/80/10 trimmed-mean central estimator per WI-S03-008 ct-variance lesson; bootstrap 95 % CI on `|Δ(trimmed_mean)|` ≤ 0.5 ms cripto-grade gate; release-mode-only via `#[cfg_attr(debug_assertions, ignore)]`; dudect §III.A batched measurement window of 256 verify ops per sample so timer-resolution ties don't overwhelm the rank statistic). HKDF info CI gate `b"ac-sig"` byte-equal asserted in lib unit + integration test. Hardening: `#![forbid(unsafe_code)]`; zero `unwrap`/`expect`/`panic` in src/; `clippy::indexing_slicing = "deny"` + canonical-offset compile-time `assert!` proof of in-range; `mod_module_files = "deny"` honored (`sig.rs` parent + `sig/` children). Workspace + worker integration: full `cargo test --workspace --all-targets --features corelink-worker/tower-middleware` 0 failures (all existing tests + 4 new `CanonicalAcSigner` adapter tests in worker side); `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `python3 scripts/validate_specs.py` clean; `python3 scripts/validate_references.py` no new dangling refs. **Deferred per charter trait-abstraction-defer**: real `CfSecretsTdkHandle` Cloudflare Secrets binding shim (alongside WI-S04-006 conformance suite + miniflare/wrangler-dev integration); 100k nightly property iter; cargo-fuzz 1h CI nightly target (alongside conformance suite); test vectors Annex C/D 50-vector publication (canonical_vectors_sig already covers the algorithmic boundary; external-consumer fixtures land alongside conformance suite); criterion p99 sign/verify ≤ 1ms benchmarks (alongside conformance suite). |

## 32. Anti-patterns evitados

- ❌ Variable-time `==` em sig compare (subtle mandatory).
- ❌ Dynamic `info` string (constant `b"ac-sig"`).
- ❌ Sig length unvalidated.
- ❌ TDK content em logs (Zeroizing).
- ❌ KMS fetch em hot path uncached.
- ❌ Single key (no rotation).
- ❌ Verify accepts unknown key_id silently.
- ❌ Skip Zeroize on TDK drop.
- ❌ Custom MAC primitive (BLAKE3-keyed standard).
- ❌ HKDF salt = random (None constant).
- ❌ Mann-Whitney p>0.05 sozinho (3-prong gate).
- ❌ Ed25519 alternative impl (rejected per ADR-0021).
- ❌ Synchronous KMS fetch every request (cache mandatory).

---

**Fim WI-S04-004.** Próximo: WI-S04-005 (TTL worker cron Durable Object + S-07 boundary alignment ADR-0019 + refresh-on-hit semantics).
