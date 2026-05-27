//! Property tests for `ClerkHealthLogic` — the pure-logic surface that
//! backs the wasm32 `ClerkHealthDo` Durable Object actor.
//!
//! WI-PROPTEST-FU-W33-002 closure for `corelink-clerk-cf`.
//!
//! Coverage map (each test pins a named invariant from
//! `src/clerk_health_do.rs`):
//!
//! - `prop_inv_parsed_route_tenant_scope_rejects_mismatch` — any input
//!   tenant segment that differs from the actor-anchored tenant id MUST
//!   be rejected with `HealthDoError::TenantScope`; the actor is anchored
//!   to a single tenant and `ParsedRoute::assert_tenant` is the
//!   constant-time enforcement boundary (CTRL-PRIV-001 defense-in-depth).
//! - `prop_inv_upsert_get_roundtrip_preserves_record` — for any valid
//!   `(correlation_id, note, now_ms)`, the value retrieved by `get` is
//!   byte-identical to the value returned by `upsert` (no field drift,
//!   no serialization loss; canonical state-store contract).
//! - `prop_inv_sweep_ttl_boundary_exact` — `sweep(now_ms)` removes a
//!   record iff `created_at_ms + ttl_ms < now_ms` (strict inequality;
//!   exact boundary survival mirrors the half-open Prometheus bucket
//!   convention also used in `corelink-billing-aggregator`).
//! - `prop_inv_audit_fail_closed_blocks_state_mutation` — for ANY
//!   random sequence of upsert/tombstone ops dispatched against a
//!   logic instance whose audit hook returns `Err`, the in-memory
//!   record map MUST remain empty (audit-emit-BEFORE fail-CLOSED;
//!   INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER canary).

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as proptest failures by design"
)]

use std::sync::Arc;

use corelink_clerk_cf::{
    AuditEvent, ClerkHealthLogic, ClerkHealthState, HealthDoError, HealthDoOp, HealthMethod,
    HealthRecord, ParsedRoute, RecordResponse, UpsertRequest, DEFAULT_TTL_MS,
};
use proptest::prelude::*;

// Re-import the audit hook type from the crate. Not re-exported, so we
// reconstruct it locally (same shape).
type AuditFn = Arc<
    dyn Fn(HealthDoOp, &str) -> Result<(), HealthDoError> + Send + Sync + 'static,
>;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 256 for
/// the PR gate; CI nightly + the `PROPTEST_CASES=256` stress run in §3
/// of the WI override.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(256)
}

/// Tenant id strategy — alphanumeric only, no forbidden bytes (`:`,
/// `/`, NUL, whitespace) so `ClerkHealthLogic::new` accepts it.
fn tenant_id_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9]{1,32}".prop_filter("non-empty", |s| !s.is_empty())
}

/// Correlation id strategy — same alphabet, ≤ 64 chars (well under the
/// 256-byte cap enforced by `HealthRecord::new`).
fn cid_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_-]{1,64}".prop_filter("non-empty", |s| !s.is_empty())
}

/// Note strategy — printable ASCII, ≤ 256 bytes (under the 1024-byte cap).
fn note_strategy() -> impl Strategy<Value = String> {
    "[ -~]{0,256}"
}

fn allow_audit() -> AuditFn {
    Arc::new(|_op, _subject| Ok(()))
}

