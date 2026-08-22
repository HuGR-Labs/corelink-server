//! `quickstart_get` — Fetch the authenticated principal via `GET /v1/users/me`.
//!
//! Customer concept: the canonical "who am I" probe. Use this to
//! verify PAT validity, scopes, and tenant binding before issuing
//! follow-up calls. Returns the PrincipalProfile envelope.
//!
//! Run:
//!
//! ```text
//! CORELINK_PAT=$PAT \
//!   cargo run --example quickstart_get -p corelink-cli
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
        .unwrap_or_else(|_| "https://corelink-api.humangr.com".to_string());
    let pat = env::var("CORELINK_PAT").context("CORELINK_PAT is required")?;

    let resp = reqwest::Client::new()
        .get(format!("{api}/v1/users/me"))
        .bearer_auth(&pat)
        .send()
        .await
        .context("GET /v1/users/me")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("server returned {status}: {body}");
    }
    let profile: serde_json::Value = resp.json().await.context("decode body")?;
    println!("status={status}");
    println!("principal={}", serde_json::to_string_pretty(&profile)?);
    Ok(())
}
