//! `quickstart_team_invite` — File a team-invite op via `POST /v1/admin/ops`.
//!
//! Customer concept: adding members to a tenant flows through the
//! admin-ops dual-approval gate (CTRL-DUAL-APPROVAL-001). The op
//! is `PENDING` until a second admin approves; on approval an invite
//! email is dispatched via the verified-sender pipeline.
//!
//! Run:
//!
//! ```text
//! CORELINK_PAT=$ADMIN_PAT INVITEE_EMAIL=alice@example.com \
//!   cargo run --example quickstart_team_invite -p corelink-cli
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
    let invitee =
        env::var("INVITEE_EMAIL").unwrap_or_else(|_| "newmember@example.com".to_string());
    let idem = format!("idem-{}", Uuid::now_v7());

    let body = json!({
        "op_type": "team_invite",
        "reason": "onboard new engineer",
        "params": { "email": invitee, "role": "developer" },
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
    Ok(())
}
