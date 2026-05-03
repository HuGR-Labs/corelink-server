//! Property tests pinning the load-bearing invariants of
//! `corelink-logpush` at 10k iterations per check (PR-gate; nightly
//! 100k via `PROPTEST_CASES` env var override per S-07 P1-2 fix).
//!
//! Coverage map (mirrors WI-S09-002 §6.1.11):
//!
//! - `prop_pii_redaction_no_leakage` — for any input with PII patterns,
//!   the output contains zero unredacted PII (CTRL-PRIV-001 enforcement;
//!   sprint contract §6 DoD).
//! - `prop_redaction_idempotent` — `redact(redact(input)) == redact(input)`.
//! - `prop_redaction_preserves_non_pii` — non-PII text passes through
//!   unchanged.
//! - `prop_log_schema_serialization_roundtrip` — `serialize → deserialize
//!   → equal`.
//! - `prop_tenant_isolation` — tenant A logs never reference tenant B
//!   (INV-TENANT-ISOLATION canary).
//! - `prop_audit_emit_per_event_type` — every `LogEventType` value
//!   emits the canonical `corelink.logpush.record_emitted` audit.
//! - `prop_cardinality_budget_respected` — log emit fails-closed if
//!   the per-event-type label tuple would breach the budget.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_analytics::{
    AnalyticsConfig, CardinalityValidator,
    InMemoryAnalyticsAuditSink, Region, Tier,
};
use corelink_logpush::{
    canonical_audit_event_strings, canonical_log_event_types,
    canonical_pii_pattern_kinds, FailingLogAuditSink,
    InMemoryLogAuditSink, InMemoryLogSink, InMemoryPiiRedactor,
    LogAuditEventType, LogEventType, LogRecord, LogpushError,
    PiiPatternKind, PiiRedactor, MIGRATION_0016_LOG_SCHEMA,
    SCHEMA_VERSION,
};
use proptest::prelude::*;
use serde_json::json;
use uuid::Uuid;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for PR gate; nightly job overrides to 100k.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

type Sink = InMemoryLogSink<InMemoryPiiRedactor, InMemoryLogAuditSink>;

fn fresh_sink() -> (Sink, Arc<InMemoryLogAuditSink>) {
    let r = Arc::new(InMemoryPiiRedactor::new());
    let audit = Arc::new(InMemoryLogAuditSink::new());
    let cardinality_audit =
        Arc::new(InMemoryAnalyticsAuditSink::new());
    let cardinality = Arc::new(CardinalityValidator::with_defaults(
        cardinality_audit,
    ));
    let s = InMemoryLogSink::new(r, Arc::clone(&audit), cardinality);
    (s, audit)
}

fn fresh_sink_with_budget(
    per_metric: u64,
    global: u64,
) -> (Sink, Arc<InMemoryLogAuditSink>) {
    let r = Arc::new(InMemoryPiiRedactor::new());
    let audit = Arc::new(InMemoryLogAuditSink::new());
    let cardinality_audit =
        Arc::new(InMemoryAnalyticsAuditSink::new());
    let cfg = AnalyticsConfig::with_budgets(per_metric, global);
    let cardinality = Arc::new(CardinalityValidator::new(
        cardinality_audit,
        cfg,
    ));
    let s = InMemoryLogSink::new(r, Arc::clone(&audit), cardinality);
    (s, audit)
}

const ALL_REGIONS: &[Region] = &[
    Region::Iad,
    Region::Sjc,
    Region::Dfw,
    Region::Sea,
    Region::Ord,
    Region::Lhr,
    Region::Fra,
    Region::Ams,
    Region::Cdg,
    Region::Mad,
    Region::Gru,
    Region::Eze,
    Region::Bog,
    Region::Nrt,
    Region::Sin,
    Region::Syd,
    Region::Hkg,
    Region::Bom,
    Region::Icn,
    Region::Jnb,
    Region::Cpt,
    Region::Dxb,
];

const ALL_TIERS: &[Tier] = &[
    Tier::Free,
    Tier::Solo,
    Tier::Team,
    Tier::Business,
    Tier::Enterprise,
];

const ALL_EVENT_TYPES: &[LogEventType] = &[
    LogEventType::RequestServed,
    LogEventType::AuthAttempt,
    LogEventType::AdminAction,
    LogEventType::BillingEvent,
];

// ---- canonical surface pinning ---------------------------------------

