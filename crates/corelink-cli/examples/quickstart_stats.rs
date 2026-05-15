//! `quickstart_stats` — Probe service health/stats via `GET /api/health`.
//!
//! Customer concept: liveness + readiness probe. The response includes
//! version, region, and per-dependency status — wire this into your
//! synthetic monitor. Anonymous endpoint (no PAT required).
//!
//! Run:
//!
//! ```text
//! cargo run --example quickstart_stats -p corelink-cli
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
        .unwrap_or_else(|_| "https://sandbox.corelink.dev".to_string());

    let resp = reqwest::Client::new()
        .get(format!("{api}/api/health"))
        .send()
        .await
        .context("GET /api/health")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("server returned {status}: {body}");
    }
    let body: serde_json::Value = resp.json().await.context("decode body")?;
    println!("status={status}");
    println!("health={}", serde_json::to_string_pretty(&body)?);
    Ok(())
}
