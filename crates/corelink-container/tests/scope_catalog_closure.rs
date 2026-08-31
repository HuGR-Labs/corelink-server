//! B-080 — every canonical PAT scope NAME must be recognized by the
//! container's enforcement surface, or be a DECLARED exception.
//!
//! # The defect class this closes
//!
//! `corelink-pat` (`scopes.rs`) publishes a canonical scope catalog via
//! [`PatScopes::names`]. The container (`corelink_server::scope`) is the
//! enforcement point. Before this test, six of the catalog's names
//! (`admin:tenant-read`, `admin:tenant-write`, `admin:tokens`,
//! `admin:billing`, `admin:audit`, `admin:users`) existed ONLY in the
//! catalog: no mint path could request them, the D1 `pat.scope` CHECK
//! domain (`migrations/d1/0037`, `('read-write','read-only','admin')`)
//! could not store them, and no predicate ever read them. A least-privilege
//! plan written against those names was decorative.
//!
//! The instance is fixed by collapsing the six into the one name that is
//! actually mintable, storable and enforced (`admin`). **This test closes
//! the CLASS**: it goes RED if anyone re-adds a scope name to the catalog
//! that no enforcement predicate consults.
//!
//! # Declared exceptions (the ledger)
//!
//! [`UNENFORCED_BY_DESIGN`] is the explicit, reasoned ledger. It is NOT an
//! escape hatch to be widened casually: adding a name here is a deliberate,
//! reviewable act that records "this name grants nothing today". Every entry
//! must carry a reason. The ledger is itself asserted to be a SUBSET of the
//! live catalog, so a stale entry cannot silently mask a removed scope.

use corelink_pat::{PatScopes, SCOPE_KNOWN_MASK};
use corelink_server::scope::{
    classify_requested_scopes, requires_audit_read, requires_billing_admin, requires_cache_read,
    requires_cache_write, requires_find_missing,
};

/// Catalog names that are deliberately NOT enforced today, each with the
/// reason. Anything NOT in this list must be recognized by the enforcement
/// surface — see the module docs.
///
/// These three are forward-looking placeholders for capabilities that are
/// not built: there is no blob-delete customer surface and no Phase-2
/// REAPI executor. They are tracked separately from B-080 (which named
/// only the six `admin:*` scopes); they are recorded here so the debt is
/// VISIBLE rather than invisible, and so a NEW decorative scope is red.
const UNENFORCED_BY_DESIGN: &[(&str, &str)] = &[
    (
        "cache:delete",
        "no blob-delete route exists on any customer surface",
    ),
    (
        "execute:action",
        "Phase-2 REAPI execution is not built (no executor route)",
    ),
    (
        "report:result",
        "Phase-2 REAPI execution is not built (no executor route)",
    ),
];

/// Every way the container's enforcement surface can consult a scope token.
/// A name is "enforced" iff at least one of these consults it.
fn is_recognized_by_enforcement(name: &str) -> bool {
    // The mint-side classifier: does the self-serve grammar know this token?
    let mintable = classify_requested_scopes(&[name.to_owned()]).is_ok();
    // The data-plane predicates: does this token grant any capability?
    let grants = requires_cache_read(name)
        || requires_cache_write(name)
        || requires_find_missing(name)
        || requires_billing_admin(name)
        || requires_audit_read(name);
    mintable || grants
}

/// CALIBRATION: the recognizer must return TRUE for a token we KNOW is
/// enforced and FALSE for one we KNOW is not. Without this, a recognizer
/// that always returned `true` would make the real assertion vacuous.
#[test]
fn recognizer_is_calibrated() {
    // Positive controls — enforced today.
    for known_good in ["cache:r", "cache:w", "cache:find-missing", "admin"] {
        assert!(
            is_recognized_by_enforcement(known_good),
            "positive control `{known_good}` must be recognized — the recognizer is broken, \
             so any green result from this file is meaningless"
        );
    }
    // Negative control — a token nobody could ever have enforced.
    assert!(
        !is_recognized_by_enforcement("totally:invented-scope"),
        "negative control must NOT be recognized — the recognizer accepts everything, \
         so this file cannot detect a decorative scope"
    );
}

