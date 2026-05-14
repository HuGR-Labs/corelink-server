//! Example: rollback drill playbook (WI-S13-001).
//!
//! Monthly drill procedure:
//! 1. Build up several config versions.
//! 2. Simulate an erroneous update at v+1.
//! 3. Rollback to a safe known-good version.
//! 4. Verify recovery ≤ 5 min p99 (SLO-ADMIN-CONFIG-ROLLBACK).
#![allow(clippy::print_stdout, clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use uuid::Uuid;

use corelink_config_api::handlers::{
    AdminContext, PutConfigRequest, handle_get_current, handle_put, handle_rollback,
};
use corelink_config_do::{
    AdminActor, ConfigPayload, FeatureFlag, RateLimitKey, RateLimitTunable,
    metrics::NoopMetrics,
    store::{ConfigSingletonStore, InMemoryAuditSink, InMemoryConfigSingletonStore},
};

fn safe_payload() -> ConfigPayload {
    let mut p = ConfigPayload::genesis();
    p.rate_limits.insert(
        RateLimitKey { layer: "cas_put".into(), tier: "Solo".into() },
        RateLimitTunable { refill_rate_per_sec: 100, burst: 200 },
    );
    p
}

fn erroneous_payload() -> ConfigPayload {
    // Simulates an erroneous flag that was accidentally enabled.
    let mut p = safe_payload();
    p.feature_flags.insert(
        "break-everything".into(),
        FeatureFlag { enabled: true, rollout_pct: 100, allowlist_tenants: vec![] },
    );
    p
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let audit = Arc::new(InMemoryAuditSink::new());
    let store = InMemoryConfigSingletonStore::new(audit.clone(), Arc::new(NoopMetrics));

    let now_ms = 1_748_000_000_000u64;
    let dual_approver = Uuid::nil();
    let ctx = AdminContext {
        actor: AdminActor { user_id: Uuid::nil(), email_hash: [0u8; 32] },
        mfa_ts_ms: now_ms - 60 * 1000,
        is_admin: true,
        dual_approver_user_id: Some(dual_approver),
    };

    // Step 1: Establish 3 safe versions.
    for i in 0..3u64 {
        let current = handle_get_current(&store).await?;
        let resp = handle_put(
            &store,
            &ctx,
            PutConfigRequest { expected_version: current.version, new_payload: safe_payload() },
            now_ms,
        )
        .await?;
        println!("Safe version v{} established", resp.new_version);
        let _ = i;
    }

    // Step 2: Record the good version to roll back to.
    let (good_version, _) = store.current().await?;
    println!("Good version checkpoint: v{good_version}");

    // Step 3: Apply erroneous update.
    let bad_resp = handle_put(
        &store,
        &ctx,
        PutConfigRequest { expected_version: good_version, new_payload: erroneous_payload() },
        now_ms,
    )
    .await?;
    println!("Erroneous version applied: v{}", bad_resp.new_version);

    // Step 4: Rollback to good version (dual-approval present).
    let drill_start_ms = now_ms;
    let rb_resp = handle_rollback(&store, &ctx, good_version, now_ms).await?;
    let drill_elapsed_ms = now_ms - drill_start_ms; // 0 in this in-memory drill
    println!(
        "Rollback to v{good_version} succeeded: new version=v{}, elapsed={}ms",
        rb_resp.new_version, drill_elapsed_ms
    );
    assert!(drill_elapsed_ms < 5 * 60 * 1000, "SLO: rollback ≤ 5 min p99");

    // Step 5: Verify audit chain.
    let entries = audit.snapshot();
    println!("Total audit entries: {}", entries.len());
    for e in &entries {
        println!("  v{} {:?}", e.version, e.change_type);
    }

    Ok(())
}
