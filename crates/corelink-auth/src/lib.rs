//! `corelink-auth` — canonical authentication-context surface for the
//! CoreLink Rust workspace.
//!
//! Wave-33 Stage 1 Stream B sub-step B.3 lands this crate as the
//! **single import target** for every authentication primitive that
//! previously lived across 6 separate crates:
//!
//! ```text
//! use corelink_auth::clerk::*;        // Clerk JWT verify + JWKS cache + adapter
//! use corelink_auth::clerk_cf::*;     // Clerk CF Worker bindings (wasm32 pure-logic surface)
//! use corelink_auth::schema::*;       // D1 auth schema + RLS WITH CHECK enforcement
//! use corelink_auth::webauthn::*;     // FIDO2/WebAuthn registration + assertion
//! use corelink_auth::pat::*;          // Personal Access Token (PAT) issuance + revoke
//! use corelink_auth::tenant_path::*;  // CTRL-CAS-001 tenant-path prefix enforcer
//! ```
//!
//! ## Stage 1 Stream B absorption strategy — Option-A aggregator
//!
//! Per `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §6 Stage 1
//! Stream B and the Stage 0 SEAL audit §4 (Option-A aggregator
//! interpretation), this crate "absorbs" 6 existing crates by
//! re-exporting them at canonical submodule paths. The absorbed
//! crates remain the canonical sources of truth — their src/, tests/,
//! benches/, and fuzz/ harnesses are unchanged. Consumer migration
//! (apps/server routes, `corelink-worker` wasm32 entry) proceeds
//! incrementally.
//!
//! ### Absorbed crates (6)
//!
//! - `corelink-clerk` — Clerk JWT verify (JWKS cache + RS256 EdDSA
//!   adapter + nonce + 5-min clock skew); re-exported at [`clerk`].
//! - `corelink-clerk-cf` — Clerk CF Worker bindings pure-logic
//!   portion (CfKvJwksCache + ClerkWebhookHandler scaffolding;
//!   wasm32-gated wire-up portion is **NOT** moved by Stage 1, only
//!   re-exported); re-exported at [`clerk_cf`].
//! - `corelink-auth-schema` — D1 auth schema + `set_local
//!   app.current_tenant` GUC + RLS WITH CHECK on every txn;
//!   re-exported at [`schema`].
//! - `corelink-webauthn` — FIDO2 WebAuthn registration + assertion
//!   ceremonies; re-exported at [`webauthn`].
//! - `corelink-pat` — Personal Access Token issuance + verify +
//!   revoke + rotation; re-exported at [`pat`].
//! - `corelink-tenant-path` — CTRL-CAS-001 tenant-path prefix
//!   enforcer (the only non-`corelink-`-named directory; package
//!   IS `corelink-tenant-path`); re-exported at [`tenant_path`].
//!
//! ### Why aggregator rather than physical move
//!
//! 1. **`corelink-clerk-cf` wasm32 build path** — the absorbed crate
//!    ships a `#[durable_object]` Cloudflare Worker actor class that
//!    only compiles to `wasm32-unknown-unknown`. Physically moving
//!    the wasm32-gated wire-up into the umbrella requires migrating
//!    the `wrangler.toml` entry + the wasm32-only `corelink-cf-bindings`
//!    dependency tree atomically. That belongs to Stage 2 binding
//!    consolidation. Charter Hard Pause Trigger 5 (wasm32 build
//!    breaks) is preserved by leaving the wire-up in place.
//! 2. **`corelink-auth-schema` D1 migration coupling** — the schema
//!    crate is referenced from `migrations/d1/*.sql` paths and from
//!    the D1 migrations replay harness (`corelink-d1-migrations`).
//!    Moving src/ requires atomic update of the migrations runner;
//!    that lives in Stage 2 / Stream C territory.
//! 3. **`corelink-tenant-path` bench harness** — the absorbed crate
//!    ships 3 `harness=false` benches (`derive`, `derive_prefix_v2`,
//!    `derive_prefix_cached`) that pin the prefix-enforcement hot
//!    path against historical baselines. Physically relocating the
//!    src/ would require updating the bench paths; that belongs to
//!    the consumer-migration stream that owns the bench harness.
//!
//! ## Behaviour preservation
//!
//! Every public symbol of the 6 absorbed crates remains reachable at
//! its original path AND at the new canonical path. No public-API
//! contract is broken. Stage 2 / Stage 3 consumers may adopt the new
//! canonical paths incrementally without coordination cost.
//!
//! ## Charter compliance (preserved by reference)
//!
//! - `INV-AUTH-SCHEMA-RLS-DEFAULT-ON` (CRITICAL) — RLS WITH CHECK
//!   on every txn in `corelink-auth-schema`; not touched by this
//!   aggregator (charter Hard Pause Trigger 1).
//! - `subtle::ConstantTimeEq` on PAT verify path
//!   (`corelink-pat::verify_*`) — preserved by reference (charter
//!   Hard Pause Trigger 3).
//! - `SecretString` on Clerk JWT signing keys + PAT secret bytes —
//!   preserved by reference.
//! - Clerk-Signature HMAC-SHA256 + nonce + 5-min timestamp window —
//!   preserved by reference.
//! - WebAuthn challenge nonce + counter-rollback rejection —
//!   preserved by reference.
//! - `#[non_exhaustive]` on every public enum/struct — re-exports
//!   inherit the attribute from the source crate.
//!
//! ## What this crate does NOT do
//!
//! - Define new types. Every type, trait, and constant surfaced is a
//!   re-export of an absorbed crate.
//! - Wire the `corelink-clerk` `jwt-adapter` feature on by default.
//!   The workspace `corelink-clerk = { ..., default-features = false }`
//!   pin is mirrored here; consumers that need the production JWT
//!   adapter MUST opt in via `corelink-auth/clerk-jwt-adapter` (the
//!   feature forwards to `corelink-clerk/jwt-adapter`).
//! - Migrate audit emit sites. The wave-33 `AuditEmitter` chokepoint
//!   trait (`corelink_audit::ports::AuditEmitter`) is available for
//!   auth-context adoption; Stream B's Stage 1 commits do NOT
//!   migrate any existing emit site to preserve the "behaviour-
//!   preserving refactor ONLY" charter rule.
//!
//! ## Wave 35 Phase 2 absorption
//!
//! Per `specs/_audits/2026-05-26-wave-33-34-closure-followups.md` §2
//! (CLOSED 2026-05-26, sealed by commits `9fb5a4fd` + `44988a11`), 2
//! of the Wave-33 Option-A re-export tenants were physically absorbed
//! into this crate as inline `pub mod` submodules; their former
//! external crates were dropped from `workspace.members`. Public-API
//! paths (`corelink_auth::webauthn::*` + `corelink_auth::schema::*`)
//! are preserved 1:1; INV-AUTH-* invariants, WebAuthn challenge
//! nonce + counter-rollback rejection, D1 RLS WITH CHECK enforcement,
//! `#![forbid(unsafe_code)]`, and `#[non_exhaustive]` discipline all
//! preserved by reference.
//!
//! Absorbed crates (2):
//!
//! - `corelink-webauthn` → [`webauthn`] — FIDO2 WebAuthn registration
//!   + assertion ceremonies.
//! - `corelink-auth-schema` → [`schema`] — D1 auth schema + RLS WITH
//!   CHECK enforcement + `set_local app.current_tenant` GUC.
//!
//! The remaining Wave-33 Stream-B tenants (`clerk`, `clerk_cf`, `pat`,
//! `tenant_path`) stay as aggregator re-exports — `clerk_cf` retains
//! its wasm32 build path (charter Hard Pause Trigger 5), and the
//! others have live external consumers per the closure-followups doc.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod clerk;
pub mod clerk_cf;
pub mod pat;
pub mod schema;
pub mod tenant_path;
pub mod webauthn;