fn deny_audit() -> AuditFn {
    Arc::new(|_op, _subject| Err(HealthDoError::AuditDenied("policy".to_owned())))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: proptest_cases(), .. ProptestConfig::default() })]

    /// INV-CLERK-TENANT-SCOPE: any URL tenant segment that differs
    /// byte-for-byte from the actor-anchored tenant id MUST trigger
    /// `HealthDoError::TenantScope`. Asserts BOTH directions: matching
    /// tenants pass, non-matching fail.
    #[test]
    fn prop_inv_parsed_route_tenant_scope_rejects_mismatch(
        actor_tenant in tenant_id_strategy(),
        url_tenant in tenant_id_strategy(),
        cid in cid_strategy(),
    ) {
        let path = format!("/record/{url_tenant}/{cid}");
        let route = ParsedRoute::parse("GET", &path)
            .expect("alphanumeric segments always parse");

        let res = route.assert_tenant(&actor_tenant);
        if actor_tenant == url_tenant {
            // Same tenant ⇒ Ok.
            prop_assert!(res.is_ok(), "same-tenant must accept: {res:?}");
        } else {
            // Different tenant ⇒ TenantScope error variant. We do NOT
            // use `matches!(..., Variant { .. })` per S-08 P1-1; we
            // pattern-bind and assert the message contains both ids.
            let err = res.expect_err("mismatch must reject");
            match err {
                HealthDoError::TenantScope(msg) => {
                    prop_assert!(
                        msg.contains(&url_tenant) && msg.contains(&actor_tenant),
                        "tenant_scope error MUST cite both URL + actor ids: {msg}"
                    );
                }
                other => prop_assert!(
                    false,
                    "expected TenantScope, got {other:?}"
                ),
            }
        }
    }

    /// INV-CLERK-ROUNDTRIP: `upsert(note, now_ms)` then `get` returns a
    /// record whose `correlation_id`, `note`, and `created_at_ms` are
    /// byte-equal to the upserted record. No field drift across the
    /// state-store boundary.
    #[test]
    fn prop_inv_upsert_get_roundtrip_preserves_record(
        tenant in tenant_id_strategy(),
        cid in cid_strategy(),
        note in note_strategy(),
        now_ms in 0i64..=i64::MAX / 4,
    ) {
        let mut logic = ClerkHealthLogic::new(&tenant)
            .expect("alphanumeric tenant accepted")
            .with_audit(allow_audit());

        let post_route = ParsedRoute::parse(
            "POST",
            &format!("/record/{tenant}/{cid}"),
        ).expect("happy parse");

        let written = logic
            .upsert(&post_route, note.clone(), now_ms)
            .expect("upsert under allow_audit");

        // Explicit field-by-field equality — NOT matches! (S-08 P1-1).
        prop_assert_eq!(written.correlation_id.as_str(), cid.as_str());
        prop_assert_eq!(written.note.as_str(), note.as_str());
        prop_assert_eq!(written.created_at_ms, now_ms);

        let get_route = ParsedRoute::parse(
            "GET",
            &format!("/record/{tenant}/{cid}"),
        ).expect("happy parse");
        let read = logic.get(&get_route).expect("get after upsert");

        prop_assert_eq!(read.correlation_id.as_str(), cid.as_str());
        prop_assert_eq!(read.note.as_str(), note.as_str());
        prop_assert_eq!(read.created_at_ms, now_ms);

        // RecordResponse projection is byte-identical too — the wire
        // format mirrors the storage shape (canonical map).
        let resp = RecordResponse::from(read);
        prop_assert_eq!(resp.correlation_id.as_str(), cid.as_str());
        prop_assert_eq!(resp.note.as_str(), note.as_str());
        prop_assert_eq!(resp.created_at_ms, now_ms);
    }

    /// INV-CLERK-SWEEP-BOUNDARY: a record is swept iff its expiry
    /// instant (`created_at_ms + ttl_ms`) is STRICTLY less than `now_ms`
    /// at sweep time. The exact boundary `created_at_ms + ttl_ms ==
    /// now_ms` survives (half-open convention).
    #[test]
    fn prop_inv_sweep_ttl_boundary_exact(
        tenant in tenant_id_strategy(),
        cid in cid_strategy(),
        created_at_ms in 0i64..=1_000_000_000_000i64,
        ttl_ms in 1i64..=DEFAULT_TTL_MS * 4,
        offset_from_expiry in -10i64..=10i64,
    ) {
        let state = ClerkHealthState::new().with_ttl_ms(ttl_ms);
        let mut logic = ClerkHealthLogic::new(&tenant)
            .expect("ctor")
            .with_audit(allow_audit())
            .with_state(state);

        let post_route = ParsedRoute::parse(
            "POST",
            &format!("/record/{tenant}/{cid}"),
        ).expect("parse");
        logic
            .upsert(&post_route, "x", created_at_ms)
            .expect("upsert");

        // Sweep at `expiry + offset`, where expiry = created_at_ms + ttl.
        let expiry = created_at_ms.saturating_add(ttl_ms);
        let now_ms = expiry.saturating_add(offset_from_expiry);

        let removed = logic.sweep(now_ms).expect("sweep");

        // Source-of-truth predicate from `sweep()`:
        //   removed iff created_at_ms.saturating_add(ttl) < now_ms
        let should_remove =
            created_at_ms.saturating_add(ttl_ms) < now_ms;

        if should_remove {
            prop_assert_eq!(removed, 1, "expected removal");
            prop_assert!(
                !logic.state().records().contains_key(&cid),
                "record must be swept"
            );
        } else {
            prop_assert_eq!(removed, 0, "must NOT remove at/before boundary");
            prop_assert!(
                logic.state().records().contains_key(&cid),
                "record must survive at/before boundary"
            );
        }
    }

    /// INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER: a failing audit hook MUST
    /// short-circuit every mutation. For ANY random sequence of
    /// upsert/tombstone ops dispatched against a logic instance whose
    /// audit hook returns Err, the in-memory record map remains empty.
    #[test]
    fn prop_inv_audit_fail_closed_blocks_state_mutation(
        tenant in tenant_id_strategy(),
        // Each op is (is_upsert?, cid, now_ms). Bound length to keep
        // the inner loop O(N) tractable under PROPTEST_CASES=256.
        ops in proptest::collection::vec(
            (any::<bool>(), cid_strategy(), 0i64..1_000_000i64),
            0..16,
        ),
    ) {
        let mut logic = ClerkHealthLogic::new(&tenant)
            .expect("ctor")
            .with_audit(deny_audit());

        for (is_upsert, cid, now_ms) in &ops {
            let path = format!("/record/{tenant}/{cid}");
            if *is_upsert {
                let route = ParsedRoute::parse("POST", &path).expect("parse");
                let err = logic
                    .upsert(&route, "note", *now_ms)
                    .expect_err("audit must fail-CLOSED");
                match err {
                    HealthDoError::AuditDenied(_) => {}
                    other => prop_assert!(
                        false,
                        "expected AuditDenied, got {other:?}"
                    ),
                }
            } else {
                let route = ParsedRoute::parse("DELETE", &path).expect("parse");
                let err = logic
                    .tombstone(&route)
                    .expect_err("audit must fail-CLOSED");
                match err {
                    HealthDoError::AuditDenied(_) => {}
                    other => prop_assert!(
                        false,
                        "expected AuditDenied, got {other:?}"
                    ),
                }
            }
        }

        // After ANY random fail-CLOSED sequence the state stays empty.
        prop_assert!(
            logic.state().records().is_empty(),
            "audit fail-CLOSED MUST keep state empty across {} ops",
            ops.len()
        );
    }
}

// ---------------------------------------------------------------------------
// Sanity checks pinning the type aliases used in the proptests above.
// ---------------------------------------------------------------------------

#[test]
fn sanity_audit_event_variants_present() {
    // Reference the re-exported public surface so the test fails fast
    // if `lib.rs` drops a re-export (downstream-breakage canary).
    let _: HealthMethod = HealthMethod::Get;
    let _: HealthDoOp = HealthDoOp::Upsert;
    let _: HealthDoOp = HealthDoOp::Tombstone;
    // AuditEvent is the re-exported audit-emit envelope; we just verify
    // the type-name resolves (a structural canary, not a value check).
    fn _accepts_audit_event(_e: AuditEvent) {}
    fn _accepts_upsert_req(_r: UpsertRequest) {}
    fn _accepts_health_record(_r: HealthRecord) {}
}
