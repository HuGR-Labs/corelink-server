//! `quickstart_byok_rotate` — File a BYOK rotation op via `POST /v1/admin/ops`.
//!
//! Customer concept: privileged operations in CoreLink are
//! **dual-approval gated** (CTRL-DUAL-APPROVAL-001). Filing the op
//! returns an `op_id`; a second admin must approve via
//! `POST /v1/admin/ops/{op_id}/approve` before execution.
//!
//! Run:
//!
//! ```text
//! CORELINK_PAT=$ADMIN_PAT \
//!   cargo run --example quickstart_byok_rotate -p corelink-cli
//! ```

#![forbid(unsafe_code)]
#![allow(
    clippy::print_stdout,
    reason = "examples emit human-readable narration to stdout"
)]

use anyhow::{Context, Result};
use serde_json::json;
use std::env;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    let api = env::var("CORELINK_API_URL")
        .unwrap_or_else(|_| "https://corelink-api.humangr.com".to_string());
    let pat = env::var("CORELINK_PAT").context("CORELINK_PAT is required")?;
    let idem = format!("idem-{}", Uuid::now_v7());

    let body = json!({
        "op_type": "byok_rotate",
        "reason": "scheduled quarterly rotation",
        "params": { "key_alias": "primary", "target_region": "eu-west-1" },
    });

    let resp = reqwest::Client::new()
        .post(format!("{api}/v1/admin/ops"))
        .bearer_auth(&pat)
        .header("Idempotency-Key", &idem)
        .json(&body)
        .send()
        .await
        .context("POST /v1/admin/ops")?;

    let status = resp.status();
    let text = resp.text().await.context("read response body")?;
    println!("status={status} body={text}");
    println!("NOTE: op is PENDING until a second admin approves it.");
    Ok(())
}
