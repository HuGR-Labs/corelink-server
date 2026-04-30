# corelink-pat

Personal Access Token (PAT) primitives for CoreLink.

Implements WI-S03-002 (HIGH_RISK auth boundary; FF-HR-002 cross-tenant
token forge, FF-HR-005 crypto boundary, FF-HR-009 5-layer defence
Layer 1).

## Canonical PAT format

Per `auth_model.md §2.2` cycle 9 SEAL decision (a) hybrid HMAC +
Argon2id:

```text
corelink_<env>_<token_id>.<random_secret>.<hmac_sig>
```

| Segment | Length | Charset | Notes |
|---|---|---|---|
| `corelink_` | 9 | ASCII literal | constant-time scanned in full |
| `<env>` | 2-3 | `pat \| ci \| ro` (exhaustive) | strict allowlist |
| `<token_id>` | 16 | Crockford base32 | indexed lookup key |
| `<random_secret>` | 43 | base64url no-pad | 32 bytes random (256-bit) |
| `<hmac_sig>` | 22 | base64url no-pad | first 16 bytes of HMAC-SHA256 (128-bit truncated) |

## Verify pipeline

1. **`parse_plaintext`** decomposes the bytes into canonical segments. Constant-time across all valid envs.
2. **`verify_hmac_sig`** rejects unsigned random spam in ≤100µs (DDoS + phishing defence).
3. (Caller looks up the row by `token_id` — DB primitive lives in WI-S03-005.)
4. **`verify_argon2id`** confirms cryptographic possession via OWASP-2024-floor Argon2id PHC verify.

On the cold path (parse fail, sig mismatch, or `token_id` absent in
DB), the middleware MUST invoke `dummy_verify_for_constant_time` so the
wire response latency envelope is identical to the warm path.

## Threat model summary

| Threat | Mitigation |
|---|---|
| Offline crack after DB exfil | Argon2id `m=64MiB / t=3 / p=4` (OWASP 2024 floor) |
| Online brute-force | Rate limit (S-08); 256-bit random_secret keyspace |
| Timing oracle on verify | `subtle::ConstantTimeEq` + `dummy_verify_for_constant_time` |
| Salt reuse rainbow tables | Per-token random salt via OS CSPRNG |
| PAT format ambiguity | Strict `pat \| ci \| ro` allowlist in env parser |
| Scope spoofing via prefix manipulation | DB `pat.scopes` column is SoT (this crate never infers from string) |
| Plaintext leakage in logs | `PatPlaintext` newtype: no Display/Serialize/Debug; Drop scrubs |
| HMAC sig forge | 128-bit truncated MAC + per-region 24h-rotated signing key |

## API surface

```rust
use corelink_pat::{
    mint, verify_with_hash, PatEnv, PatScopes, PatSigningKey,
    PrincipalId, TenantId, SCOPE_CACHE_RW,
};
use std::time::Duration;
use uuid::Uuid;

let signing_key = PatSigningKey::from_bytes(vec![0x42u8; 32])?;
let (plaintext, pat) = mint(
    PatEnv::Pat,
    TenantId(Uuid::new_v4()),
    PrincipalId(Uuid::new_v4()),
    PatScopes::from_u64(SCOPE_CACHE_RW),
    Some(Duration::from_secs(86_400 * 90)),
    &signing_key,
    1, // signing_key_id (rotation generation)
)?;
let plaintext_string = plaintext.into_string();
let verified = verify_with_hash(&plaintext_string, &pat.token_id, &pat.hash, &signing_key)?;
```

## WASM compatibility note

The `argon2` crate compiles to `wasm32-unknown-unknown`, but Argon2id
execution under WASM exceeds typical CF Worker CPU budgets at
OWASP-2024 cost parameters; the deployed validation path lives in the
host-server runtime. CI defers the wasm32 check for this crate
(matching the precedent set by WI-S03-001 for `ring`).

## Test coverage

- `tests/canonical_vectors.rs` — deterministic regression vectors (format, parse, verify, sig).
- `tests/prop_pat.rs` — property tests at 10k iter (verify-rejects-tampered, cross-key isolation, parser-no-panic, scope bitset roundtrip).
- `tests/adversarial.rs` — CVE-class regressions (algorithm downgrade, `m_cost` floor, `t_cost` floor, sig forge, salt-per-token uniqueness, signing-key length floor, plaintext redaction, dummy-pad invariant).
- `tests/constant_time.rs` — release-only median-variance gate (≤ 5% on `parse_env`; sanity ceiling on `parse_plaintext` malformity probes).

## Crate-wide invariants

- `PatPlaintext`: no `Display`, no `Serialize`, no public `Debug` revealing bytes; Drop scrubs.
- `PatHash`: PHC string; safe to log; embeds salt.
- `PatSigningKey`: redacts in `Debug`; zeroizes on drop; rejects keys < 32 bytes.
- `PatScopes` u64 bitset: hot-path bitwise check; reserved bits dropped on import.