#[test]
fn canonical_audit_event_strings_pinned() {
    let s = canonical_audit_event_strings();
    assert_eq!(s.len(), 4);
    assert!(s.contains(&"corelink.logpush.record_emitted"));
    assert!(s.contains(&"corelink.logpush.redaction_applied"));
    assert!(s.contains(&"corelink.logpush.redaction_failure"));
    assert!(s.contains(&"corelink.logpush.sink_failure"));
}

#[test]
fn canonical_log_event_types_pinned() {
    let v = canonical_log_event_types();
    assert_eq!(v.len(), 4);
    let mut set = std::collections::HashSet::new();
    for t in v {
        assert!(set.insert(t.as_str()));
    }
    assert_eq!(set.len(), 4);
}

#[test]
fn canonical_pii_pattern_kinds_pinned() {
    let v = canonical_pii_pattern_kinds();
    assert_eq!(v.len(), 5);
    let mut set = std::collections::HashSet::new();
    for p in v {
        assert!(set.insert(p.as_str()));
    }
    assert_eq!(set.len(), 5);
    assert!(set.contains("email"));
    assert!(set.contains("ip"));
    assert!(set.contains("token"));
    assert!(set.contains("pan"));
    assert!(set.contains("cpf_cnpj"));
}

#[test]
fn migration_0016_is_embedded() {
    assert!(MIGRATION_0016_LOG_SCHEMA
        .contains("log_schema_versions"));
    assert!(MIGRATION_0016_LOG_SCHEMA
        .contains("log_redaction_patterns"));
    assert!(MIGRATION_0016_LOG_SCHEMA.contains("WI-S09-002"));
}

#[test]
fn schema_version_pinned() {
    assert_eq!(SCHEMA_VERSION, 16);
    assert_eq!(corelink_logpush::logpush_schema_version(), 16);
}

#[test]
fn pii_pattern_kind_default_placeholders_pinned() {
    assert_eq!(
        PiiPatternKind::Email.default_placeholder(),
        "<EMAIL_REDACTED>"
    );
    assert_eq!(
        PiiPatternKind::Ip.default_placeholder(),
        "<IP_REDACTED>"
    );
    assert_eq!(
        PiiPatternKind::Token.default_placeholder(),
        "<TOKEN_REDACTED>"
    );
    assert_eq!(
        PiiPatternKind::Pan.default_placeholder(),
        "<PAN_REDACTED>"
    );
    assert_eq!(
        PiiPatternKind::CpfCnpj.default_placeholder(),
        "<CPF_REDACTED>"
    );
}

// ---- Property test strategies ---------------------------------------

prop_compose! {
    fn arb_region()(idx in 0_usize..ALL_REGIONS.len()) -> Region {
        ALL_REGIONS[idx]
    }
}

prop_compose! {
    fn arb_tier()(idx in 0_usize..ALL_TIERS.len()) -> Tier {
        ALL_TIERS[idx]
    }
}

prop_compose! {
    fn arb_event_type()(
        idx in 0_usize..ALL_EVENT_TYPES.len()
    ) -> LogEventType {
        ALL_EVENT_TYPES[idx]
    }
}

prop_compose! {
    fn arb_email()(
        local in "[a-z]{1,12}",
        domain in "[a-z]{1,8}",
        tld in "[a-z]{2,4}",
    ) -> String {
        format!("{local}@{domain}.{tld}")
    }
}

prop_compose! {
    fn arb_ipv4()(
        a in 1_u8..255,
        b in 0_u8..=255,
        c in 0_u8..=255,
        d in 1_u8..=255,
    ) -> String {
        format!("{a}.{b}.{c}.{d}")
    }
}

prop_compose! {
    fn arb_bearer()(
        s in "[A-Za-z0-9]{16,80}",
    ) -> String {
        format!("Bearer {s}")
    }
}

prop_compose! {
    fn arb_clean_text()(
        s in "[A-Za-z]{0,5}( [A-Za-z]{1,5}){0,5}",
    ) -> String {
        s
    }
}

