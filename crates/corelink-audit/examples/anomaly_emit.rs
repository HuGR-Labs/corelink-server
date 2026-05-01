//! Example — emit `auth.anomaly.cross_region_burst` (SEV-1) and show
//! the dual-fan-out hint via `AuthEventType::is_sev1()`.
//!
//! Run with `cargo run --example anomaly_emit -p corelink-audit`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    reason = "example program — pretty-printing to stdout is the point"
)]

use corelink_audit::{
    compute_content_hash, AuthEvent, AuthEventData, AuthEventType, Emitter, InMemoryEmitter,
    PrincipalIdHash, RegionTag, RequestId, RetentionHint, TenantId,
};
use uuid::Uuid;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event = AuthEvent::new(
        AuthEventType::CrossRegionBurst,
        "corelink://wnam/auth/anomaly_detector",
        TenantId::from_uuid(Uuid::nil()),
        PrincipalIdHash::derive("user_2NkX8a3Bq")?,
        RegionTag::Wnam,
        RequestId::new("req_anomaly_42"),
        RetentionHint::Enterprise7y,
        1_700_000_000_000,
        AuthEventData::CrossRegionBurst { distinct_regions: 4 },
    );

    println!("Event type        : {}", event.event_type);
    println!("SEV-1 (dual fan-out): {}", event.event_type.is_sev1());
    println!("Subject (tenant)  : {}", event.subject());
    println!("content_hash      : {}", compute_content_hash(&event)?);

    let emitter = InMemoryEmitter::new();
    emitter.emit(event)?;
    println!("Emitter captured  : {} event(s)", emitter.len());
    Ok(())
}
