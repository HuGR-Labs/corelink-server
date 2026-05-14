//! `corelink-config-api` — Worker HTTP handlers for the DO config-singleton
//! admin API (WI-S13-001, HIGH_RISK lane FF-HR-005).
//!
//! # What this crate ships
//!
//! - [`handlers`] module: [`handle_get_current`], [`handle_put`],
//!   [`handle_rollback`], [`handle_get_history`] — logic for the four
//!   admin config endpoints.
//! - [`middleware::mfa_freshness`]: [`check_mfa_freshness`] — enforces
//!   CTRL-AUTH-010 + INV-ADMIN-MFA-FRESHNESS (30 min hard window).
//! - [`error::ApiError`]: HTTP status-aware error taxonomy.
//!
//! # Auth & authz enforcement (per handler)
//!
//! | Endpoint | Admin role | MFA freshness | Dual-approval |
//! |----------|------------|---------------|---------------|
//! | GET /current | No | No | No |
//! | PUT /config | Yes | Yes | No |
//! | POST /rollback | Yes | Yes | Yes |
//! | GET /history | No | No | No |
//!
//! # wasm32 compatibility
//!
//! This crate is wasm32-clean: no `tokio` in `src/`, uuid uses `js` feature
//! on wasm32 targets.
//!
//! # Quickstart — PUT update
//!
//! ```
//! use corelink_config_api::{
//!     handlers::{AdminContext, PutConfigRequest, handle_put},
//!     error::ApiError,
//! };
//! use corelink_config_do::{
//!     AdminActor, ConfigPayload,
//!     metrics::NoopMetrics,
//!     store::{InMemoryAuditSink, InMemoryConfigSingletonStore},
//! };
//! use std::sync::Arc;
//! use uuid::Uuid;
//!
//! # async fn ex() -> Result<(), Box<dyn std::error::Error>> {
//! let store = InMemoryConfigSingletonStore::new(
//!     Arc::new(InMemoryAuditSink::new()),
//!     Arc::new(NoopMetrics),
//! );
//!
//! let now_ms = 10_000_000u64;
//! let ctx = AdminContext {
//!     actor: AdminActor { user_id: Uuid::nil(), email_hash: [0u8; 32] },
//!     mfa_ts_ms: now_ms - 5 * 60 * 1000, // 5 min ago (fresh)
//!     is_admin: true,
//!     dual_approver_user_id: None,
//! };
//! let req = PutConfigRequest {
//!     expected_version: 0,
//!     new_payload: ConfigPayload::genesis(),
//! };
//! let resp = handle_put(&store, &ctx, req, now_ms).await?;
//! assert_eq!(resp.new_version, 1);
//! # Ok(()) }
//! # tokio_test::block_on(ex()).expect("doctest");
//! ```

#![forbid(unsafe_code)]

pub mod error;
pub mod handlers;
pub mod middleware;

pub use error::ApiError;
pub use handlers::{
    AdminContext, GetCurrentResponse, HistoryResponse, PutConfigRequest, PutConfigResponse,
    RollbackResponse, handle_get_current, handle_get_history, handle_put, handle_rollback,
};

/// Crate version string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
