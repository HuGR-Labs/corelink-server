//! Targeted regression tests that close mutation-testing surface
//! coverage gaps identified by `cargo mutants -p corelink-signup` on
//! 2026-05-14.
//!
//! Pattern: each block targets one `as_str` / `Display` / canonical
//! taxonomy surface where the existing suite did not exercise the
//! returned value directly. See
//! `specs/_audits/sealed/2026-05-14-mutation-baseline.md` for full rationale.

#![forbid(unsafe_code)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args
)]

use corelink_signup::{
    audit::{
        canonical_signup_audit_event_strings, InMemorySignupAuditSink, SignupAuditEventType,
        SignupAuditRecord, SignupAuditSink,
    },
    billing::{
        BillingClient, BillingError, InMemoryBillingClient, StripeCustomerId,
        StripeOutageBillingClient,
    },
    correlation::CorrelationId,
    idempotency::IdempotencyKey,
    orchestrator::{InMemoryProvisionRecord, ProvisionRecord},
    outcome::{canonical_orchestration_steps, OrchestrationStep},
    pat::{PatHash, ShownOnceToken},
    region::{canonical_regions, Bcp47Locale, PrimaryRegion},
    signup_schema_version,
    store::{
        AtomicSignupStore, DpaPendingRow, FailingAtomicSignupStore, PatRow, StorageError,
        TenantRow, UsageCounterRow,
    },
    tenant::{SignupId, TenantId, UserEmailHash},
    FIRST_PAT_EXPIRY_SECONDS, WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS,
};

// =====================================================================
// OrchestrationStep::as_str — canonical Prometheus labels.
// Kills mutations replacing each branch with "" / "xyzzy" / Default.
// =====================================================================

#[test]
fn orchestration_step_as_str_strings_canonical_and_distinct() {
    let pairs = [
        (OrchestrationStep::Begin, "begin"),
        (OrchestrationStep::InsertTenant, "insert_tenant"),
        (OrchestrationStep::InsertDpa, "insert_dpa"),
        (OrchestrationStep::InsertFirstPat, "insert_first_pat"),
        (
            OrchestrationStep::InsertUsageCounterAndCommit,
            "insert_usage_counter_and_commit",
        ),
    ];
    for (step, expected) in &pairs {
        assert_eq!(step.as_str(), *expected, "{:?} as_str", step);
        assert!(!step.as_str().is_empty());
        assert_ne!(step.as_str(), "xyzzy");
        // Display impl must match as_str.
        assert_eq!(format!("{}", step), *expected);
    }
    // canonical_orchestration_steps must match.
    let canonical = canonical_orchestration_steps();
    let collected: Vec<&str> = pairs.iter().map(|(_, s)| *s).collect();
    assert_eq!(canonical.as_slice(), collected.as_slice());
    // All 5 strings distinct.
    let mut sorted = collected.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 5);
}

// =====================================================================
// SignupAuditEventType::as_str — canonical CloudEvents type strings.
// =====================================================================

#[test]
fn signup_audit_event_type_as_str_strings_canonical_and_distinct() {
    let pairs = [
        (SignupAuditEventType::Started, "corelink.signup.started"),
        (SignupAuditEventType::Completed, "corelink.signup.completed"),
        (SignupAuditEventType::Failed, "corelink.signup.failed"),
        (SignupAuditEventType::Deferred, "corelink.signup.deferred"),
    ];
    for (ev, expected) in &pairs {
        assert_eq!(ev.as_str(), *expected);
        assert!(ev.as_str().starts_with("corelink.signup."));
        assert!(!ev.as_str().is_empty());
        assert_ne!(ev.as_str(), "xyzzy");
        assert_eq!(format!("{}", ev), *expected);
    }
    let canonical = canonical_signup_audit_event_strings();
    let collected: Vec<&str> = pairs.iter().map(|(_, s)| *s).collect();
    assert_eq!(canonical.as_slice(), collected.as_slice());
    let mut sorted = collected.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 4);
}

// =====================================================================
// PrimaryRegion::as_str — D1 row label.
// =====================================================================

#[test]
fn primary_region_as_str_strings_canonical_and_distinct() {
    // Eu => "weur" (the canonical D1 macro; the old "eu" string was invalid —
    // not in the tenant.primary_region CHECK set — M4 fix).
    let pairs = [
        (PrimaryRegion::Enam, "enam"),
        (PrimaryRegion::Sam, "sam"),
        (PrimaryRegion::Eu, "weur"),
        (PrimaryRegion::Apac, "apac"),
    ];
    for (r, expected) in &pairs {
        assert_eq!(r.as_str(), *expected);
        assert!(!r.as_str().is_empty());
        assert_ne!(r.as_str(), "xyzzy");
        assert_eq!(format!("{}", r), *expected);
    }
    assert_eq!(canonical_regions(), &["enam", "sam", "weur", "apac"]);
}

