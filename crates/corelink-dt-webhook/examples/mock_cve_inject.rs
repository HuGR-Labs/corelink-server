//! Example: inject a synthetic CVE for E2E alert path validation.
//!
//! Simulates the nightly CI mock CVE injection test.
//!
//! Run: `cargo run -p corelink-dt-webhook --example mock_cve_inject`

use corelink_dt_webhook::{
    handler::InMemoryDtWebhookHandler,
    types::{DtProjectUuid, SyntheticCve},
    DtWebhookHandler,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    // Mock injection requires DT_MOCK_INJECTION_ENABLED=true in production.
    // Here we construct the handler with the flag enabled directly.
    let handler = InMemoryDtWebhookHandler::new(b"staging-secret".to_vec(), true);

    let project_uuid = DtProjectUuid::new("staging-project-uuid-12345")?;
    let synthetic_cve = SyntheticCve {
        cve_id: "CVE-2026-9999".into(),
        cvss_score: 9.8,
        description: Some("Synthetic test CVE — nightly CI mock injection".into()),
    };

    println!("Injecting mock CVE: {}", synthetic_cve.cve_id);

    let result = handler.inject_mock_cve(project_uuid, synthetic_cve).await?;

    if result.sla_met {
        println!(
            "Mock injection OK: cve={} e2e_latency={}ms SLA=MET",
            result.synthetic_cve_id, result.e2e_latency_ms
        );
        std::process::exit(0);
    } else {
        eprintln!(
            "MOCK INJECTION SLA VIOLATION: cve={} e2e_latency={}ms > 900000ms budget",
            result.synthetic_cve_id, result.e2e_latency_ms
        );
        std::process::exit(1);
    }
}
