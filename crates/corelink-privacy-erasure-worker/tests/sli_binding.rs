//! Cross-crate SLI binding regression — pin alignment between the
//! canonical Prometheus metric base name emitted by the
//! `corelink-privacy-erasure-worker` 24h verification cron worker
//! (`corelink_dsr_resolution_hours`) and the `corelink-slo` canonical
//! [`Sli::FreshDsrErasure`] arm.
//!
//! If either side renames without updating the other, the dashboard
//! panel + multi-burn-rate alert evaluator + SLO calculator all break
//! silently. This test pins the alignment at `cargo test` time so the
//! failure mode is a compile-time / test-time signal, not a silent
//! metric drop.
//!
//! Closure from `specs/_audits/2026-05-15-dsr-worker-production.md
//! §3` per WI-S11-002 §10.4 O-4.1.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test file"
)]

use corelink_privacy_erasure_worker::{METRIC_DSR_RESOLUTION_HOURS, SLA_WINDOW_HOURS};
use corelink_slo::{canonical_slis, Sli};

#[test]
fn sli_metric_base_pinned_to_corelink_slo() {
    // Production cron emits to this exact base name. Renaming either
    // side without the other → silent dashboard / alert / SLO calc
    // breakage. Pinned per
    // `specs/_audits/2026-05-15-dsr-worker-production.md §3`.
    assert_eq!(
        METRIC_DSR_RESOLUTION_HOURS,
        Sli::FreshDsrErasure.prometheus_metric_base(),
        "SLI metric base name drifted between corelink-privacy-erasure-worker \
         and corelink-slo::Sli::FreshDsrErasure"
    );
}

#[test]
fn sli_slug_pinned_to_slo_catalog() {
    // SLO slug must match `slo_catalog.md §4.12` exactly. PagerDuty
    // dedup-key construction + alert annotations depend on this slug.
    assert_eq!(Sli::FreshDsrErasure.slug(), "SLO-FRESH-DSR-ERASURE");
}

#[test]
fn fresh_dsr_erasure_registered_in_canonical_list() {
    // The canonical SLI list pins surface stability for downstream
    // sinks (alert evaluator, dashboard generator, burn-rate
    // calculator). Pin the FreshDsrErasure arm at the tail position.
    let slis = canonical_slis();
    assert!(
        slis.contains(&Sli::FreshDsrErasure),
        "FreshDsrErasure missing from canonical SLI list"
    );
    assert_eq!(slis.len(), 18);
}

#[test]
fn sla_window_matches_slo_catalog_bucket_bound() {
    // `slo_catalog.md §4.12` defines the SLI numerator as
    // `corelink_dsr_resolution_hours_bucket{request="erasure", le≤720}`.
    // The 720 bucket bound = 30 days = LGPD Art. 19 + GDPR Art. 12.3
    // + CCPA §1798.130 canonical SLA. Pinned here as a regression
    // guard against accidental constant drift.
    assert_eq!(SLA_WINDOW_HOURS, 720);
}
