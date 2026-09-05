//! What the ledger EMITS: which phases appear, which are omitted, and the
//! arithmetic that keeps `sum(phases) + oother` equal to the container total.
//!
//! This is the partition property — the one that makes `oother` a residue
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
    let parsed = parse(&ledger.server_timing_value_with(140_250, true));
    let sum: i64 = parsed.values().sum();
    assert_eq!(
        sum, 140,
        "the container sub-phases must account for the whole 140ms the \
         container held the request; they summed to {sum}. Split: {parsed:?}"
    );
    assert_eq!(parsed["opat"], 3);
    assert_eq!(parsed["oquota"], 118);
    assert_eq!(parsed["ostore"], 0);
    // 140 - (3 + 118 + 0): the truncation residue lands in `oother`, which
    // is exactly where an unattributed millisecond is supposed to go.
    assert_eq!(parsed["oother"], 19);
}

#[test]
fn all_seven_named_phases_plus_oother_sum_exactly_to_the_container_total() {
    // W2/W3: opat + oquota + ostore + oargon + opermit + ortier + oaudit +
    // oother must reconcile exactly against the container's own
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
        "opat+oquota+ostore+oargon+opermit+ortier+oaudit+oother must equal \
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
    // 140 - (3 + 20 + 0 + 61 + 12 + 4 + 8) = 32, the truncation residue.
    assert_eq!(parsed["oother"], 32);
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
    assert_eq!(parsed["oother"], 0);
}

/// **A nested re-entry of the SAME phase is one wall-clock window.**
///
/// The outer scope may live on the request task while an inner scope runs in
/// a `spawn_blocking` task with the same ledger handle. Both scopes describe
/// one storage window, so only the outermost scope owns the phase clock. This
/// keeps `Server-Timing` additive instead of allowing `oother`'s clamp to hide
/// an overcount.
#[test]
fn nested_reentry_of_the_same_phase_records_once() {
    let ledger = std::sync::Arc::new(PhaseLedger::new());
    let outer = super::PhaseScope::with_handle(Some(std::sync::Arc::clone(&ledger)), Phase::Store);
    assert_eq!(ledger.active_depth_for_test(Phase::Store), 1);
    let inner = super::PhaseScope::with_handle(Some(std::sync::Arc::clone(&ledger)), Phase::Store);
    assert_eq!(ledger.active_depth_for_test(Phase::Store), 2);
    drop(inner);
    assert_eq!(ledger.completed_windows_for_test(Phase::Store), 0);
    assert_eq!(ledger.recordings_for_test(Phase::Store), 0);
    assert_eq!(ledger.active_depth_for_test(Phase::Store), 1);
    drop(outer);

    assert_eq!(ledger.completed_windows_for_test(Phase::Store), 1);
    assert_eq!(ledger.recordings_for_test(Phase::Store), 1);
    assert_eq!(ledger.active_depth_for_test(Phase::Store), 0);
    assert!(ledger.micros(Phase::Store).is_some());
}

/// The residue always closes: named phases plus `oother` sum to the total,
/// with the gate OFF as well as ON. This is what makes `oother` a residue
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
            "phases + oother must equal the container total (detail={detail})"
        );
    }
}
