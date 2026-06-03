//! `quickstart_audit` — Stream audit events via `GET /v1/admin/audit-events`.
//!
//! Customer concept: every privileged operation in CoreLink is
//! Merkle-chained into the audit log (INV-AUDIT-APPEND-ONLY). This
//! endpoint paginates the chain in commit order; the response includes
//! `chain_head_hash` for client-side verification (CTRL-AUDIT-002).
//!
//! Run:
//!
//! ```text
//! CORELINK_PAT=$ADMIN_PAT \
//!   cargo run --example quickstart_audit -p corelink-cli
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
        .get(format!("{api}/v1/admin/audit-events"))
        .bearer_auth(&pat)
        .query(&[("limit", "50")])
        .send()
        .await
        .context("GET /v1/admin/audit-events")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("server returned {status}: {body}");
    }
    let body: serde_json::Value = resp.json().await.context("decode body")?;
    let events = body
        .get("events")
        .and_then(|v| v.as_array())
        .map_or(0, Vec::len);
    let head = body
        .get("chain_head_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("<none>");
    println!("status={status} events={events} chain_head={head}");
    Ok(())
}
