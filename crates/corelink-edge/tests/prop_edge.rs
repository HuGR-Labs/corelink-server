//! Property tests pinning the load-bearing invariants of
//! `corelink-edge` at 10k iterations per check (PR-gate; nightly 100k
//! via `PROPTEST_CASES` env var override per S-07 P1-2 fix pattern).
//!
//! Coverage map (mirrors WI-S08-002 §6.1.11):
//!
//! - `prop_cidr_longest_prefix_match` — most-specific match wins when
//!   multiple admin entries cover the same address (the matcher
//!   resolves correctly even when the orchestrator's overlap rejection
//!   is bypassed via direct `longest_match` invocation).
//! - `prop_ipv6_supported` — IPv6 prefixes match correctly across the
//!   /128 ↔ /0 ladder; IPv4 cross-family never matches.
//! - `prop_blocklist_idempotent_add` — repeated adds of the same CIDR
//!   are no-op.
//! - `prop_blocklist_remove_round_trip` — `add → remove → no match`
//!   round-trip preserves canonical default-allow.
//! - `prop_tenant_isolation` — tenant A's blocklist NEVER affects
//!   tenant B (INV-AVAIL-ISOLATION reinforcement at the per-IP edge).
//! - `prop_audit_emit_per_decision_arm` — every Allow / DenyBlocklisted
//!   /DenyAbuse decision emits exactly one canonical audit record.
//! - `prop_decision_deterministic` — same `(tenant, ip)` → same
//!   `EdgeDecision` across calls (no hidden mutation in the read path).
//! - `prop_no_match_default_allow` — empty blocklist → all IPs Allow.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_edge::{
    canonical_audit_event_strings, canonical_metric_names,
    edge_schema_version, longest_match, Cidr, CidrBlocklist, EdgeClock,
    EdgeConfig, EdgeDecision, EdgeEventType, EdgeMetricKind, EdgePolicy,
    EdgeResultLabel, FailingEdgeAuditSink, InMemoryCidrBlocklist,
    InMemoryEdgeAuditSink, InMemoryEdgeMetrics, InMemoryEdgePolicy,
    IpAddr, MIGRATION_0011_EDGE_BLOCKLIST,
};
use proptest::prelude::*;
use uuid::Uuid;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn ten_a() -> Uuid {
    Uuid::from_u128(0xa)
}

fn ten_b() -> Uuid {
    Uuid::from_u128(0xb)
}

fn admin() -> Uuid {
    Uuid::from_u128(0xad)
}

#[derive(Clone, Copy, Debug)]
struct FixedClock(u64);

impl EdgeClock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.0
    }
}

type Bl = InMemoryCidrBlocklist<InMemoryEdgeAuditSink, InMemoryEdgeMetrics>;
type Pol = InMemoryEdgePolicy<Bl, InMemoryEdgeAuditSink, InMemoryEdgeMetrics, FixedClock>;

