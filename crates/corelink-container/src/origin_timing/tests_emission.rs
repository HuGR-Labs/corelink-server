//! What the ledger EMITS: which phases appear, which are omitted, and the
//! arithmetic that keeps named phases plus `ohandler` equal to the container total.
//!
//! This is the partition property — the one that makes `ohandler` explicit
//! rather than a guess — plus the re-entry hazard that would break it.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use super::tests_support::parse;
use super::{Phase, PhaseLedger};

#[test]
fn a_phase_that_never_ran_is_omitted_and_one_that_ran_is_emitted_at_zero() {
    let ledger = PhaseLedger::new();
    ledger.add(Phase::Pat, 0);
    let parsed = parse(&ledger.server_timing_value_with(5_000, true));
    assert_eq!(
        parsed.get("opat"),
        Some(&0),
        "a phase that RAN must be reported even at dur=0, or `fast` and \
         `skipped` become the same observation on the wire"
    );
    assert!(!parsed.contains_key("oquota"));
    assert!(!parsed.contains_key("ostore"));
    assert!(!parsed.contains_key("oargon"));
    assert!(!parsed.contains_key("opermit"));
    assert!(!parsed.contains_key("ortier"));
    assert!(!parsed.contains_key("oaudit"));
}

#[test]
fn emitted_phases_sum_exactly_to_the_container_total() {
    let ledger = PhaseLedger::new();
    ledger.add(Phase::Pat, 3_400);
    ledger.add(Phase::Quota, 118_900);
    ledger.add(Phase::Store, 900);
    let wire = ledger.server_timing_value_with(140_250, true);
    assert!(wire.contains("ohandler;dur=19"));
    assert!(wire.contains("oother;dur=19;desc=\"legacy-alias\""));
    let parsed = parse(&wire);
    let sum: i64 = parsed.values().sum();
    assert_eq!(
        sum, 140,
        "the container sub-phases must account for the whole 140ms the \
         container held the request; they summed to {sum}. Split: {parsed:?}"
    );
    assert_eq!(parsed["opat"], 3);
    assert_eq!(parsed["oquota"], 118);
    assert_eq!(parsed["ostore"], 0);
    // 140 - (3 + 118 + 0): framework time is explicitly named `ohandler`.
    assert_eq!(parsed["ohandler"], 19);
}

#[test]
fn accounting_is_emitted_as_a_disjoint_storage_phase() {
    // B-107 needs to answer whether a slow PUT is R2 or D1 accounting. These
    // are separate phase windows, not two labels over one broad Store scope.
    let ledger = PhaseLedger::new();
    ledger.add(Phase::Store, 30_000);
    ledger.add(Phase::Accounting, 10_000);
    let parsed = parse(&ledger.server_timing_value_with(50_000, true));
    assert_eq!(parsed["ostore"], 30);
    assert_eq!(parsed["oaccounting"], 10);
    assert_eq!(parsed["ohandler"], 10);
    assert_eq!(parsed.values().sum::<i64>(), 50);
}

#[test]
fn all_named_phases_plus_handler_sum_exactly_to_the_container_total() {
    // W2/W3: opat + oquota + ostore + oargon + opermit + ortier + oaudit +
    // ohandler must reconcile exactly against the container's own
    // whole-request clock, the same way the original three did.
    let ledger = PhaseLedger::new();
    ledger.add(Phase::Pat, 3_400);
    ledger.add(Phase::Quota, 20_100);
    ledger.add(Phase::Store, 900);
    ledger.add(Phase::Argon, 61_700);
    ledger.add(Phase::Permit, 12_300);
    ledger.add(Phase::Tier, 4_600);
    ledger.add(Phase::Audit, 8_500);
    let parsed = parse(&ledger.server_timing_value_with(140_250, true));
    let sum: i64 = parsed.values().sum();
    assert_eq!(
        sum, 140,
        "named phases + ohandler must equal \
         the 140ms the container held the request; they summed to {sum}. \
         Split: {parsed:?}"
    );
    assert_eq!(parsed["opat"], 3);
    assert_eq!(parsed["oquota"], 20);
    assert_eq!(parsed["ostore"], 0);
    assert_eq!(parsed["oargon"], 61);
    assert_eq!(parsed["opermit"], 12);
    assert_eq!(parsed["ortier"], 4);
    assert_eq!(parsed["oaudit"], 8);
    // 140 - (3 + 20 + 0 + 61 + 12 + 4 + 8) = 32, framework work.
    assert_eq!(parsed["ohandler"], 32);
}

