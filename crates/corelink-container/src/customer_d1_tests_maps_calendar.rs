// ── Frozen maps ──────────────────────────────────────────────────────────

#[test]
fn billing_status_map_is_frozen() {
    // The FROZEN table from the WP-3 design.
    assert_eq!(map_billing_status(Some("paid")), "active");
    assert_eq!(map_billing_status(Some("past_due")), "past_due");
    assert_eq!(map_billing_status(Some("canceled")), "canceled");
    assert_eq!(map_billing_status(Some("incomplete")), "past_due");
    assert_eq!(map_billing_status(Some("inactive")), "inactive");
    assert_eq!(map_billing_status(None), "inactive"); // no-row
    assert_eq!(map_billing_status(Some("weird")), "inactive");
}

/// AUDIT REV-S5 (known-limitation guard): the dashboard status the map
/// emits MUST stay inside the frozen cross-team union
/// (`apps/admin-ui/src/lib/customer-types.ts`:
/// `"trialing" | "active" | "past_due" | "canceled" | "inactive"`) and
/// the `tenant_billing.status` CHECK in
/// `specs/03_architecture/data_model.md` (`active`/`past_due`/`canceled`).
/// In particular `incomplete` (pending FIRST payment) is collapsed onto
/// `past_due` ON PURPOSE — there is no `pending`/`awaiting_payment` value
/// in the frozen contract yet. If a future change starts emitting a new
/// string from this map, it MUST widen that union + the spec CHECK in the
/// SAME change; this test is the tripwire that forces that coordination.
#[test]
fn billing_status_map_stays_in_frozen_dashboard_union() {
    const FROZEN_DASHBOARD_STATUSES: &[&str] =
        &["trialing", "active", "past_due", "canceled", "inactive"];
    for d1_status in [
        Some("paid"),
        Some("past_due"),
        Some("incomplete"),
        Some("canceled"),
        Some("inactive"),
        Some("unrecognized_future_value"),
        None,
    ] {
        let mapped = map_billing_status(d1_status);
        assert!(
            FROZEN_DASHBOARD_STATUSES.contains(&mapped),
            "map_billing_status({d1_status:?}) = {mapped:?} is OUTSIDE the \
                 frozen dashboard status union {FROZEN_DASHBOARD_STATUSES:?} \
                 (REV-S5): widen apps/admin-ui customer-types.ts + the \
                 data_model.md CHECK in the same change before emitting it",
        );
    }
    // The specific REV-S5 collapse is intentional and asserted here so
    // the deferral is explicit, not accidental.
    assert_eq!(
        map_billing_status(Some("incomplete")),
        "past_due",
        "REV-S5: 'incomplete' deliberately collapses onto 'past_due' until \
             a distinct 'pending' status is added to the frozen contract",
    );
}

#[test]
fn scope_map_is_frozen() {
    assert_eq!(
        map_requested_scopes(&["cache:read".to_owned()]).unwrap(),
        "read-only"
    );
    assert_eq!(
        map_requested_scopes(&["cache:read".to_owned(), "cache:write".to_owned()]).unwrap(),
        "read-write"
    );
    assert_eq!(
        map_requested_scopes(&["cas:rw".to_owned()]).unwrap(),
        "read-write"
    );
    // Least privilege for an empty request.
    assert_eq!(map_requested_scopes(&[]).unwrap(), "read-only");
    // ADR-0071: find-ONLY stores the CHECK-safe base "read-only" + the
    // `find_only` marker (NOT a 4th `pat.scope` value — the 0037 CHECK forbids
    // it); combined with read/write it folds into the superset (read ⊇ find).
    assert_eq!(
        map_requested_scopes(&["cache:find-missing".to_owned()]).unwrap(),
        "read-only"
    );
    assert!(mint_is_find_only(&["cache:find-missing".to_owned()]));
    assert_eq!(
        map_requested_scopes(&["cache:read".to_owned(), "cache:find-missing".to_owned()]).unwrap(),
        "read-only"
    );
    // find + read is NOT find-only (it's a full read grant).
    assert!(!mint_is_find_only(&[
        "cache:read".to_owned(),
        "cache:find-missing".to_owned()
    ]));
    assert!(!mint_is_find_only(&["cache:read".to_owned()]));
    // Display: a find-only PAT surfaces as cache:find-missing (via the marker),
    // a normal read PAT as cache:read.
    assert_eq!(
        scope_to_list("read-only", true),
        vec!["cache:find-missing".to_owned()]
    );
    assert_eq!(
        scope_to_list("read-only", false),
        vec!["cache:read".to_owned()]
    );
    // 'admin' is NEVER grantable.
    let err = map_requested_scopes(&["admin".to_owned()]).unwrap_err();
    assert!(
        matches!(err, CustomerHandlerError::Unauthorized(_)),
        "{err:?}"
    );
    // rt-nuclear #15 REGRESSION: a substring-y token like "writes" must NOT
    // silently persist `read-write` (the old `s.contains("write")` mapper bug
    // that let a read-only PAT self-escalate). It is unrecognized ⇒ REJECTED
    // (fail-CLOSED) — this is where the escalation is actually closed, since
    // the mint gate intentionally treats unknown tokens as non-write.
    for evil in ["writes", "cache:write-x", "my-write"] {
        let err = map_requested_scopes(&[evil.to_owned()]).unwrap_err();
        assert!(
            matches!(err, CustomerHandlerError::Unauthorized(_)),
            "{evil:?} must be rejected, not mapped to read-write; got {err:?}"
        );
    }
}

// ── Calendar helpers ─────────────────────────────────────────────────────

#[test]
fn ms_to_iso8601_known_instants() {
    assert_eq!(ms_to_iso8601(0), "1970-01-01T00:00:00Z");
    assert_eq!(
        ms_to_iso8601(i64::try_from(NOW_MS).unwrap()),
        "2023-11-14T22:13:20Z"
    );
}

#[test]
fn period_from_ms_known_instants() {
    assert_eq!(period_from_ms(0), "1970-01");
    assert_eq!(period_from_ms(NOW_MS), "2023-11");
}

// ── Overview ─────────────────────────────────────────────────────────────