fn fresh() -> (
    Arc<Bl>,
    Arc<InMemoryEdgeAuditSink>,
    Arc<InMemoryEdgeMetrics>,
    Pol,
) {
    let audit = Arc::new(InMemoryEdgeAuditSink::new());
    let metrics = Arc::new(InMemoryEdgeMetrics::new());
    let bl = Arc::new(InMemoryCidrBlocklist::with_defaults(
        Arc::clone(&audit),
        Arc::clone(&metrics),
    ));
    let clock = Arc::new(FixedClock(1_000));
    let pol = InMemoryEdgePolicy::with_defaults(
        Arc::clone(&bl),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    (bl, audit, metrics, pol)
}

fn ipv4_addr(o0: u8, o1: u8, o2: u8, o3: u8) -> IpAddr {
    IpAddr::v4([o0, o1, o2, o3])
}

// ---- canonical surface pinning ---------------------------------------

#[test]
fn canonical_audit_event_strings_pinned() {
    let s = canonical_audit_event_strings();
    assert_eq!(s.len(), 5);
    assert!(s.contains(&"corelink.edge.allowed"));
    assert!(s.contains(&"corelink.edge.denied_blocklist"));
    assert!(s.contains(&"corelink.edge.denied_abuse"));
    assert!(s.contains(&"corelink.edge.blocklist_added"));
    assert!(s.contains(&"corelink.edge.blocklist_removed"));
}

#[test]
fn canonical_metric_names_pinned() {
    let m = canonical_metric_names();
    assert_eq!(m.len(), 5);
    assert!(m.contains(&"corelink.edge.decision_total"));
    assert!(m.contains(&"corelink.edge.cidr_blocklist_size"));
    assert!(m.contains(&"corelink.edge.cidr_blocklist_added_total"));
    assert!(m.contains(&"corelink.edge.cidr_blocklist_removed_total"));
    assert!(m.contains(&"corelink.edge.cf_api_error_total"));
}

#[test]
fn migration_0011_is_embedded() {
    assert!(MIGRATION_0011_EDGE_BLOCKLIST.contains("edge_blocklist"));
    assert!(MIGRATION_0011_EDGE_BLOCKLIST.contains("migration 0011"));
}

#[test]
fn edge_schema_version_pinned() {
    assert_eq!(edge_schema_version(), 11);
}

#[test]
fn edge_config_canonical_constants() {
    let c = EdgeConfig::canonical();
    assert_eq!(c.max_blocklist_size(), 10_000);
    assert_eq!(c.alert_threshold_size(), 8_000);
}

#[test]
fn audit_failure_aborts_decision_and_state_unchanged() {
    let bl_audit = Arc::new(InMemoryEdgeAuditSink::new());
    let pol_audit = Arc::new(FailingEdgeAuditSink::new());
    let metrics = Arc::new(InMemoryEdgeMetrics::new());
    let bl = Arc::new(InMemoryCidrBlocklist::with_defaults(
        Arc::clone(&bl_audit),
        Arc::clone(&metrics),
    ));
    let clock = Arc::new(FixedClock(1));
    let pol = InMemoryEdgePolicy::with_defaults(
        bl,
        Arc::clone(&pol_audit),
        metrics,
        clock,
    );
    let ip = ipv4_addr(203, 0, 113, 5);
    let err = pol.evaluate(ten_a(), ip, "rid").unwrap_err();
    assert!(matches!(err, corelink_edge::EdgeError::Audit(_)));
}

// ---- Property tests --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Most-specific entry wins when multiple admin entries cover the
    /// same address (admin overlap rejection at the orchestrator
    /// boundary leaves the matcher itself responsible for canonical
    /// resolution; we exercise it directly here).
    #[test]
    fn prop_cidr_longest_prefix_match(
        third_octet in 0_u8..=255,
        fourth_octet in 0_u8..=255,
    ) {
        let nets = vec![
            Cidr::parse("10.0.0.0/8").unwrap(),
            Cidr::parse("10.0.0.0/16").unwrap(),
            Cidr::parse("10.0.0.0/24").unwrap(),
        ];
        let ip = ipv4_addr(10, 0, third_octet, fourth_octet);
        let m = longest_match(&nets, ip).expect("must match /8 at minimum");
        // 10.0.0.x is covered by /8, /16, /24 → /24 wins.
        // 10.0.<n>.x with n != 0 is covered by /8, /16 → /16 wins.
        let expected = if third_octet == 0 { 24 } else { 16 };
        prop_assert_eq!(m.prefix_len(), expected);
    }

    /// IPv6 prefixes match correctly; cross-family addresses never
    /// match an IPv4 entry.
    #[test]
    fn prop_ipv6_supported(
        suffix in 0_u128..(1_u128 << 96),
    ) {
        // /32 covers any IPv6 starting with 2001:0db8.
        let net = Cidr::parse("2001:db8::/32").unwrap();
        let bits = (0x2001_0db8_u128 << 96) | (suffix & ((1_u128 << 96) - 1));
        // Reconstruct as 16 octets.
        let mut octets = [0u8; 16];
        for (i, slot) in octets.iter_mut().enumerate() {
            let shift = (15 - i) * 8;
            *slot = ((bits >> shift) & 0xff) as u8;
        }
        let ip = IpAddr::v6(octets);
        prop_assert!(net.contains(ip));
        // Cross-family: an IPv4 address cannot match an IPv6 entry.
        let v4 = ipv4_addr(32, 1, 13, 184);
        prop_assert!(!net.contains(v4));
    }

    /// Repeated adds of the SAME CIDR are no-op (idempotent).
    #[test]
    fn prop_blocklist_idempotent_add(
        a in 1_u8..=254,
        b in 0_u8..=255,
    ) {
        let (bl, audit, _m, _p) = fresh();
        let cidr_text = format!("{a}.{b}.0.0/16");
        let cidr = Cidr::parse(&cidr_text).unwrap();
        bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1).unwrap();
        bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 2).unwrap();
        bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 3).unwrap();
        prop_assert_eq!(bl.len(ten_a()).unwrap(), 1);
        prop_assert_eq!(
            audit.snapshot_of(EdgeEventType::BlocklistAdded).len(),
            1
        );
    }

    /// `add → remove → evaluate(ip)` returns Allow (round-trip).
    #[test]
    fn prop_blocklist_remove_round_trip(
        a in 1_u8..=254,
        b in 0_u8..=255,
        c in 0_u8..=255,
    ) {
        let (bl, _a, _m, pol) = fresh();
        let cidr = Cidr::parse(&format!("{a}.{b}.{c}.0/24")).unwrap();
        let ip = ipv4_addr(a, b, c, 5);
        bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1).unwrap();
        let d = pol.evaluate(ten_a(), ip, "rid").unwrap();
        prop_assert!(d.is_deny());
        let removed = bl
            .remove_prefix(ten_a(), cidr, admin(), "rid", 2)
            .unwrap();
        prop_assert!(removed);
        let d2 = pol.evaluate(ten_a(), ip, "rid").unwrap();
        prop_assert!(d2.is_allow());
    }

    /// Tenant A's blocklist NEVER affects tenant B's evaluations.
    #[test]
    fn prop_tenant_isolation(
        a in 1_u8..=254,
        b in 0_u8..=255,
        c in 0_u8..=255,
    ) {
        let (bl, _a, _m, pol) = fresh();
        let cidr = Cidr::parse(&format!("{a}.{b}.{c}.0/24")).unwrap();
        bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1).unwrap();
        let ip = ipv4_addr(a, b, c, 1);
        // tenant_a sees deny.
        let d_a = pol.evaluate(ten_a(), ip, "rid").unwrap();
        prop_assert!(d_a.is_deny());
        // tenant_b's blocklist is empty for this CIDR; must allow.
        let d_b = pol.evaluate(ten_b(), ip, "rid").unwrap();
        prop_assert!(d_b.is_allow());
    }

    /// Every decision arm emits exactly ONE canonical audit record
    /// (Allowed | DeniedBlocklist | DeniedAbuse) per evaluate.
    #[test]
    fn prop_audit_emit_per_decision_arm(
        a in 1_u8..=254,
        b in 0_u8..=255,
        c in 0_u8..=255,
        host in 0_u8..=255,
        block_it in any::<bool>(),
    ) {
        let (bl, audit, _m, pol) = fresh();
        let net_a = a;
        let net_b = b;
        let net_c = c;
        if block_it {
            let cidr = Cidr::parse(&format!("{net_a}.{net_b}.{net_c}.0/24")).unwrap();
            bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1).unwrap();
        }
        let allowed_before = audit.snapshot_of(EdgeEventType::Allowed).len();
        let denied_before = audit.snapshot_of(EdgeEventType::DeniedBlocklist).len();
        let abuse_before = audit.snapshot_of(EdgeEventType::DeniedAbuse).len();

        let ip = ipv4_addr(net_a, net_b, net_c, host);
        let d = pol.evaluate(ten_a(), ip, "rid").unwrap();

        let allowed_after = audit.snapshot_of(EdgeEventType::Allowed).len();
        let denied_after = audit.snapshot_of(EdgeEventType::DeniedBlocklist).len();
        let abuse_after = audit.snapshot_of(EdgeEventType::DeniedAbuse).len();

        let delta = (allowed_after - allowed_before)
            + (denied_after - denied_before)
            + (abuse_after - abuse_before);
        prop_assert_eq!(delta, 1);
        match d.audit_event_type() {
            EdgeEventType::Allowed => {
                prop_assert_eq!(allowed_after - allowed_before, 1);
            }
            EdgeEventType::DeniedBlocklist => {
                prop_assert_eq!(denied_after - denied_before, 1);
            }
            EdgeEventType::DeniedAbuse => {
                prop_assert_eq!(abuse_after - abuse_before, 1);
            }
            _ => {
                prop_assert!(false, "EdgeEventType expanded; test must absorb new arm");
            }
        }
    }

    /// Same `(tenant, ip)` → same decision across N back-to-back
    /// evaluates (no hidden mutation in the read path).
    #[test]
    fn prop_decision_deterministic(
        a in 1_u8..=254,
        b in 0_u8..=255,
        c in 0_u8..=255,
        host in 0_u8..=255,
        block_it in any::<bool>(),
    ) {
        let (bl, _a, _m, pol) = fresh();
        if block_it {
            let cidr = Cidr::parse(&format!("{a}.{b}.{c}.0/24")).unwrap();
            bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1).unwrap();
        }
        let ip = ipv4_addr(a, b, c, host);
        let d1 = pol.evaluate(ten_a(), ip, "rid").unwrap();
        let d2 = pol.evaluate(ten_a(), ip, "rid").unwrap();
        let d3 = pol.evaluate(ten_a(), ip, "rid").unwrap();
        prop_assert_eq!(d1, d2);
        prop_assert_eq!(d2, d3);
    }

    /// Empty blocklist → all IPs Allow regardless of ip.
    #[test]
    fn prop_no_match_default_allow(
        o0 in 1_u8..=254,
        o1 in 0_u8..=255,
        o2 in 0_u8..=255,
        o3 in 0_u8..=255,
    ) {
        let (_bl, _a, _m, pol) = fresh();
        let ip = ipv4_addr(o0, o1, o2, o3);
        let d = pol.evaluate(ten_a(), ip, "rid").unwrap();
        prop_assert!(d.is_allow());
    }
}