#[test]
fn a_phase_that_never_ran_among_the_new_three_is_omitted_and_one_that_ran_is_emitted() {
    let ledger = PhaseLedger::new();
    ledger.add(Phase::Argon, 0);
    let parsed = parse(&ledger.server_timing_value_with(5_000, true));
    assert_eq!(
        parsed.get("oargon"),
        Some(&0),
        "oargon must be reported even at dur=0, exactly like the existing \
         three phases — see the not-found/found parity requirement"
    );
    assert!(!parsed.contains_key("opermit"));
    assert!(!parsed.contains_key("ortier"));
    assert!(!parsed.contains_key("oaudit"));
    assert!(!parsed.contains_key("opat"));
    assert!(!parsed.contains_key("oquota"));
    assert!(!parsed.contains_key("ostore"));
}

#[test]
fn a_phase_that_never_ran_among_audit_is_omitted_and_one_that_ran_is_emitted() {
    let ledger = PhaseLedger::new();
    ledger.add(Phase::Audit, 0);
    let parsed = parse(&ledger.server_timing_value_with(5_000, true));
    assert_eq!(
        parsed.get("oaudit"),
        Some(&0),
        "oaudit must be reported even at dur=0, exactly like the other \
         detail phases"
    );
    assert!(!parsed.contains_key("oargon"));
    assert!(!parsed.contains_key("opermit"));
    assert!(!parsed.contains_key("ortier"));
    assert!(!parsed.contains_key("opat"));
    assert!(!parsed.contains_key("oquota"));
    assert!(!parsed.contains_key("ostore"));
}

#[test]
fn repeated_entries_into_one_phase_add_rather_than_overwrite() {
    let ledger = PhaseLedger::new();
    ledger.add(Phase::Pat, 1_500);
    ledger.add(Phase::Pat, 2_500);
    assert_eq!(ledger.micros(Phase::Pat), Some(4_000));
}

#[test]
fn residue_is_clamped_rather_than_reported_negative() {
    let ledger = PhaseLedger::new();
    ledger.add(Phase::Quota, 50_000);
    let parsed = parse(&ledger.server_timing_value_with(10_000, true));
    assert_eq!(parsed["ohandler"], 0);
}

/// **A nested re-entry of the SAME phase is one wall-clock window.**
///
/// The outer scope may live on the request task while an inner scope runs in
/// a `spawn_blocking` task with the same ledger handle. Both scopes describe
/// one storage window, so only the outermost scope owns the phase clock. This
/// keeps `Server-Timing` additive instead of allowing a clamp to hide
/// an overcount.
#[tokio::test]
async fn nested_reentry_of_the_same_phase_records_once() {
    let ledger = std::sync::Arc::new(PhaseLedger::new());
    let outer = super::PhaseScope::with_handle(Some(std::sync::Arc::clone(&ledger)), Phase::Store);
    assert_eq!(ledger.active_depth_for_test(Phase::Store), 1);

    // Exercise the boundary that caused B-122: the inner scope must carry the
    // same ledger explicitly because `spawn_blocking` does not inherit the
    // request task-local. A same-task nested scope would not prove this.
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let inner = tokio::task::spawn_blocking({
        let ledger = std::sync::Arc::clone(&ledger);
        move || {
            let inner = super::PhaseScope::with_handle(Some(ledger), Phase::Store);
            entered_tx
                .send(())
                .expect("the parent must still be waiting for the inner scope");
            std::thread::sleep(std::time::Duration::from_millis(2));
            drop(inner);
        }
    });
    entered_rx
        .await
        .expect("the blocking task must enter the shared Store window");
    assert_eq!(ledger.active_depth_for_test(Phase::Store), 2);
    assert_eq!(ledger.completed_windows_for_test(Phase::Store), 0);
    assert_eq!(ledger.recordings_for_test(Phase::Store), 0);
    inner.await.expect("the blocking task must not panic");
    assert_eq!(ledger.active_depth_for_test(Phase::Store), 1);
    drop(outer);

    assert_eq!(ledger.completed_windows_for_test(Phase::Store), 1);
    assert_eq!(ledger.recordings_for_test(Phase::Store), 1);
    assert_eq!(ledger.active_depth_for_test(Phase::Store), 0);
    assert!(ledger.micros(Phase::Store).is_some());
}

/// Named phases plus `ohandler` sum to the total,
/// with the gate OFF as well as ON. This explicit phase is what makes
/// rather than a guess, and splitting the gate must not break it.
#[test]
fn phases_plus_residue_sum_to_the_total_on_both_sides_of_the_gate() {
    let ledger = PhaseLedger::new();
    ledger.add(Phase::Pat, 10_000);
    ledger.add(Phase::Argon, 90_000);
    ledger.add(Phase::Permit, 5_000);
    ledger.add(Phase::Tier, 20_000);
    ledger.add(Phase::Audit, 15_000);

    for detail in [false, true] {
        let parsed = parse(&ledger.server_timing_value_with(200_000, detail));
        let sum: i64 = parsed.values().copied().sum();
        assert_eq!(
            sum, 200,
            "named phases + ohandler must equal the container total (detail={detail})"
        );
    }
}