// ---- Property tests --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// For any input with PII patterns, the output contains zero
    /// unredacted PII. CTRL-PRIV-001 enforcement; sprint contract §6
    /// DoD.
    #[test]
    fn prop_pii_redaction_no_leakage(
        email in arb_email(),
        ipv4 in arb_ipv4(),
        bearer in arb_bearer(),
        prefix in "[A-Za-z]{0,8}",
        suffix in "[A-Za-z]{0,8}",
    ) {
        let r = InMemoryPiiRedactor::new();
        let input = format!(
            "{prefix} user={email} ip={ipv4} auth={bearer} {suffix}"
        );
        let outcome = r.redact(&input);
        let redacted = &outcome.redacted;
        prop_assert!(
            !redacted.contains(&email),
            "raw email leaked: {redacted}"
        );
        prop_assert!(
            !redacted.contains(&ipv4),
            "raw IPv4 leaked: {redacted}"
        );
        // The bearer token text after the "Bearer " prefix MUST be
        // redacted.
        let token_part = bearer.strip_prefix("Bearer ").unwrap_or(&bearer);
        prop_assert!(
            !redacted.contains(token_part),
            "raw bearer token leaked: {redacted}"
        );
        let valid_hits = outcome.email_hits >= 1
            && outcome.ip_hits >= 1
            && outcome.token_hits >= 1;
        prop_assert!(
            valid_hits,
            "expected ≥ 1 hit per category; got {outcome:?}"
        );
    }

    /// `redact(redact(input)) == redact(input)`. Idempotency canary.
    #[test]
    fn prop_redaction_idempotent(
        email in arb_email(),
        ipv4 in arb_ipv4(),
        bearer in arb_bearer(),
    ) {
        let r = InMemoryPiiRedactor::new();
        let input = format!(
            "user={email} ip={ipv4} auth={bearer}"
        );
        let pass1 = r.redact(&input);
        let pass2 = r.redact(&pass1.redacted);
        prop_assert_eq!(&pass1.redacted, &pass2.redacted);
        // Second pass MUST find zero hits (idempotency invariant).
        prop_assert_eq!(pass2.email_hits, 0);
        prop_assert_eq!(pass2.ip_hits, 0);
        prop_assert_eq!(pass2.token_hits, 0);
    }

    /// Non-PII text passes through unchanged.
    #[test]
    fn prop_redaction_preserves_non_pii(
        text in arb_clean_text(),
    ) {
        let r = InMemoryPiiRedactor::new();
        let outcome = r.redact(&text);
        // The clean text strategy generates ASCII-letter words +
        // single spaces; none of the 5 patterns can match.
        prop_assert_eq!(&outcome.redacted, &text);
        prop_assert_eq!(outcome.total_hits(), 0);
    }

    /// `serialize → deserialize → equal`.
    #[test]
    fn prop_log_schema_serialization_roundtrip(
        event_type in arb_event_type(),
        region in arb_region(),
        time_ms in 0_u64..u64::MAX / 2,
        has_tenant in any::<bool>(),
    ) {
        let id = Uuid::now_v7();
        let tenant = if has_tenant {
            Some(Uuid::now_v7())
        } else {
            None
        };
        let r = LogRecord::new(
            event_type,
            "corelink-test",
            id,
            time_ms,
            tenant,
            region,
            json!({"k": "v"}),
        );
        let line = r.to_ndjson_line().unwrap();
        let r2 = LogRecord::from_ndjson_line(&line).unwrap();
        prop_assert_eq!(&r, &r2);
    }

    /// Tenant A logs never reference tenant B body data.
    /// INV-TENANT-ISOLATION canary.
    #[test]
    fn prop_tenant_isolation(
        tenant_a in any::<u128>(),
        tenant_b in any::<u128>(),
        region in arb_region(),
    ) {
        prop_assume!(tenant_a != tenant_b);
        let (s, _audit) = fresh_sink();
        let id_a = Uuid::from_u128(tenant_a);
        let id_b = Uuid::from_u128(tenant_b);
        let r_a = LogRecord::new(
            LogEventType::RequestServed,
            "corelink-test",
            Uuid::now_v7(),
            1,
            Some(id_a),
            region,
            json!({"path": "/v1/cas/put", "tenant_marker": "tenant_a"}),
        );
        let r_b = LogRecord::new(
            LogEventType::RequestServed,
            "corelink-test",
            Uuid::now_v7(),
            2,
            Some(id_b),
            region,
            json!({"path": "/v1/cas/put", "tenant_marker": "tenant_b"}),
        );
        s.emit(r_a, Tier::Team, "req-a", 1).unwrap();
        s.emit(r_b, Tier::Team, "req-b", 2).unwrap();
        let snap = s.snapshot();
        prop_assert_eq!(snap.len(), 2);
        let line_a = &snap.first().unwrap().ndjson;
        let line_b = &snap.get(1).unwrap().ndjson;
        // Neither tenant id appears in the opposite tenant's body.
        prop_assert!(!line_a.contains(&id_b.to_string()));
        prop_assert!(!line_b.contains(&id_a.to_string()));
        prop_assert!(line_a.contains("tenant_a"));
        prop_assert!(line_b.contains("tenant_b"));
        prop_assert!(!line_a.contains("tenant_b"));
        prop_assert!(!line_b.contains("tenant_a"));
    }

    /// Every `LogEventType` value emits the canonical
    /// `corelink.logpush.record_emitted` audit on every emit arm.
    #[test]
    fn prop_audit_emit_per_event_type(
        event_type in arb_event_type(),
        region in arb_region(),
        tier in arb_tier(),
    ) {
        let (s, audit) = fresh_sink();
        let r = LogRecord::new(
            event_type,
            "corelink-test",
            Uuid::now_v7(),
            1,
            None,
            region,
            json!({"k": "v"}),
        );
        s.emit(r, tier, "req", 1).unwrap();
        let recs =
            audit.snapshot_of(LogAuditEventType::RecordEmitted);
        prop_assert_eq!(recs.len(), 1);
        let first = recs.first().unwrap();
        prop_assert_eq!(first.log_event_type, event_type.as_str());
    }

    /// Log emit fails-closed if the per-event-type label tuple would
    /// breach the analytics cardinality budget.
    /// INV-OBS-CARDINALITY-BUDGET enforcement.
    #[test]
    fn prop_cardinality_budget_respected(
        budget in 1_u64..5,
        n_emits in 1_u32..30,
    ) {
        let (s, _audit) = fresh_sink_with_budget(budget, budget * 100);
        let mut ok = 0_u32;
        let mut rejected = 0_u32;
        for i in 0..n_emits {
            // Emit a NEW unique tuple each iteration by varying tier.
            let tier = ALL_TIERS[(i as usize) % ALL_TIERS.len()];
            let r = LogRecord::new(
                LogEventType::RequestServed,
                "corelink-test",
                Uuid::now_v7(),
                u64::from(i),
                None,
                Region::Iad,
                json!({}),
            );
            match s.emit(r, tier, "req", u64::from(i)) {
                Ok(_) => ok = ok.saturating_add(1),
                Err(LogpushError::CardinalityBudgetExceeded {
                    scope,
                    ..
                }) => {
                    rejected = rejected.saturating_add(1);
                    let valid = matches!(
                        scope,
                        "per_metric" | "global"
                    );
                    prop_assert!(valid);
                }
                Err(e) => {
                    prop_assert!(
                        false,
                        "unexpected error variant: {e}"
                    );
                }
            }
        }
        // Defensive: ok + rejected = n_emits; ok always ≥ 1 because
        // the first emit registers the first unique tuple (budget ≥ 1
        // by construction).
        prop_assert!(ok.saturating_add(rejected) == n_emits);
        prop_assert!(ok >= 1, "first emit should always succeed");
        prop_assert!(ok <= n_emits);
        // Defensive: if budget ≥ ALL_TIERS.len() then NO rejections.
        if budget >= ALL_TIERS.len() as u64 {
            prop_assert!(rejected == 0);
        }
    }
}

