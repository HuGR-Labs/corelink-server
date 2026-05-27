//! CoreLink Clerk SSO adapter — JWT RS256 validation + JWKS cache.
//!
//! Implements WI-S03-001 (HIGH_RISK lane; FF-HR-002 cross-tenant via JWT
//! spoof, FF-HR-005 JWT validation as crypto boundary, FF-HR-009 layered
//! defense). The adapter validates JWTs emitted by Clerk SSO (RS256
//! signing key published in JWKS), extracts canonical claims, and
//! returns a [`ClerkPrincipal`] suitable for downstream consumption by
//! WI-S03-003 Tower middleware.
//!
//! # Architectural seam
//!
//! Two trait abstractions decouple the validate path from the
//! runtime concerns this crate intentionally does NOT depend on
//! directly. Two production fetcher implementations coexist on the
//! same [`JwksFetcher`] trait; the outer crate picks one based on
//! target:
//!
//! | Target | Fetcher | Crate / feature |
//! |---|---|---|
//! | Native (`apps/server`, CLI, CI) | `HttpJwksFetcher` (`reqwest` + `rustls-tls`) | `corelink-clerk` feature `http-fetcher` |
//! | Cloudflare Workers (`wasm32-unknown-unknown`) | `CfJwksFetcher` (`worker::Fetch::Url`) | `corelink-clerk-cf` |
//!
//! - [`JwksFetcher`]: HTTPS JWKS endpoint fetch. Tests use
//!   [`fakes::StaticJwksFetcher`] (in-memory; no network) +
//!   [`fakes::ScriptedJwksFetcher`] (rotation simulation).
//! - [`KvJwksCache`]: KV TTL-bound cache (CF KV in production; in-memory
//!   `BTreeMap` for tests via [`fakes::InMemoryKvCache`]). Key
//!   `clerk:jwks:<instance_hash>`. TTL canonical 86_400 seconds (24h)
//!   per WI §1 / §9.2.
//!
//! # Environment-driven configuration (R2-2)
//!
//! [`ClerkConfig::from_env`] reads the canonical env-var matrix
//! (`CLERK_PUBLISHABLE_KEY`, `CLERK_SECRET_KEY`, optional
//! `CLERK_JWKS_URL` / `CLERK_JWT_ISSUER`, required `CLERK_AUDIENCE`)
//! and derives the JWKS URL + issuer allowlist from the publishable
//! key's frontend-API host when overrides are unset. The secret key
//! is surfaced separately via [`ClerkEnvSecrets`] so the
//! `Debug`-printable [`ClerkConfig`] never carries it.
//!
//! # admin-ui middleware wiring
//!
//! The `apps/admin-ui` TypeScript middleware calls the CF Worker
//! binding which in turn wires [`ClerkAdapter`] with
//! `corelink-clerk-cf::CfJwksFetcher` + `CfKvJwksCache`. The wire
//! contract is:
//! - middleware extracts JWT from `Cookie: __session=…` (or
//!   `Authorization: Bearer …`),
//! - calls `validate_session(jwt, config, fetcher, cache)`,
//! - on `Ok(principal)` injects `x-corelink-user-id` + `x-corelink-org-id`
//!   headers into the downstream request,
//! - on `Err(_)` returns a 401 `invalid_token` per WI-S03-003 §23.
//!
//! # Validate path
//!
//! 1. Decode header (no signature verification yet) → extract `kid`.
//! 2. KV cache lookup; miss/stale → JWKS fetch → cache populate.
//! 3. Find key by `kid`; if absent **single** lazy-refresh + retry.
//! 4. RS256 verification via `jsonwebtoken` 9.x with explicit
//!    `Validation::new(Algorithm::RS256)` (alg=none and HS-confusion
//!    rejected at the decoder boundary; WI §9.1).
//! 5. Validate `iss ∈ issuer_allowlist` (exact match) and `aud ==
//!    audience` (exact match).
//! 6. Extract claims into [`ClerkPrincipal`].
//!
//! Constant-time primitives ([`subtle::ConstantTimeEq`]) gate any
//! string compare that touches secret material — the validator
//! delegates signature compare to the audited `jsonwebtoken` crate but
//! still surfaces a CT-safe `kid` compare and a CT-safe issuer
//! membership check.
//!
//! # Anti-scope (WI §7)
//!
//! - No HS256, no `alg=none`, no `alg` allowlist relaxation.
//! - No JWKS fetch over plain HTTP; HTTPS only.
//! - No clock-skew window > 120s. Canonical leeway = 60s (RFC 7519
//!   §4.1.4 industry standard).
//! - No PII (`sub`, `email` raw) in logs; the adapter exposes a
//!   `principal_hash` (SHA-256 prefix 8 hex chars) for tracing.
//!
//! # Quickstart
//!
//! ```rust,no_run
//! use corelink_clerk::{ClerkAdapter, ClerkConfig};
//! use corelink_clerk::fakes::{InMemoryKvCache, StaticJwksFetcher};
//!
//! # async fn ex() -> Result<(), Box<dyn std::error::Error>> {
//! let cache = InMemoryKvCache::new();
//! let fetcher = StaticJwksFetcher::empty();
//! let adapter = ClerkAdapter::new(
//!     ClerkConfig::builder()
//!         .jwks_url("https://clerk.example.dev/.well-known/jwks.json")
//!         .issuer_allowlist(["https://clerk.example.dev".to_string()])
//!         .audience("corelink-api")
//!         .build()?,
//!     fetcher,
//!     cache,
//! );
//! let _principal = adapter.validate("a.b.c").await; // would error on a real token
//! # Ok(()) }
//! ```

#![forbid(unsafe_code)]

// The JWT adapter module (ClerkAdapter::validate) depends on
// `jsonwebtoken` which pulls in `ring`. Ring requires a C toolchain
// that supports wasm32-unknown-unknown, which is not available in
// standard Rust toolchains (requires emscripten or a custom clang).
// Gate the adapter behind the `jwt-adapter` feature so the trait
// surface modules (jwks, jwks_cache, principal, config, error, fakes)
// can compile to wasm32-unknown-unknown for production CF Worker
// binding crates like `corelink-clerk-cf`.
#[cfg(feature = "jwt-adapter")]
pub mod adapter;
pub mod config;
#[cfg(feature = "jwt-adapter")]
pub mod env_config;
pub mod error;
pub mod fakes;
#[cfg(feature = "http-fetcher")]
pub mod http_fetcher;
pub mod jwks;
pub mod jwks_cache;
pub mod principal;
mod redact;

#[cfg(feature = "jwt-adapter")]
pub use adapter::{validate_session, ClerkAdapter};
pub use config::{ClerkConfig, ClerkConfigBuilder, ClerkConfigError, JWKS_TTL_SECS, LEEWAY_SECS};
#[cfg(feature = "jwt-adapter")]
pub use env_config::{
    ClerkEnvSecrets, EnvLoadError, SecretKey, ENV_AUDIENCE, ENV_JWKS_URL, ENV_JWT_ISSUER,
    ENV_PUBLISHABLE_KEY, ENV_SECRET_KEY,
};
pub use error::AuthError;
#[cfg(feature = "http-fetcher")]
pub use http_fetcher::{HttpFetcherBuildError, HttpJwksFetcher};
pub use jwks::{Jwks, JwksFetchError, JwksFetcher, JwksKey};
pub use jwks_cache::{CachedJwks, KvJwksCache, KvJwksCacheError};
pub use principal::{
    ClerkOrgId, ClerkPrincipal, ClerkRole, ClerkSessionId, ClerkUserId, Email, EmailParseError,
};
pub use redact::principal_hash;
