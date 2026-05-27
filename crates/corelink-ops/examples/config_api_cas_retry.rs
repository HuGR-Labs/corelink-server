//! Example: CAS retry pattern with exponential backoff (WI-S13-001).
//!
//! Demonstrates the recommended client-side retry pattern:
//! 1. Fetch current version.
//! 2. Attempt CAS update.
//! 3. On `VersionConflict` (409): back off exponentially, re-fetch, retry.
//! 4. Abort after 3 attempts.
#![allow(clippy::print_stdout, clippy::print_stderr, clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use uuid::Uuid;

use corelink_ops::config::api::handlers::{AdminContext, PutConfigRequest, handle_get_current, handle_put};
use corelink_config_do::{
    AdminActor, ConfigError, ConfigPayload,
    metrics::NoopMetrics,
    store::{InMemoryAuditSink, InMemoryConfigSingletonStore},
};

const MAX_RETRIES: u32 = 3;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = InMemoryConfigSingletonStore::new(
        Arc::new(InMemoryAuditSink::new()),
        Arc::new(NoopMetrics),
    );

    let now_ms = 1_748_000_000_000u64;
    let ctx = AdminContext {
        actor: AdminActor { user_id: Uuid::nil(), email_hash: [0u8; 32] },
        mfa_ts_ms: now_ms - 60 * 1000,
        is_admin: true,
        dual_approver_user_id: None,
    };

    let mut attempts = 0u32;
    loop {
        attempts += 1;
        if attempts > MAX_RETRIES {
            eprintln!("CAS retry budget exhausted after {MAX_RETRIES} attempts");
            break;
        }

        // Fetch current version before each attempt.
        let current = handle_get_current(&store).await?;
        println!("Attempt {attempts}: current version={}", current.version);

        let result = handle_put(
            &store,
            &ctx,
            PutConfigRequest {
                expected_version: current.version,
                new_payload: ConfigPayload::genesis(),
            },
            now_ms,
        )
        .await;

        match result {
            Ok(resp) => {
                println!("CAS update succeeded at version {}", resp.new_version);
                break;
            }
            Err(ref e)
                if matches!(
                    e,
                    corelink_ops::config::api::ApiError::Config(ConfigError::VersionConflict { .. })
                ) =>
            {
                // Exponential backoff (simulated — no sleep in example to stay wasm32-clean).
                let backoff_ms = 100u64 * (1 << (attempts - 1));
                println!("  VersionConflict — would back off {backoff_ms}ms before retry");
                // In production: tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }

    Ok(())
}