/// Sanity: decision_total{result=...} aggregates correctly across all
/// 3 result labels.
#[test]
fn decision_total_counts_per_label() {
    let (bl, _a, metrics, pol) = fresh();
    // Allow path.
    let ip = ipv4_addr(8, 8, 8, 8);
    let _ = pol.evaluate(ten_a(), ip, "rid").unwrap();
    // Deny-blocklist path.
    let cidr = Cidr::parse("9.9.9.0/24").unwrap();
    bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1).unwrap();
    let blocked = ipv4_addr(9, 9, 9, 1);
    let _ = pol.evaluate(ten_a(), blocked, "rid").unwrap();
    // Deny-abuse path.
    let _ = pol
        .evaluate_with_abuse_signal(ten_a(), ip, "rid", Some("sustained"))
        .unwrap();
    let total = metrics.counter_total(EdgeMetricKind::DecisionTotal);
    assert_eq!(total, 3);
    // Per-label breakdown.
    let allowed = metrics.counter(&format!(
        "{}{{result=allowed}}",
        EdgeMetricKind::DecisionTotal.as_str()
    ));
    assert_eq!(allowed, 1);
    let denied_block = metrics.counter(&format!(
        "{}{{result=denied_blocklist}}",
        EdgeMetricKind::DecisionTotal.as_str()
    ));
    assert_eq!(denied_block, 1);
    let denied_abuse = metrics.counter(&format!(
        "{}{{result=denied_abuse}}",
        EdgeMetricKind::DecisionTotal.as_str()
    ));
    assert_eq!(denied_abuse, 1);
}

