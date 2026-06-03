//! Drive the manual admin trigger envelope: 403 when scope is missing,
//! 501 when env != staging/dev, 200 otherwise.

#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "example: prints + ergonomic unwraps for clarity"
)]

use corelink_gc::{admin_trigger, AdminTriggerOutcome, GcRegion, InMemoryGcAuditSink};
use uuid::Uuid;

fn main() {
    let audit = InMemoryGcAuditSink::new();
    let tenant = Uuid::from_u128(0xc0ffee);

    // 403 path.
    let forbidden = admin_trigger(
        false,
        "pat_aaaa",
        "staging",
        tenant,
        GcRegion::Sam,
        &audit,
        100,
        1,
    )
    .unwrap();
    println!("forbidden: {forbidden:?}");
    assert!(matches!(forbidden, AdminTriggerOutcome::Forbidden { .. }));

    // 501 path.
    let prod_block = admin_trigger(
        true,
        "pat_aaaa",
        "prod",
        tenant,
        GcRegion::Sam,
        &audit,
        200,
        2,
    )
    .unwrap();
    println!("prod_block: {prod_block:?}");
    assert!(matches!(
        prod_block,
        AdminTriggerOutcome::NotImplemented { .. }
    ));

    // 200 path.
    let admitted = admin_trigger(
        true,
        "pat_aaaa",
        "staging",
        tenant,
        GcRegion::Sam,
        &audit,
        300,
        3,
    )
    .unwrap();
    println!("admitted: {admitted:?}");
    assert!(matches!(admitted, AdminTriggerOutcome::Admitted { .. }));

    println!("audit events emitted total: {}", audit.snapshot().len(),);
}
