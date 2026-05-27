//! `corelink-privacy` — canonical privacy-context surface for the
//! CoreLink Rust workspace.
//!
//! Wave-33 Stage 1 Stream B sub-step B.5 lands this crate as the
//! **single import target** for every privacy primitive that
//! previously lived across 11 separate crates:
//!
//! ```text
//! use corelink_privacy::dsr::*;                  // DSR rights orchestrator + 6-rights surface
//! use corelink_privacy::dsr::statuspage::*;      // DSR Statuspage 24h publish scheduler
//! use corelink_privacy::breach::*;               // Breach notification emit (GDPR Art. 33)
//! use corelink_privacy::consent::*;              // Consent ledger + withdrawal trail
//! use corelink_privacy::erasure::*;              // 12-backend erasure worker + signed report
//! use corelink_privacy::notice::*;               // Privacy notice emit (Art. 13/14)
//! use corelink_privacy::pseudonymize::*;         // Tenant-scoped HMAC pseudonymization
//! use corelink_privacy::residency::*;            // Data residency enforcement
//! use corelink_privacy::sub_processor::*;        // Sub-processor change notification
//! use corelink_privacy::dpa::acceptance::*;      // DPA click-through acceptance + JWT receipt
//! use corelink_privacy::dpa::versioning::*;      // DPA versioning + 30d grace + read-only degrade
//! ```
//!
//! ## Stage 1 Stream B absorption strategy — Option-A aggregator
//!
//! Per `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §6 Stage 1
//! Stream B and the Stage 0 SEAL audit §4 (Option-A aggregator
//! interpretation), this crate "absorbs" 11 existing crates by
//! re-exporting them at canonical submodule paths. The absorbed
//! crates remain the canonical sources of truth — their src/, tests/,
//! benches/, and fuzz/ harnesses are unchanged. Consumer migration
//! (apps/server routes, D1 migrations runner) proceeds incrementally.
//!
//! ### Absorbed crates (11)
//!
//! - `corelink-dsr` — DSR rights orchestrator (access / portability
//!   / rectification / erasure / restriction / objection) +
//!   12-backend erasure plan; re-exported at [`dsr`].
//! - `corelink-dsr-statuspage-scheduler` — DSR 24h-rolling
//!   Statuspage publish scheduler (06:00 UTC daily cron-firable);
//!   re-exported at [`dsr::statuspage`].
//! - `corelink-privacy-breach-emit` — GDPR Art. 33 + 34 breach
//!   notification emit + 72h timer; re-exported at [`breach`].
//! - `corelink-privacy-consent-ledger` — consent ledger +
//!   withdrawal audit trail; re-exported at [`consent`].
//! - `corelink-privacy-erasure-worker` — 12-backend per-row erasure
//!   worker + Ed25519 signed completion report + 24h aggregation
//!   window; re-exported at [`erasure`]. **The Ed25519 signing
//!   primitive itself lives at `corelink_crypto::ed25519::attestation`
//!   (absorbed by Stage 0); this aggregator does NOT touch the
//!   crypto path — preserves charter "Ed25519 erasure attestation
//!   signing path preserved" guarantee by reference.**
//! - `corelink-privacy-notice-emit` — GDPR Art. 13 + 14 privacy
//!   notice emit; re-exported at [`notice`].
//! - `corelink-privacy-pseudonymize` — tenant-scoped HMAC-SHA-256
//!   pseudonymization for analytics; re-exported at [`pseudonymize`].
//! - `corelink-privacy-residency-enforcement` — data residency
//!   enforcement (Enam / Sam / Eu region pinning); re-exported at
//!   [`residency`].
//! - `corelink-privacy-sub-processor-emit` — sub-processor change
//!   notification (Art. 28 + DPA §16); re-exported at
//!   [`sub_processor`].
//! - `corelink-dpa-acceptance` — DPA click-through 6-field consent
//!   + RS256 JWT receipt + 3 locales; re-exported at
//!     [`dpa::acceptance`].
//! - `corelink-dpa-versioning` — DPA versioning + 30-day grace +
//!   read-only degrade middleware; re-exported at [`dpa::versioning`].
//!
//! ### Why aggregator rather than physical move
//!
//! 1. **Ed25519 erasure attestation signing path** — preserved
//!    untouched at its canonical Stage 0 location
//!    (`corelink_crypto::ed25519::attestation`); migrating it would
//!    require atomic update of the erasure-worker call sites
//!    (charter hard rule "Ed25519 erasure attestation signing path
//!    preserved").
//! 2. **D1 migrations coupling** — every absorbed crate that touches
//!    D1 (consent_ledger, dpa-acceptance, dpa-versioning,
//!    erasure-worker, residency) has a paired `migrations/d1/*.sql`
//!    file. Moving src/ requires atomic update of the migrations
//!    replay harness (`corelink-d1-migrations`); that lives in
//!    Stage 2 / Stream C territory.
//! 3. **24h Statuspage scheduler timing window** — the DSR
//!    statuspage scheduler is paired with the erasure-worker via the
//!    `aggregate_24h_window` + `bridge_to_report` composition;
//!    physically moving src/ across crate boundaries risks breaking
//!    the 06:00 UTC cron-firing path that the deployed CF Worker
//!    `dsr_statuspage_cron` event handler expects (charter Hard
//!    Pause Trigger 4 — previously-green test goes red).
//!
//! ## Behaviour preservation
//!
//! Every public symbol of the 11 absorbed crates remains reachable at
//! its original path AND at the new canonical path. No public-API
//! contract is broken.
//!
//! ## Charter compliance (preserved by reference)
//!
//! - Ed25519 erasure attestation signing path — preserved (lives at
//!   `corelink_crypto::ed25519::attestation`; consumed by
//!   `corelink-privacy-erasure-worker` unchanged).
//! - `subtle::ConstantTimeEq` on HMAC pseudonymization tag compare
//!   (`corelink-privacy-pseudonymize`) — preserved by reference.
//! - `SecretString` on DPA JWT signing key — preserved by reference.
//! - Audit-emit-BEFORE-mutation fail-CLOSED envelopes across every
//!   absorbed crate — preserved by reference.
//! - `#[non_exhaustive]` on every public enum/struct — inherited via
//!   re-export.
//!
//! ## Wave 35 Phase 2 absorption
//!
//! Per `specs/_audits/2026-05-26-w35-p2-privacy-absorption.md` (SEALED
//! 2026-05-26), 6 of the Wave-33 Option-A re-export tenants were
//! physically absorbed into this crate as inline submodules (8,754
//! LOC + 242 tests moved in-tree); workspace.members dropped by 6.
//! Public-API paths (`corelink_privacy::<mod>::*`) are preserved 1:1;
//! GDPR Art. 13/14/33/34 invariants, Ed25519 erasure attestation
//! signing path (still consumed unchanged from `corelink_crypto`),
//! consent withdrawal audit trail, residency enforcement,
//! sub-processor change-notification trail, DPA versioning + 30d
//! grace + read-only degrade, `#![forbid(unsafe_code)]`, and
//! `#[non_exhaustive]` discipline all preserved by reference.
//!
//! Absorbed crates (6):
//!
//! - `corelink-dpa-versioning` → [`dpa::versioning`] — DPA versioning
//!   + 30d grace + read-only degrade.
//! - `corelink-privacy-breach-emit` → [`breach`] — GDPR Art. 33 + 34
//!   breach notification emit.
//! - `corelink-privacy-consent-ledger` → [`consent`] — consent ledger
//!   + withdrawal audit trail.
//! - `corelink-privacy-notice-emit` → [`notice`] — GDPR Art. 13 + 14
//!   privacy notice emit.
//! - `corelink-privacy-residency-enforcement` → [`residency`] — data
//!   residency enforcement.
//! - `corelink-privacy-sub-processor-emit` → [`sub_processor`] —
//!   sub-processor change notification.
//!
//! The remaining Wave-33 Stream-B tenants (`dsr`, `dsr::statuspage`,
//! `erasure`, `pseudonymize`, `dpa::acceptance`) stay as aggregator
//! re-exports — they retain coupling to D1 migration runner / CF
//! Worker cron path / crypto signing path that requires atomic
//! consumer migration outside this batch.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod breach;
pub mod consent;
pub mod dpa;
pub mod dsr;
pub mod erasure;
pub mod notice;
pub mod pseudonymize;
pub mod residency;
pub mod sub_processor;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    //! Smoke tests proving every canonical re-export path resolves at
    //! compile time. No new state is introduced.

    #[test]
    fn dsr_path_resolves() {
        #[allow(unused_imports)]
        use crate::dsr as _d;
    }

    #[test]
    fn dsr_statuspage_path_resolves() {
        #[allow(unused_imports)]
        use crate::dsr::statuspage as _s;
    }

    #[test]
    fn breach_path_resolves() {
        #[allow(unused_imports)]
        use crate::breach as _b;
    }

    #[test]
    fn consent_path_resolves() {
        #[allow(unused_imports)]
        use crate::consent as _c;
    }

    #[test]
    fn erasure_path_resolves() {
        #[allow(unused_imports)]
        use crate::erasure as _e;
    }

    #[test]
    fn notice_path_resolves() {
        #[allow(unused_imports)]
        use crate::notice as _n;
    }

    #[test]
    fn pseudonymize_path_resolves() {
        #[allow(unused_imports)]
        use crate::pseudonymize as _p;
    }

    #[test]
    fn residency_path_resolves() {
        #[allow(unused_imports)]
        use crate::residency as _r;
    }

    #[test]
    fn sub_processor_path_resolves() {
        #[allow(unused_imports)]
        use crate::sub_processor as _sp;
    }

    #[test]
    fn dpa_acceptance_path_resolves() {
        #[allow(unused_imports)]
        use crate::dpa::acceptance as _da;
    }

    #[test]
    fn dpa_versioning_path_resolves() {
        #[allow(unused_imports)]
        use crate::dpa::versioning as _dv;
    }
}
