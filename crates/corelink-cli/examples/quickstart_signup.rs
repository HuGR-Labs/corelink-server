//! `quickstart_signup` — Provision a new CoreLink tenant via `POST /v1/signup`.
//!
//! Customer concept: a tenant is the top-level isolation boundary in
//! CoreLink. Signup is **atomic**: tenant row + DPA acceptance + first
//! PAT are committed together (INV-ONBOARD-ATOMIC-PROVISIONING).
//!
//! Run:
//!
//! ```text
//! CORELINK_API_URL=https://api.corelink.dev \
//! CORELINK_PAT=$PAT \
//!   cargo run --example quickstart_signup -p corelink-cli
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
    let idem = format!("idem-{}", Uuid::now_v7());

    let body = json!({
        "clerk_event_id": "evt_demo_quickstart",
        "correlation_id": Uuid::now_v7().to_string(),
        "email_hash": "0".repeat(64),
        "idempotency_key": idem.clone(),
        "locale": "en-US",
    });

    let resp = reqwest::Client::new()
        .post(format!("{api}/v1/signup"))
        .bearer_auth(&pat)
        .header("Idempotency-Key", &idem)
        .json(&body)
        .send()
        .await
        .context("POST /v1/signup")?;

    let status = resp.status();
    let text = resp.text().await.context("read response body")?;
    println!("status={status} body={text}");
    Ok(())
}
