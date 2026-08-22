//! `quickstart_put` — Issue a new Personal Access Token via `POST /v1/pats`.
//!
//! Customer concept: PATs are the canonical credential for CoreLink
//! APIs. The plaintext token is **shown-once** in the response
//! (CTRL-CRED-001) — store it in your secret manager immediately.
//!
//! Run:
//!
//! ```text
//! CORELINK_PAT=$BOOTSTRAP_PAT \
//!   cargo run --example quickstart_put -p corelink-cli
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
        "label": "ci-cache-rw",
        "scopes": ["cache:r", "cache:w"],
        "ttl_hours": 24,
    });

    let resp = reqwest::Client::new()
        .post(format!("{api}/v1/pats"))
        .bearer_auth(&pat)
        .header("Idempotency-Key", &idem)
        .json(&body)
        .send()
        .await
        .context("POST /v1/pats")?;

    let status = resp.status();
    let text = resp.text().await.context("read response body")?;
    println!("status={status}");
    println!("response={text}");
    println!("NOTE: the plaintext token is shown only once — store it now.");
    Ok(())
}
