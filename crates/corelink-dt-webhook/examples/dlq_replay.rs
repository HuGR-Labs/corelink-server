//! Example: DLQ replay — drain failed webhook events and re-process them.
//!
//! Simulates the daily reconciliation job re-driving the dead-letter queue.
//!
//! Run: `cargo run -p corelink-dt-webhook --example dlq_replay`

use corelink_dt_webhook::{
    dlq::{DlqEntry, InMemoryDlq},
    handler::InMemoryDtWebhookHandler,
    hmac::sign,
    types::{ComponentMetadata, DtEventType, DtProjectUuid, DtWebhookEvent, VulnerabilityMetadata},
    DtWebhookHandler,
};
use std::time::SystemTime;

fn make_failed_event(cve_id: &str) -> DtWebhookEvent {
    DtWebhookEvent {
        event_type: DtEventType::NewVulnerability,
        project_uuid: DtProjectUuid::new("replay-project").expect("valid"),
        component: ComponentMetadata {
            purl: "pkg:cargo/ring@0.16.0".into(),
            name: "ring".into(),
            version: "0.16.0".into(),
            patched_locally: false,
            patched_locally_adr: None,
            adr_ratified_date: None,
        },
        vulnerability: VulnerabilityMetadata {
            cve_id: cve_id.to_owned(),
            cvss_score: 7.5,
            severity_label: Some("HIGH".into()),
            description: None,
            sources: vec!["NVD".into()],
        },
        timestamp: SystemTime::now(),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    let dlq = InMemoryDlq::new();
    let secret = b"replay-secret";

    // Seed the DLQ with 3 failed events (simulating previous delivery failures).
    for i in 1..=3 {
        dlq.push(DlqEntry {
            event: make_failed_event(&format!("CVE-2024-{i:05}")),
            attempt_count: 1,
            last_error: "Slack 500".into(),
        })?;
    }

    println!("DLQ size before replay: {}", dlq.len()?);

    // Replay: drain and re-deliver.
    let handler = InMemoryDtWebhookHandler::new(secret.to_vec(), false);
    let entries = dlq.drain_all()?;
    let mut ok = 0usize;
    let mut failed = 0usize;

    for entry in entries {
        let body = serde_json::to_vec(&entry.event)?;
        let sig = sign(secret, &body)?;
        handler.set_signature(Some(sig));

        match handler.handle_webhook(entry.event.clone()).await {
            Ok(delivered) => {
                ok += 1;
                println!(
                    "Replayed OK: cve={} channels={:?}",
                    delivered.cve_id, delivered.channels
                );
            }
            Err(e) => {
                failed += 1;
                eprintln!(
                    "Replay failed (attempt {}): cve={} error={e}",
                    entry.attempt_count + 1,
                    entry.event.vulnerability.cve_id,
                );
                // Re-enqueue with incremented attempt count (up to 3).
                if entry.attempt_count < 3 {
                    dlq.push(DlqEntry {
                        event: entry.event,
                        attempt_count: entry.attempt_count + 1,
                        last_error: e.to_string(),
                    })?;
                }
            }
        }
    }

    println!(
        "DLQ replay complete: ok={ok} failed={failed} dlq_remaining={}",
        dlq.len()?
    );
    Ok(())
}
