//! `corelink-pat` — Personal Access Token (PAT) primitives.
//!
//! Implements WI-S03-002 (HIGH_RISK auth boundary; FF-HR-002 cross-tenant
//! token forge, FF-HR-005 crypto boundary, FF-HR-009 5-layer defence
//! Layer 1). The crate owns the canonical PAT plaintext format, the
//! HMAC-SHA256 fast-fail signature layer, and the Argon2id PHC-string
//! mint/verify primitives. The Tower middleware (WI-S03-003) consumes
//! these primitives behind the request-context boundary; the Neon
//! `pat` schema (WI-S03-005) persists the rows produced by [`mint()`].
//!
//! # Canonical PAT format
//!
//! Per `auth_model.md §2.2` cycle 9 SEAL decision (a) hybrid HMAC +
//! Argon2id:
//!
//! ```text
//! corelink_<env>_<token_id>.<random_secret>.<hmac_sig>
//! ```
//!
//! - `<env>` ∈ `pat | ci | ro` (strict allowlist).
//! - `<token_id>` = 16-char Crockford base32 indexed lookup key.
//! - `<random_secret>` = 43-char base64url-no-pad (32 random bytes).
//! - `<hmac_sig>` = 22-char base64url-no-pad
//!   (`HMAC-SHA256(pat_signing_key, token_id || "." || random_secret)`
//!   truncated to 16 bytes / 128 bits).
//!
//! # Verify pipeline
//!
//! 1. `parse_plaintext` decomposes the bytes into the canonical
//!    segments. Constant-time across all valid envs.
//! 2. `verify_hmac_sig` rejects unsigned random spam in ≤100µs
//!    (DDoS + phishing defence; cycle 9 SEAL decision (a) layer).
//! 3. (Caller looks up the row by `token_id` — DB primitive lives
//!    in WI-S03-005.)
//! 4. `verify_argon2id` confirms cryptographic possession of the
//!    `random_secret` via OWASP-2024-floor Argon2id PHC verify.
//!
//! On the cold path (parse fail, sig mismatch, or `token_id`
//! absent in DB), the middleware MUST invoke
//! [`dummy_verify_for_constant_time`] so the wire response latency
//! envelope is identical to the warm path. Without that pad, an
//! attacker can use end-to-end response time as an existence
//! oracle.
//!
//! # Crate-wide invariants
//!
//! - **`PatPlaintext` newtype**: no `Display`, no `Serialize`, no
//!   public `Debug` revealing bytes. Single legal consumer is
//!   [`PatPlaintext::into_string`] inside the mint API response
//!   handler. Drop scrubs the buffer.
//! - **`PatHash` is a PHC string**: safe to log; embeds salt.
//! - **`PatSigningKey`**: redacts in `Debug`; zeroizes on drop;
//!   rejects keys < 32 bytes.
//! - **`PatScopes` u64 bitset**: hot-path bitwise check; reserved
//!   bits dropped on import (forward-compat safe).
//!
//! # Quickstart
//!
//! ```rust
//! use corelink_pat::{
//!     mint, verify_with_hash, PatEnv, PatScopes, PatSigningKey,
//!     PrincipalId, TenantId, SCOPE_CACHE_RW,
//! };
//! use std::time::Duration;
//! use uuid::Uuid;
//!
//! # fn ex() -> Result<(), Box<dyn std::error::Error>> {
//! let signing_key = PatSigningKey::from_bytes(vec![0x42u8; 32])?;
//! let (plaintext, pat) = mint(
//!     PatEnv::Pat,
//!     TenantId(Uuid::nil()),
//!     PrincipalId(Uuid::nil()),
//!     PatScopes::from_u64(SCOPE_CACHE_RW),
//!     Some(Duration::from_secs(86_400)),
//!     &signing_key,
//!     1, // signing_key_id
//! )?;
//! let pt_string = plaintext.into_string();
//! let verified = verify_with_hash(&pt_string, &pat.token_id, &pat.hash, &signing_key)?;
//! assert_eq!(verified.env, PatEnv::Pat);
//! # Ok(()) }
//! ```
//!
//! # WASM / Cloudflare Workers compatibility note
//!
//! The `argon2` crate compiles to `wasm32-unknown-unknown` but
//! Argon2id execution under WASM exceeds typical CF Worker CPU
//! budgets at OWASP-2024 cost parameters; the deployed validation
//! path lives in the host-server runtime. CI defers the wasm32
//! check for this crate (matching the precedent set by
//! WI-S03-001 which deferred wasm32 for `ring`); the unit tests
//! exercise both the parser and the Argon2id path natively.

#![forbid(unsafe_code)]

pub mod argon;
pub mod error;
pub mod format;
pub mod mint;
pub mod scopes;
pub mod sig;
pub mod types;
pub mod verify;

pub use argon::{
    dummy_verify_for_constant_time, hash_random_secret, verify_argon2id, ARGON2_M_COST_KIB,
    ARGON2_OUTPUT_LEN, ARGON2_P_COST, ARGON2_T_COST,
};
pub use error::PatError;
pub use format::{
    parse_env, parse_plaintext, PatPlaintextParts, PAT_HMAC_SIG_LEN, PAT_HMAC_SIG_RAW_LEN,
    PAT_PREFIX_LEN, PAT_RANDOM_SECRET_LEN, PAT_RANDOM_SECRET_RAW_LEN,
};
pub use mint::mint;
pub use scopes::{
    PatScopes, SCOPE_ADMIN_AUDIT, SCOPE_ADMIN_BILLING, SCOPE_ADMIN_TENANT_R,
    SCOPE_ADMIN_TENANT_W, SCOPE_ADMIN_TOKENS, SCOPE_ADMIN_USERS, SCOPE_CACHE_DELETE,
    SCOPE_CACHE_FIND, SCOPE_CACHE_R, SCOPE_CACHE_RW, SCOPE_CACHE_W, SCOPE_EXECUTE_ACTION,
    SCOPE_KNOWN_MASK, SCOPE_REPORT_RESULT,
};
pub use sig::{compute_hmac_sig, verify_hmac_sig};
pub use types::{
    Pat, PatEnv, PatHash, PatId, PatPlaintext, PatSigningKey, PatTokenId, PrincipalId, TenantId,
    PAT_TOKEN_ID_LEN,
};
pub use verify::{verify_hmac_only, verify_with_hash, VerifiedPat};