#[cfg(feature = "tower-middleware")]
pub mod middleware {
    //! Wave 33 Stage 2.A-v2 additive aggregator (per 2.A HALT audit §9(b)):
    //! canonical wave-33 path for Tower/HTTP auth middleware that physically
    //! lives in `corelink-worker::middleware`. Includes the timing-padding
    //! 404 layer (INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE). Gated behind the
    //! `tower-middleware` feature because the underlying `corelink-worker`
    //! module is wasm32-excluded; enable via `corelink-auth/tower-middleware`.
    pub use corelink_worker::middleware::*;
}

#[cfg(feature = "tower-middleware")]
pub mod worker_session {
    //! Wave 33 Stage 2.A-v2 additive aggregator: canonical wave-33 path for
    //! worker-side auth session lifecycle adapters (revocation orchestrator,
    //! KV invalidator, cross-region propagation) that physically live in
    //! `corelink-worker::auth`. Gated behind `tower-middleware` for the same
    //! wasm32 reason as `middleware` above.
    pub use corelink_worker::auth::*;
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    //! Smoke tests proving every canonical re-export path resolves at
    //! compile time. No new state is introduced.

    #[test]
    fn schema_path_resolves() {
        // Surface a constant from the re-exported schema crate to
        // prove the path resolves at compile time.
        #[allow(unused_imports)]
        use crate::schema as _s;
    }

    #[test]
    fn pat_path_resolves() {
        // PAT issuance / verify trait surface.
        #[allow(unused_imports)]
        use crate::pat as _p;
    }

    #[test]
    fn webauthn_path_resolves() {
        // WebAuthn ceremony types.
        #[allow(unused_imports)]
        use crate::webauthn as _w;
    }

    #[test]
    fn tenant_path_path_resolves() {
        // CTRL-CAS-001 prefix enforcer.
        #[allow(unused_imports)]
        use crate::tenant_path as _t;
    }

    #[test]
    fn clerk_path_resolves() {
        // Clerk JWT verify surface.
        #[allow(unused_imports)]
        use crate::clerk as _c;
    }

    #[test]
    fn clerk_cf_path_resolves() {
        // Clerk CF Worker bindings (native build surface; wasm32
        // wire-up is gated inside the absorbed crate).
        #[allow(unused_imports)]
        use crate::clerk_cf as _cf;
    }
}