// =====================================================================
// PrimaryRegion::from_locale boundary cases (EU + fallback).
// Kills mutations replacing the starts_with checks with ! / constants.
// =====================================================================

#[test]
fn primary_region_from_locale_boundary_cases() {
    // pt-BR full + pt-BR prefix variants → Sam.
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("pt-BR")),
        PrimaryRegion::Sam
    );
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("PT-br")),
        PrimaryRegion::Sam
    );
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("pt")),
        PrimaryRegion::Sam
    );
    // de-DE / de-AT → Eu.
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("de-DE")),
        PrimaryRegion::Eu
    );
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("de-AT")),
        PrimaryRegion::Eu
    );
    // fr-FR / fr-CA → Eu (fr prefix).
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("fr-FR")),
        PrimaryRegion::Eu
    );
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("fr-CA")),
        PrimaryRegion::Eu
    );
    // es-ES → Eu; es-MX → Enam (fallback; es-ES is exact prefix).
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("es-ES")),
        PrimaryRegion::Eu
    );
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("es-MX")),
        PrimaryRegion::Enam
    );
    // en-US, en-GB, unknown → Enam.
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("en-US")),
        PrimaryRegion::Enam
    );
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("en-GB")),
        PrimaryRegion::Enam
    );
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("zz-XX")),
        PrimaryRegion::Enam
    );
    // Empty → Enam fallback (defensive default).
    assert_eq!(
        PrimaryRegion::from_locale(&Bcp47Locale::new("")),
        PrimaryRegion::Enam
    );
}

// =====================================================================
// Newtype as_str / Display surface — TenantId, SignupId, PatHash,
// ShownOnceToken, UserEmailHash, CorrelationId, IdempotencyKey.
// =====================================================================

#[test]
fn signup_newtype_as_str_and_display_round_trip() {
    macro_rules! rt {
        ($T:ty, $val:expr) => {{
            let v = <$T>::new($val);
            assert_eq!(v.as_str(), $val, concat!(stringify!($T), "::as_str"));
            assert_eq!(format!("{}", v), $val, concat!(stringify!($T), "::Display"));
            assert!(!v.as_str().is_empty());
        }};
    }
    rt!(TenantId, "t-mut-1");
    rt!(SignupId, "s-mut-1");
    rt!(PatHash, "h-mut-1");
    rt!(ShownOnceToken, "tok-mut-1");
    rt!(UserEmailHash, "abc123hash");
    rt!(CorrelationId, "evt_mut_1");
    rt!(IdempotencyKey, "idem-mut-1");
    rt!(Bcp47Locale, "pt-BR");
}

#[test]
fn idempotency_key_is_empty_returns_true_only_when_empty() {
    assert!(IdempotencyKey::new("").is_empty());
    assert!(!IdempotencyKey::new("x").is_empty());
    assert!(!IdempotencyKey::new("idem-001").is_empty());
}

// =====================================================================
// In-memory audit sink — emit / snapshot / len / is_empty.
// Kills mutations replacing len -> 0/1 and is_empty -> true/false.
// =====================================================================

#[test]
fn in_memory_audit_sink_len_and_is_empty_track_emits() {
    let sink = InMemorySignupAuditSink::new();
    assert!(sink.is_empty());
    assert_eq!(sink.len(), 0);
    for i in 0..3 {
        let rec = SignupAuditRecord::new(
            SignupAuditEventType::Started,
            CorrelationId::new(format!("cid-{i}")),
            format!("idem-{i}"),
        );
        sink.emit(&rec).unwrap();
    }
    assert!(!sink.is_empty());
    assert_eq!(sink.len(), 3);
    let snap = sink.snapshot();
    assert_eq!(snap.len(), 3);
    // The records must be the ones we emitted, in order.
    for (i, r) in snap.iter().enumerate() {
        assert_eq!(r.idempotency_key, format!("idem-{i}"));
        assert_eq!(r.correlation_id.as_str(), &format!("cid-{i}"));
    }
}

// =====================================================================
// SignupAuditRecord builder — with_region / with_step / with_reason.
// =====================================================================

