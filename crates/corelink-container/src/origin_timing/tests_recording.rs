//! The two recording MECHANISMS, and where each one works.
//!
//! `timed` / `PhaseScope::enter` read the ambient task-local, so they record
//! only on the task the scope was opened on. `PhaseScope::with_handle` carries
//! a handle captured before the boundary, which is what a spawned region needs.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use super::tests_support::parse;
use super::{current_ledger, timed, Phase, PhaseLedger, PhaseScope, LEDGER};

#[tokio::test]
async fn timed_is_a_pass_through_when_no_ledger_is_in_scope() {
    // Every unit test and background task in the crate calls instrumented
    // code with no layer above it. Recording must never be load-bearing.
    let v = timed(Phase::Pat, async { 42_u8 }).await;
    assert_eq!(v, 42);
}

#[tokio::test]
async fn phase_scope_records_on_early_return_from_inside_it() {
    // The `oargon`/`opermit` wraps in `adapter_pat.rs` cover regions with
    // their own internal early returns. `PhaseScope` must record on THAT
    // exit path too, not only when the scoped block runs to completion.
    async fn region_with_early_return() -> u8 {
        let _scope = PhaseScope::enter(Phase::Argon);
        if true {
            return 7; // exits the function; the guard must still drop here
        }
        #[allow(unreachable_code)]
        0
    }

    let ledger = Arc::new(PhaseLedger::new());
    let v = LEDGER
        .scope(Arc::clone(&ledger), region_with_early_return())
        .await;
    assert_eq!(v, 7);
    assert!(
        ledger.micros(Phase::Argon).is_some(),
        "PhaseScope must record even when the scoped region exits via an \
         early `return` — Drop runs on every exit path, branch or not"
    );
}

#[tokio::test]
async fn phase_scope_with_handle_records_from_a_spawned_task() {
    // `opermit`'s acquire runs inside `FlightGroup::run`'s `tokio::spawn`ed
    // `lead` future, where the ambient task-local is not visible. Proves
    // the captured-handle path (`current_ledger` + `with_handle`) records
    // correctly from a genuinely different task, not just a nested future
    // on the same task.
    let ledger = Arc::new(PhaseLedger::new());
    let handle = LEDGER
        .scope(Arc::clone(&ledger), async { current_ledger() })
        .await;
    assert!(
        handle.is_some(),
        "current_ledger must see the scope it is called inside"
    );

    let spawned = tokio::spawn(async move {
        let _scope = PhaseScope::with_handle(handle, Phase::Permit);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    });
    spawned.await.expect("spawned task must not panic");

    assert!(
        ledger.micros(Phase::Permit).is_some(),
        "a captured-handle PhaseScope must record from the spawned task \
         it runs on, not just from the task that opened the ledger scope"
    );
}

#[tokio::test]
async fn current_ledger_is_none_with_no_scope_in_place() {
    // Mirrors `timed_is_a_pass_through_when_no_ledger_is_in_scope`: a
    // caller with no ledger in scope must get `None`, never panic.
    assert!(current_ledger().is_none());
}

#[tokio::test]
async fn both_recording_mechanisms_reach_the_same_ledger() {
    // ⚠️ READ THIS BEFORE TRUSTING THIS TEST AS A SECURITY PROOF — IT IS NOT ONE.
    //
    // `found_arm` and `not_found_arm` below are two hand-written stand-ins
    // that are DELIBERATELY identical. A test whose two sides are identical
    // by construction can only prove that identical code behaves
    // identically, which is a tautology. It would pass unchanged even if the
    // real row-FOUND and row-NOT-FOUND arms in `adapter_pat.rs` were
    // asymmetric — the exact defect it looks like it is guarding against.
    //
    // What it DOES prove, and the only reason it is worth keeping: the two
    // recording MECHANISMS agree. `PhaseScope::enter` reads the ambient
    // task-local, while `PhaseScope::with_handle` carries a handle captured
    // on the originating task across a `tokio::spawn` (which the real
    // Argon2id flights need, because `FlightGroup::run` spawns its lead
    // future onto a task that cannot see this task-local). Both must land in
    // the same ledger and yield the same phase-name set. That is a real
    // property, and it is the one asserted here.
    //
    // Arm symmetry in `adapter_pat.rs` itself is NOT enforced by this test.
    // It is enforced by (1) code review of the two call sites, and (2) the
    // `CORELINK_ORIGIN_TIMING_DETAIL` gate, which is off in production and
    // makes both arms trivially indistinguishable on the wire because
    // neither emits these phases at all. Test (2)'s coverage is
    // `detail_phases_are_absent_when_the_gate_is_off`.
    async fn found_arm() {
        let _argon = PhaseScope::enter(Phase::Argon);
        let handle = current_ledger();
        tokio::spawn(async move {
            let _permit = PhaseScope::with_handle(handle, Phase::Permit);
        })
        .await
        .unwrap();
    }
    async fn not_found_arm() {
        let _argon = PhaseScope::enter(Phase::Argon);
        let handle = current_ledger();
        tokio::spawn(async move {
            let _permit = PhaseScope::with_handle(handle, Phase::Permit);
        })
        .await
        .unwrap();
    }

    let found_ledger = Arc::new(PhaseLedger::new());
    LEDGER.scope(Arc::clone(&found_ledger), found_arm()).await;
    let not_found_ledger = Arc::new(PhaseLedger::new());
    LEDGER
        .scope(Arc::clone(&not_found_ledger), not_found_arm())
        .await;

    let found_names: std::collections::HashSet<String> =
        parse(&found_ledger.server_timing_value_with(1_000, true))
            .into_keys()
            .collect();
    let not_found_names: std::collections::HashSet<String> =
        parse(&not_found_ledger.server_timing_value_with(1_000, true))
            .into_keys()
            .collect();
    assert_eq!(
        found_names, not_found_names,
        "the found and not-found PAT arms must produce the identical SET \
         of phase names — a caller must not be able to distinguish them \
         by which phases are present on the wire"
    );
    assert!(found_names.contains("oargon"));
    assert!(found_names.contains("opermit"));
}