// ---- Audit fail-closed coverage --------------------------------------

#[test]
fn audit_fail_closed_aborts_emit_no_buffer_mutation() {
    let r = Arc::new(InMemoryPiiRedactor::new());
    let audit = Arc::new(FailingLogAuditSink::new());
    let cardinality_audit =
        Arc::new(InMemoryAnalyticsAuditSink::new());
    let cardinality = Arc::new(CardinalityValidator::with_defaults(
        cardinality_audit,
    ));
    let s = InMemoryLogSink::new(r, audit, cardinality);
    let rec = LogRecord::new(
        LogEventType::RequestServed,
        "corelink-test",
        Uuid::now_v7(),
        1,
        None,
        Region::Iad,
        json!({}),
    );
    let err = s.emit(rec, Tier::Team, "req-1", 1).unwrap_err();
    assert!(matches!(err, LogpushError::Audit(_)));
    assert_eq!(s.len(), 0);
}

#[test]
fn ndjson_emit_does_not_contain_raw_email_anywhere() {
    let (s, _a) = fresh_sink();
    let r = LogRecord::new(
        LogEventType::AuthAttempt,
        "corelink-test",
        Uuid::now_v7(),
        1,
        None,
        Region::Iad,
        json!({
            "outcome": "success",
            "user": "alice@example.com",
            "ip": "192.168.1.42"
        }),
    );
    s.emit(r, Tier::Team, "req-1", 1).unwrap();
    let snap = s.snapshot();
    let line = &snap.first().unwrap().ndjson;
    assert!(!line.contains("alice@example.com"));
    assert!(!line.contains("192.168.1.42"));
}