#[test]
fn signup_audit_record_builders_attach_fields() {
    let rec = SignupAuditRecord::new(
        SignupAuditEventType::Failed,
        CorrelationId::new("cid-build"),
        "idem-build",
    );
    // No fields populated yet.
    assert!(rec.primary_region.is_none());
    assert!(rec.step.is_none());
    assert!(rec.reason.is_none());

    let with_all = rec
        .with_region(PrimaryRegion::Sam)
        .with_step(OrchestrationStep::InsertTenant)
        .with_reason("FK violation on tenant insert");
    assert_eq!(with_all.primary_region, Some(PrimaryRegion::Sam));
    assert_eq!(with_all.step, Some(OrchestrationStep::InsertTenant));
    assert_eq!(
        with_all.reason.as_deref(),
        Some("FK violation on tenant insert")
    );
}

// =====================================================================
// BillingClient — InMemory happy + StripeOutage chaos surfaces.
// =====================================================================

#[test]
fn in_memory_billing_client_returns_stripe_customer_id() {
    let c = InMemoryBillingClient::new();
    let tid = TenantId::new("t-bill-1");
    let id = c.create_customer(&tid).expect("happy path");
    // Must be a Stripe-canonical `cus_*` token (kills `Ok("")` mutants).
    assert!(id.as_str().starts_with("cus_"));
    assert!(!id.as_str().is_empty());
    assert_ne!(id.as_str(), "xyzzy");
}

#[test]
fn stripe_outage_billing_client_always_returns_outage_error() {
    let c = StripeOutageBillingClient;
    let tid = TenantId::new("t-outage-1");
    let err = c.create_customer(&tid).expect_err("outage must error");
    assert!(matches!(err, BillingError::Outage(_)));
    // Repeated calls remain in outage (not transient).
    let err2 = c.create_customer(&tid).expect_err("still outage");
    assert!(matches!(err2, BillingError::Outage(_)));
}

// =====================================================================
// Crate-level canonical constants — kills mutations on lib.rs
// `signup_schema_version`, `FIRST_PAT_EXPIRY_SECONDS`,
// `WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS` (mutations replace with 0 /
// swap `*` to `+` or `/`).
// =====================================================================

#[test]
fn signup_schema_version_is_one() {
    // Kills `replace signup_schema_version -> u32 with 0`.
    assert_eq!(signup_schema_version(), 1);
    assert_ne!(signup_schema_version(), 0);
}

#[test]
fn first_pat_expiry_seconds_canonical_90_days() {
    // Kills `replace * with + / /` mutations on `90 * 24 * 60 * 60`.
    // 90 days × 24 h × 60 min × 60 s = 7_776_000 s.
    assert_eq!(FIRST_PAT_EXPIRY_SECONDS, 90 * 24 * 60 * 60);
    assert_eq!(FIRST_PAT_EXPIRY_SECONDS, 7_776_000);
    // The constant must be more than a day and less than a year.
    // Use `const _: () = assert!(...)` so the bound check is a true
    // compile-time invariant rather than a runtime no-op that clippy
    // (rightly) flags as `assert!(true)` on a literal-vs-const compare.
    const _: () = assert!(FIRST_PAT_EXPIRY_SECONDS > 60 * 60 * 24);
    const _: () = assert!(FIRST_PAT_EXPIRY_SECONDS < 60 * 60 * 24 * 365);
    // Each mutated arithmetic produces a distinctly wrong value:
    //   90 + 24 + 60 + 60 = 234           (way too small)
    //   90 * 24 + 60 * 60 = 5_760          (way too small)
    //   90 + 24 * 60 * 60 = 86_490         (too small)
    //   90 / 24 / 60 / 60 = 0              (zero)
    // Any of these would fail the strict equality + the bounds above.
}

#[test]
fn webhook_timestamp_tolerance_seconds_canonical_5_minutes() {
    // Kills mutations on `5 * 60`.
    assert_eq!(WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS, 5 * 60);
    assert_eq!(WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS, 300);
    // Sanity: greater than 1 min, less than 1 day. Compile-time invariant.
    const _: () = assert!(WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS > 60);
    const _: () = assert!(WEBHOOK_TIMESTAMP_TOLERANCE_SECONDS < 60 * 60 * 24);
}

// =====================================================================
// FailingAtomicSignupStore — adversarial fixture must reject EVERY
// method (not just `begin`). Kills mutations replacing each method
// body with `Ok(())` / `Ok(0/1)`.
// =====================================================================

