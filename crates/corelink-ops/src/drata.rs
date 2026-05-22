//! SOC 2 Drata evidence collection pipeline — wave-33 canonical surface.
//!
//! Re-exports the entire public API of `corelink-drata-sync` (R5-prep
//! SOC 2 evidence pipeline: pulls evidence from audit_outbox / tenants
//! RBAC / PAT issuance / GH PR-merge / PD incident timeline / pentest
//! findings → POSTs to Drata REST API with Bearer auth, SHA-256
//! idempotency dedup, retry, fail-CLOSED audit emit). The actual
//! implementation lives in `crates/corelink-drata-sync/` (Stage 1
//! Stream C sub-step C.2 Option-A aggregator pattern).

pub use corelink_drata_sync::*;
