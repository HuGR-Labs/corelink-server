//! Example: basic CRUD flow for the config admin API (WI-S13-001).
//!
//! Demonstrates: create → read current → update via CAS → read history.
#![allow(clippy::print_stdout, clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use uuid::Uuid;

use corelink_ops::config::api::handlers::{
    AdminContext, PutConfigRequest, handle_get_current, handle_get_history, handle_put,
};
use corelink_config_do::{
    AdminActor, ConfigPayload, FeatureFlag,
    metrics::NoopMetrics,
    store::{InMemoryAuditSink, InMemoryConfigSingletonStore},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let audit = Arc::new(InMemoryAuditSink::new());
    let store = InMemoryConfigSingletonStore::new(audit.clone(), Arc::new(NoopMetrics));

    let now_ms = 1_748_000_000_000u64; // 2025-05-23 ~UTC
    let ctx = AdminContext {
        actor: AdminActor { user_id: Uuid::nil(), email_hash: [0u8; 32] },
        mfa_ts_ms: now_ms - 2 * 60 * 1000, // 2 min ago
        is_admin: true,
        dual_approver_user_id: None,
    };

    // Read initial state (version=0).
    let current = handle_get_current(&store).await?;
    println!("Initial version: {}", current.version);

    // Build an update with a feature flag.
    let mut payload = ConfigPayload::genesis();
    payload.feature_flags.insert(
        "dark-launch-new-cache".into(),
        FeatureFlag { enabled: true, rollout_pct: 10, allowlist_tenants: vec![] },
    );

    // CAS update (expected_version=0).
    let resp = handle_put(
        &store,
        &ctx,
        PutConfigRequest { expected_version: 0, new_payload: payload },
        now_ms,
    )
    .await?;
    println!("New version after update: {}", resp.new_version);

    // Read history.
    let history = handle_get_history(&store, 10).await?;
    println!("History entries: {}", history.entries.len());
    for entry in &history.entries {
        println!("  v{} {:?}", entry.version, entry.change_type);
    }

    // Audit sink captured the write.
    let captured = audit.snapshot();
    println!("Audit entries captured: {}", captured.len());

    Ok(())
}
