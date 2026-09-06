//! The credential-path gate: which detail phases publish, and which stay
//! behind `CORELINK_ORIGIN_TIMING_DETAIL`.
//!
//! A security property, not a formatting one: `opermit`'s PRESENCE reports
//! whether the Argon2id flight ran for that exact credential.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use super::tests_support::parse;
use super::{Phase, PhaseLedger};

/// The gate is OFF by default and that default must keep the header exactly
/// as it was before the PAT-detail split existed: no `oargon`, no `opermit`,
/// their time represented by the explicit handler phase — while the two NEUTRAL
/// phases (`ortier`, `oaudit`) publish either way.
///
/// This is a security property, not a formatting preference — see
/// `detail_phases_enabled`. `opermit`'s presence reports whether the
/// Argon2id flight ran, which reports the state of a per-process cache on
/// the credential path. `ortier`/`oaudit` report neither, which is why
/// B-109 could enumerate them without arming the oracle.
#[test]
fn the_credential_phases_are_absent_when_the_gate_is_off() {
    let ledger = PhaseLedger::new();
    ledger.add(Phase::Pat, 10_000);
    ledger.add(Phase::Argon, 90_000);
    ledger.add(Phase::Permit, 5_000);
    ledger.add(Phase::Tier, 20_000);
    ledger.add(Phase::Audit, 15_000);

    let off = parse(&ledger.server_timing_value_with(200_000, false));
    assert!(
        !off.contains_key("oargon"),
        "oargon is on the credential path and must not ship by default"
    );
    assert!(
        !off.contains_key("opermit"),
        "opermit's PRESENCE is the oracle — it must not ship by default"
    );
    assert_eq!(
        off.get("ortier"),
        Some(&20),
        "ortier carries no credential oracle and is enumerated by default (B-109)"
    );
    assert_eq!(
        off.get("oaudit"),
        Some(&15),
        "oaudit carries no credential oracle and is enumerated by default (B-109)"
    );
    assert_eq!(
        off.get("opat"),
        Some(&10),
        "the pre-existing phases are untouched"
    );
    // 200 total - 10 opat - 20 ortier - 15 oaudit = 155; only the two
    // credential-path phases stay in the residue.
    assert_eq!(
        off.get("ohandler"),
        Some(&155),
        "only the gated credential phases are excluded from the public split"
    );

    // With the gate on, the same ledger partitions the very same total.
    let on = parse(&ledger.server_timing_value_with(200_000, true));
    assert_eq!(on.get("oargon"), Some(&90));
    assert_eq!(on.get("opermit"), Some(&5));
    assert_eq!(on.get("ortier"), Some(&20));
    assert_eq!(on.get("oaudit"), Some(&15));
    assert_eq!(on.get("ohandler"), Some(&60), "190 - 90 - 5 - 20 - 15 = 60");
}
