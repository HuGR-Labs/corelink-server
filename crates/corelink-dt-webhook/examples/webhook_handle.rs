#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::indexing_slicing,
    clippy::panic
)]
//! Example: handle a DT webhook event end-to-end.
//!
//! Run: `cargo run -p corelink-dt-webhook --example webhook_handle`

use corelink_dt_webhook::{
    handler::InMemoryDtWebhookHandler,
    hmac::sign,
    types::{ComponentMetadata, DtEventType, DtProjectUuid, DtWebhookEvent, VulnerabilityMetadata},
    DtWebhookHandler,
};
use std::time::SystemTime;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    let secret = b"example-webhook-secret";
    let handler = InMemoryDtWebhookHandler::new(secret.to_vec(), false);

    let event = DtWebhookEvent {
        event_type: DtEventType::NewVulnerability,
        project_uuid: DtProjectUuid::new("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")?,
        component: ComponentMetadata {
            purl: "pkg:cargo/ring@0.16.0".into(),
            name: "ring".into(),
            version: "0.16.0".into(),
            patched_locally: false,
            patched_locally_adr: None,
            adr_ratified_date: None,
        },
        vulnerability: VulnerabilityMetadata {
            cve_id: "CVE-2024-12345".into(),
            cvss_score: 9.5,
            severity_label: Some("CRITICAL".into()),
            description: Some("Use-after-free in ring AES-GCM implementation".into()),
            sources: vec!["NVD".into(), "OSV".into(), "GHSA".into()],
        },
        timestamp: SystemTime::now(),
    };

    // Sign the event (in production, DT signs with the shared secret).
    let body = serde_json::to_vec(&event)?;
    let sig = sign(secret, &body)?;
    handler.set_signature(Some(sig));

    let delivered = handler.handle_webhook(event).await?;
    println!(
        "Alert delivered: cve={} severity={} channels={:?} latency={}ms",
        delivered.cve_id, delivered.severity, delivered.channels, delivered.delivery_latency_ms,
    );

    Ok(())
}
