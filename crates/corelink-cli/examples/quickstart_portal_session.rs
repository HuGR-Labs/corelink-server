//! `quickstart_portal_session` — Bootstrap an enterprise-portal session via
//! `POST /v1/enterprise/inquire`.
//!
//! Customer concept: enterprise prospects use the portal to request
//! contracted-tier provisioning (DPA + BAA + custom SLA). This call
//! returns a portal URL + short-lived session token; the prospect
//! completes the questionnaire in-browser.
//!
//! Run:
//!
//! ```text
//! CORELINK_PAT=$PAT \
//!   cargo run --example quickstart_portal_session -p corelink-cli
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
        .unwrap_or_else(|_| "https://sandbox.corelink.humangr.com".to_string());
    let pat = env::var("CORELINK_PAT").context("CORELINK_PAT is required")?;
    let idem = format!("idem-{}", Uuid::now_v7());

    let body = json!({
        "company": "Acme Corp",
        "contact_email_hash": "0".repeat(64),
        "expected_seats": 250,
        "use_case": "monorepo build cache for 80 engineers",
    });

    let resp = reqwest::Client::new()
        .post(format!("{api}/v1/enterprise/inquire"))
        .bearer_auth(&pat)
        .header("Idempotency-Key", &idem)
        .json(&body)
        .send()
        .await
        .context("POST /v1/enterprise/inquire")?;

    let status = resp.status();
    let text = resp.text().await.context("read response body")?;
    println!("status={status} body={text}");
    Ok(())
}