/// THE CLASS-CLOSER. Every canonical scope name is enforced, or declared.
#[test]
fn every_canonical_scope_name_is_enforced_or_declared() {
    let catalog = PatScopes::from_u64(SCOPE_KNOWN_MASK).names();
    assert!(
        !catalog.is_empty(),
        "catalog is empty — anti-vacuity: this test cannot pass by finding nothing to check"
    );

    let declared: Vec<&str> = UNENFORCED_BY_DESIGN.iter().map(|(n, _)| *n).collect();

    let decorative: Vec<&str> = catalog
        .iter()
        .copied()
        .filter(|n| !declared.contains(n))
        .filter(|n| !is_recognized_by_enforcement(n))
        .collect();

    assert!(
        decorative.is_empty(),
        "DECORATIVE SCOPE(S) in the canonical catalog — defined in \
         `corelink-pat/src/scopes.rs` but consulted by NO enforcement predicate in \
         `corelink-server::scope`: {decorative:?}.\n\
         A token bearing such a name gets exactly the privilege of one without it, so any \
         least-privilege policy written against it is decorative (B-080).\n\
         Fix one of two ways: (1) make an enforcement predicate consult the name, or \
         (2) remove the name from the catalog. If it must exist unenforced, add it to \
         `UNENFORCED_BY_DESIGN` WITH A REASON — a deliberate, reviewable act."
    );
}

/// The ledger may not outlive the catalog: a declared exception for a name
/// that no longer exists would silently excuse a future re-add of it.
#[test]
fn declared_exceptions_are_all_still_in_the_catalog() {
    let catalog = PatScopes::from_u64(SCOPE_KNOWN_MASK).names();
    for (name, reason) in UNENFORCED_BY_DESIGN {
        assert!(
            catalog.contains(name),
            "stale ledger entry `{name}` ({reason}) is no longer in the canonical catalog — \
             remove it, or it will silently excuse a future re-add"
        );
        assert!(
            !reason.trim().is_empty(),
            "ledger entry `{name}` must carry a non-empty reason"
        );
    }
}

/// B-080's explicit demand: PROVE DENIAL. The removed `admin:*` names must
/// be refused everywhere — they must not mint, and they must not open the
/// billing / account-PII surface (`routes::customer::billing_pii_gate_reject`
/// gates on [`requires_billing_admin`]).
#[test]
fn removed_admin_scope_names_are_denied_everywhere() {
    const REMOVED: &[&str] = &[
        "admin:tenant-read",
        "admin:tenant-write",
        "admin:tokens",
        "admin:billing",
        "admin:audit",
        "admin:users",
    ];

    // Calibration: the SAME assertions must PASS for the real `admin` token,
    // otherwise these predicates deny everything and prove nothing.
    assert!(
        requires_billing_admin("admin"),
        "control: the real `admin` token MUST open billing — else the denial below is vacuous"
    );
    assert!(
        classify_requested_scopes(&["cache:r".to_owned()]).is_ok(),
        "control: `cache:r` MUST classify — else the mint refusals below are vacuous"
    );

    for name in REMOVED {
        assert!(
            !requires_billing_admin(name),
            "`{name}` must NOT grant billing / account-PII access"
        );
        assert!(
            !requires_cache_read(name),
            "`{name}` must NOT grant cache read"
        );
        assert!(
            !requires_cache_write(name),
            "`{name}` must NOT grant cache write"
        );
        assert!(
            !requires_find_missing(name),
            "`{name}` must NOT grant find-missing"
        );
        assert!(
            classify_requested_scopes(&[(*name).to_owned()]).is_err(),
            "`{name}` must NOT be mintable via the self-serve classifier"
        );
        // And it must no longer be published as a canonical name at all.
        assert!(
            !PatScopes::from_u64(SCOPE_KNOWN_MASK).names().contains(name),
            "`{name}` is still published in the canonical scope catalog — it grants nothing, \
             so publishing it invites a decorative least-privilege plan (B-080)"
        );
    }
}
