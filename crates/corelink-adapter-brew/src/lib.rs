//! `corelink-adapter-brew` — Homebrew bottle caching adapter.
//!
//! Wave-34 lands this crate as a read-path HTTPS proxy that brew
//! redirects to via the `HOMEBREW_BOTTLE_DOMAIN` environment override
//! (canonical reference: <https://docs.brew.sh/Manpage#environment>).
//! With `HOMEBREW_BOTTLE_DOMAIN=http://corelink-brew-adapter`, every
//! `brew install` triggers a plain HTTPS `GET <domain>/<bottle-path>`
//! against this adapter. The adapter:
//!
//! 1. authenticates the caller via PAT (`Authorization: Bearer
//!    hugr-pat_<token>`; constant-time compared via `subtle`);
//! 2. derives a per-tenant CAS key from the canonicalized request URL
//!    using BLAKE3 (brew URLs do NOT embed an upstream SHA, so the
//!    canonical URL is the only stable cache key — best-effort
//!    integrity, with brew's downstream formula-DSL hash check as the
//!    authoritative verification gate);
//! 3. serves the bottle from CAS on hit; on miss, fetches upstream
//!    (default `https://ghcr.io`), emits an audit row BEFORE the CAS
//!    put (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`), then stores and
//!    streams the bytes back to brew.
//!
//! ## Scope (per `specs/_proposals/adapters/brew.md` §1)
//!
//! **In scope:** bottle (`.tar.gz`) caching via `HOMEBREW_BOTTLE_DOMAIN`.
//! Read path (`brew install`). Per-tenant namespace.
//!
//! **Out of scope:** tap source caching, formula generation, casks
//! (`.dmg`/`.pkg`), bottling itself (`brew bottle`), Linux/Apple Silicon
//! arch matrix logic (brew handles arch; we cache whatever brew
//! requests).
//!
//! ## Charter constraints enforced
//!
//! - `#![forbid(unsafe_code)]`: enforced at crate root.
//! - `#[non_exhaustive]`: every `pub` type below carries the attribute
//!   so future fields can be added without breaking call sites.
//! - No `unwrap()`/`expect()`/`panic!()` in `src/` (clippy `deny`).
//! - PAT compared via `subtle::ConstantTimeEq`; secrets boxed in
//!   `secrecy::SecretString`.
//! - Audit emit BEFORE CAS state mutation — see `bottle::Bottle::get`.

#![forbid(unsafe_code)]

pub mod audit;
pub mod auth;
pub mod bottle;
pub mod config;
pub mod error;
pub mod ports;
pub mod server;
pub mod upstream;

pub use config::BrewAdapterConfig;
pub use error::BrewAdapterError;
pub use server::run_brew_adapter;