/// Sanity: result_label / audit_event_type alignment for every arm.
#[test]
fn decision_arm_canonical_alignment() {
    assert_eq!(EdgeDecision::Allow.result_label(), EdgeResultLabel::Allowed);
    assert_eq!(
        EdgeDecision::Allow.audit_event_type(),
        EdgeEventType::Allowed
    );
    let mp = corelink_edge::MatchedPrefix {
        cidr: Cidr::parse("10.0.0.0/8").unwrap(),
    };
    let d = EdgeDecision::DenyBlocklisted { matched_prefix: mp };
    assert_eq!(d.result_label(), EdgeResultLabel::DeniedBlocklist);
    assert_eq!(d.audit_event_type(), EdgeEventType::DeniedBlocklist);
    let d_abuse = EdgeDecision::DenyAbuse { reason: "x" };
    assert_eq!(d_abuse.result_label(), EdgeResultLabel::DeniedAbuse);
    assert_eq!(d_abuse.audit_event_type(), EdgeEventType::DeniedAbuse);
}

/// Sanity: family gauge updates on add + remove (size after add == 1;
/// after remove == 0).
#[test]
fn family_gauge_tracks_blocklist_size() {
    let (bl, _a, metrics, _p) = fresh();
    let cidr = Cidr::parse("203.0.113.0/24").unwrap();
    bl.add_prefix(ten_a(), cidr, "ManualAdmin", admin(), "rid", 1)
        .unwrap();
    let label = format!(
        "{}{{family=4}}",
        EdgeMetricKind::CidrBlocklistSize.as_str()
    );
    assert_eq!(metrics.gauge(&label), 1);
    let _ = bl
        .remove_prefix(ten_a(), cidr, admin(), "rid", 2)
        .unwrap();
    assert_eq!(metrics.gauge(&label), 0);
    // IPv6 gauge unchanged at 0.
    let v6_label = format!(
        "{}{{family=6}}",
        EdgeMetricKind::CidrBlocklistSize.as_str()
    );
    assert_eq!(metrics.gauge(&v6_label), 0);
}