#[test]
fn failing_atomic_signup_store_rejects_every_mutating_method() {
    let s = FailingAtomicSignupStore;
    // begin always fails.
    assert!(matches!(s.begin(), Err(StorageError::TxFault(_))));

    // Construct a synthetic tx to feed the insert_* methods directly.
    // We can't get one from `begin()` (it always errors), so we
    // fabricate a default `SignupTx` via the public constructor
    // pattern: there isn't one — instead, exploit that `SignupTx` is
    // `#[non_exhaustive]` only across crates: we can't brace-init.
    // Walk the trait via the orchestrator entry point: that already
    // surfaces the begin error, so the inner inserts are unreachable
    // through the production path. The mutation `insert_tenant ->
    // Ok(())` is therefore a *fixture contract* mutation: it changes
    // what the adversarial fake does, not what the production code
    // does. We assert the fixture contract directly by invoking each
    // method through a synthetic tx constructed in the
    // `corelink_signup::store` test module accessor (the struct is
    // `#[non_exhaustive]` so we route through `begin` on a non-failing
    // store, swap stores, then call the failing store's methods on
    // the borrowed tx).
    //
    // The cleanest expression: re-use the InMemoryAtomicSignupStore
    // begin to mint a real `SignupTx`, then assert that
    // `FailingAtomicSignupStore` rejects every insert / commit /
    // committed_count call on that tx.
    let happy = corelink_signup::store::InMemoryAtomicSignupStore::new();
    let mut tx = happy.begin().expect("happy begin");

    let tenant_row = TenantRow {
        tenant_id: TenantId::new("t-fail"),
        signup_id: SignupId::new("s-fail"),
        email_hash: UserEmailHash::new("eh"),
        primary_region: PrimaryRegion::Enam,
    };
    assert!(matches!(
        s.insert_tenant(&mut tx, tenant_row.clone()),
        Err(StorageError::TxFault(_))
    ));

    let dpa_row = DpaPendingRow {
        tenant_id: TenantId::new("t-fail"),
    };
    assert!(matches!(
        s.insert_dpa_pending(&mut tx, dpa_row),
        Err(StorageError::TxFault(_))
    ));

    let pat_row = PatRow {
        tenant_id: TenantId::new("t-fail"),
        pat_hash: PatHash::new("h-fail"),
        shown_once_token: ShownOnceToken::new("tok-fail"),
    };
    assert!(matches!(
        s.insert_first_pat(&mut tx, pat_row),
        Err(StorageError::TxFault(_))
    ));

    let uc_row = UsageCounterRow {
        tenant_id: TenantId::new("t-fail"),
    };
    assert!(matches!(
        s.insert_usage_counter(&mut tx, uc_row),
        Err(StorageError::TxFault(_))
    ));

    let idem = IdempotencyKey::new("idem-fail");
    // commit takes ownership of tx; need to mint another to also test
    // rollback / committed_count.
    let tx_for_commit = happy.begin().expect("happy begin 2");
    assert!(matches!(
        s.commit(tx_for_commit, &idem),
        Err(StorageError::TxFault(_))
    ));

    // committed_tenant_count returns Ok(0) on the failing fixture —
    // distinct from `Ok(1)` mutation.
    assert_eq!(s.committed_tenant_count().expect("count"), 0);

    // lookup_idempotent returns Ok(None) on the failing fixture.
    assert_eq!(s.lookup_idempotent(&idem).expect("lookup"), None);
}

// =====================================================================
// InMemoryProvisionRecord counter — kills mutations on the private
// `next()` method (`-> 0` / `-> 1`). The test fixture MUST mint
// monotonically-increasing distinct ids; otherwise property tests +
// idempotency assertions get false positives.
// =====================================================================

#[test]
fn in_memory_provision_record_mints_distinct_monotonic_ids() {
    let rec = InMemoryProvisionRecord::new();
    let mut tenants = std::collections::HashSet::new();
    let mut signups = std::collections::HashSet::new();
    let mut pats = std::collections::HashSet::new();
    let mut tokens = std::collections::HashSet::new();
    for _ in 0..10 {
        let t = rec.mint_tenant_id();
        let s = rec.mint_signup_id();
        let p = rec.mint_first_pat_hash(&t);
        let tok = rec.mint_shown_once_token();
        assert!(
            tenants.insert(t.as_str().to_string()),
            "tenant id duplicate"
        );
        assert!(
            signups.insert(s.as_str().to_string()),
            "signup id duplicate"
        );
        assert!(pats.insert(p.as_str().to_string()), "pat hash duplicate");
        assert!(tokens.insert(tok.as_str().to_string()), "token duplicate");
    }
    assert_eq!(tenants.len(), 10);
    assert_eq!(signups.len(), 10);
    assert_eq!(pats.len(), 10);
    assert_eq!(tokens.len(), 10);
}

#[test]
fn stripe_customer_id_round_trip() {
    let id = StripeCustomerId::new("cus_abc123");
    assert_eq!(id.as_str(), "cus_abc123");
    assert_eq!(format!("{}", id), "cus_abc123");
    assert!(!id.as_str().is_empty());
}
