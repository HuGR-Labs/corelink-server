//! `quickstart_list` — List all PATs for the current tenant via `GET /v1/pats`.
//!
//! Customer concept: PATs are server-side authoritative — this is the
//! source of truth for credential inventory. Plaintext tokens are
//! **never** returned by this endpoint (only metadata + last-4 prefix).
//!
//! Run:
//!
//! ```text
//! CORELINK_PAT=$PAT \
//!   cargo run --example quickstart_list -p corelink-cli
//! ```

#![forbid(unsafe_code)]
#![allow(
    clippy::print_stdout,
    reason = "examples emit human-readable narration to stdout"
)]

use anyhow::{Context, Result};
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let api = env::var("CORELINK_API_URL")
        .unwrap_or_else(|_| "https://sandbox.corelink.humangr.com".to_string());
    let pat = env::var("CORELINK_PAT").context("CORELINK_PAT is required")?;

    let resp = reqwest::Client::new()
        .get(format!("{api}/v1/pats"))
        .bearer_auth(&pat)
        .send()
        .await
        .context("GET /v1/pats")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("server returned {status}: {body}");
    }
    let body: serde_json::Value = resp.json().await.context("decode body")?;
    let count = body
        .get("items")
        .and_then(|v| v.as_array())
        .map_or(0, Vec::len);
    println!("status={status} pat_count={count}");
    println!("body={}", serde_json::to_string_pretty(&body)?);
    Ok(())
}
