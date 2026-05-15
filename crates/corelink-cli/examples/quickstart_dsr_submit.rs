//! `quickstart_dsr_submit` — Submit a DSR (export/erase) via
//! `POST /v1/privacy/dsr/{action}`.
//!
//! Customer concept: GDPR/CCPA data-subject requests. Returns a
//! `request_id`; poll `GET /v1/privacy/dsr/{request_id}/status` for
//! progress. Erasure produces a signed attestation
//! (CTRL-ERASURE-ATTEST-001).
//!
//! Run:
//!
//! ```text
//! CORELINK_PAT=$PAT \
//!   cargo run --example quickstart_dsr_submit -p corelink-cli
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
        .unwrap_or_else(|_| "https://sandbox.corelink.dev".to_string());
    let pat = env::var("CORELINK_PAT").context("CORELINK_PAT is required")?;
    let action = env::var("DSR_ACTION").unwrap_or_else(|_| "export".to_string());
    let idem = format!("idem-{}", Uuid::now_v7());

    let body = json!({
        "subject_email_hash": "0".repeat(64),
        "verification_token": "demo-verification-token",
        "scope": ["profile", "audit_events"],
    });

    let resp = reqwest::Client::new()
        .post(format!("{api}/v1/privacy/dsr/{action}"))
        .bearer_auth(&pat)
        .header("Idempotency-Key", &idem)
        .json(&body)
        .send()
        .await
        .context("POST /v1/privacy/dsr/{action}")?;

    let status = resp.status();
    let text = resp.text().await.context("read response body")?;
    println!("action={action} status={status} body={text}");
    Ok(())
}
