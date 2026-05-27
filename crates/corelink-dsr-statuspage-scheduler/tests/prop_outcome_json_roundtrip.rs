//! Wave-19 property test: canonical `outcome_json` snapshot column
//! (migration `0049`) round-trip.
//!
//! Pins the discipline that
//! [`corelink_dsr_statuspage_scheduler::render_outcome_json`] +
//! [`corelink_dsr_statuspage_scheduler::parse_outcome_json`] are
//! perfect inverses across the full
//! [`corelink_privacy_erasure_worker::VerificationOutcome`] surface.
//! Any drift in the serde derives (decision arm rename, report field
//! addition, signature byte-shape change) fails CI here.
//!
//! # 10_000-iter property
//!
//! The canonical property exercises 10_000 deterministic seeds (per
//! the WI-S11-002 wave-19 charter mandate). Override via
//! `PROPTEST_CASES=N` for adversarial local runs (S-07 P1-2 pattern).
//!
//! # Why a separate integration test (not a lib unit test)
//!
//! `serde_json` is the runtime serializer for the wasm32 cron read
//! path; the proptest crate is a dev-dependency we keep out of the
//! library hot path. Mirrors `tests/dsr_statuspage_cron.rs`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_dsr_statuspage_scheduler::{parse_outcome_json, render_outcome_json};
use corelink_privacy_erasure_worker::{
    BackendCompletion, BackendErasureOutcome, BackendKind, ErasureDecision, ErasurePlan,
    ErasureReport, ErasureRequest, ErasureSalt, ReportSignature, VerificationOutcome,
};
use proptest::prelude::*;
use uuid::Uuid;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for the PR gate; 100k nightly via `PROPTEST_CASES=100_000`.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn uuid_strategy() -> impl Strategy<Value = Uuid> {
    proptest::array::uniform16(any::<u8>()).prop_map(Uuid::from_bytes)
}

fn signature_strategy() -> impl Strategy<Value = ReportSignature> {
    proptest::array::uniform32(any::<u8>()).prop_map(ReportSignature::from_bytes)
}

fn backend_kind_strategy() -> impl Strategy<Value = BackendKind> {
    prop_oneof![
        Just(BackendKind::NeonMain),
        Just(BackendKind::NeonBilling),
        Just(BackendKind::R2Cas),
        Just(BackendKind::R2Ac),
        Just(BackendKind::D1),
        Just(BackendKind::Kv),
        Just(BackendKind::Stripe),
        Just(BackendKind::Loki),
        Just(BackendKind::R2AuditPseudo),
        Just(BackendKind::NeonPitrPseudo),
        Just(BackendKind::R2CasLegalHoldPseudo),
        Just(BackendKind::R2EvidencePseudo),
    ]
}

fn outcome_strategy() -> impl Strategy<Value = BackendErasureOutcome> {
    prop_oneof![
        any::<u64>().prop_map(|n| BackendErasureOutcome::Erased { records_deleted: n }),
        any::<u64>()
            .prop_map(|n| BackendErasureOutcome::Pseudonymized { records_redacted: n }),
        (any::<u64>(), any::<u64>()).prop_map(|(s, f)| {
            BackendErasureOutcome::PartialFailure {
                records_succeeded: s,
                records_failed: f,
            }
        }),
        any::<u64>()
            .prop_map(|s| BackendErasureOutcome::Failed { retry_after_seconds: s }),
        Just(BackendErasureOutcome::NotApplicable),
    ]
}

prop_compose! {
    fn backend_completion_strategy()(
        dsr in uuid_strategy(),
        tenant in uuid_strategy(),
        backend in backend_kind_strategy(),
        outcome in outcome_strategy(),
        started in 0u64..1_000_000_000_000u64,
        elapsed in 0u64..1_000_000u64,
        retry in 0u32..=5u32,
        verification_hash in proptest::array::uniform32(any::<u8>()),
    ) -> BackendCompletion {
        let idempotency_key = format!(
            "corelink-{dsr_short}-{backend}-{retry:03}",
            dsr_short = &dsr.simple().to_string()[..8],
            backend = backend.as_str(),
        );
        BackendCompletion {
            dsr_id: dsr,
            tenant_id: tenant,
            backend,
            outcome,
            idempotency_key,
            started_at_ms: started,
            completed_at_ms: started.saturating_add(elapsed),
            retry_count: retry,
            verification_hash,
        }
    }
}

prop_compose! {
    fn verification_outcome_strategy()(
        completions in proptest::collection::vec(backend_completion_strategy(), 0..=12),
        with_report in any::<bool>(),
        verified_complete in any::<bool>(),
        verified_at_ms in 0u64..1_000_000_000_000u64,
        failed_count in 0usize..=12,
        unverified_count in 0usize..=12,
        elapsed_ms in 0u64..1_000_000_000u64,
        dsr in uuid_strategy(),
        tenant in uuid_strategy(),
        signature in signature_strategy(),
    ) -> VerificationOutcome {
        if with_report && !completions.is_empty() {
            let salt = ErasureSalt::synthetic_for_test(7);
            let request = ErasureRequest::new(dsr, tenant, dsr, salt, verified_at_ms);
            let plan = ErasurePlan::canonical(&request, verified_at_ms);
            let report = ErasureReport {
                dsr_id: dsr,
                tenant_id: tenant,
                plan,
                completions,
                verified_at_ms,
                verified_complete,
                cloudevent_types: vec![
                    "dev.hugr.corelink.dsr.erasure.completed.v1".to_string(),
                ],
            };
            let decision = if verified_complete {
                ErasureDecision::VerifiedComplete {
                    completions: report.completions.clone(),
                }
            } else {
                ErasureDecision::VerifiedPartial {
                    completions: report.completions.clone(),
                    failed_count,
                }
            };
            VerificationOutcome {
                decision,
                report: Some(report),
                signature: Some(signature),
                object_key: Some(format!(
                    "evidence-dsr/{}/erasure-report.json",
                    dsr.simple()
                )),
            }
        } else {
            let decision = ErasureDecision::SlaBreached {
                unverified_count,
                elapsed_ms,
            };
            VerificationOutcome {
                decision,
                report: None,
                signature: None,
                object_key: None,
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Canonical 10k-iter round-trip: render → SQL string → parse
    /// preserves every field of `VerificationOutcome`. Pins the
    /// `outcome_json` snapshot column discipline.
    #[test]
    fn outcome_json_render_parse_roundtrip(
        outcome in verification_outcome_strategy(),
    ) {
        let rendered = render_outcome_json(&outcome)
            .expect("render must not fail on a typed VerificationOutcome");
        let parsed = parse_outcome_json(&rendered)
            .expect("parse must not fail on canonical render output");
        let re_rendered = render_outcome_json(&parsed)
            .expect("re-render after parse must not fail");
        // serde_json is canonical at the typed-boundary level: the
        // re-rendered string MUST byte-equal the first render (no
        // field-order drift, no whitespace drift).
        prop_assert_eq!(rendered, re_rendered);
    }
}
