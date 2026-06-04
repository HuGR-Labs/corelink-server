//! `corelink-ops` — canonical ops-context surface for the CoreLink
//! Rust workspace.
//!
//! Wave-33 Stage 1 Stream C sub-step C.2 lands this crate as the
//! **single import target** for every ops-context primitive that
//! previously lived across **28 separate crates** (the largest
//! absorption of Wave-33).
//!
//! ```text
//! use corelink_ops::oncall::*;             // PagerDuty oncall scheduler + fatigue tracking
//! use corelink_ops::statuspage::*;         // Atlassian Statuspage client (pure-logic re-export)
//! use corelink_ops::slack::*;              // Slack incoming-webhook client (pure-logic re-export)
//! use corelink_ops::admin::api::*;         // Admin REST API surface
//! use corelink_ops::admin::dry_run::*;     // Admin dry-run preview
//! use corelink_ops::admin::handler::*;     // Admin handler trait + SLI observer
//! use corelink_ops::admin::dual_approval::*; // Admin 2-of-N approval gate
//! use corelink_ops::alerts::*;             // Customer-facing multi-channel alerts
//! use corelink_ops::enterprise::*;         // Enterprise inquiry form + 24h SLA
//! use corelink_ops::survey::*;             // NPS / CSAT / free-text survey infra
//! use corelink_ops::runbook::*;            // Runbook drill tracker (PAT-RUNBOOK-DRILL-001)
//! use corelink_ops::tenant_offboarding::*; // Tenant offboarding 5-state machine
//! use corelink_ops::deploy::*;             // Deploy verifier (artifact integrity)
//! use corelink_ops::terraform::*;          // Terraform drift consumer
//! use corelink_ops::dr::drill::*;          // DR drill scheduler (CF region outage sim)
//! use corelink_ops::dr::backup_verify::*;  // Continuous daily backup verification
//! use corelink_ops::chaos::*;              // Chaos scheduler (8 experiment types)
//! use corelink_ops::rotation::adapters::*; // Key rotation adapters
//! use corelink_ops::rotation::worker::*;   // Key rotation worker
//! use corelink_ops::dt::webhook::*;        // Drift Tracker webhook receiver
//! // (corelink-dt-cli + corelink-dt-reconcile are binary-only crates;
//! //  not re-exported into the library surface — see `dt` rustdoc.)
//! use corelink_ops::drata::*;              // Drata SOC 2 evidence sync
//! use corelink_ops::supply_chain::policy::*; // Supply-chain policy engine
//! use corelink_ops::supply_chain::verify::*; // Supply-chain verification (SBOM + attestation)
//! use corelink_ops::config::api::*;        // Config REST API
//! use corelink_ops::config::durable_object::*; // Config Durable Object
//! use corelink_ops::migrations::*;         // D1 migrations replay harness
//! ```
//!
//! ## Stage 1 Stream C absorption strategy — Option-A aggregator
//!
//! Per `specs/_audits/sealed/2026-05-22-wave33-code-reorg-spec.md` §6 Stage 1
//! Stream C and the Stage 0 SEAL audit §4 (Option-A aggregator
//! interpretation), this crate "absorbs" 28 existing crates by
//! re-exporting them at canonical submodule paths. The absorbed
//! crates remain the canonical sources of truth — their src/, tests/,
//! benches/ harnesses are unchanged. Consumer migration proceeds
//! incrementally.
//!
//! ### Absorbed crates (28 — LARGEST Wave-33 absorption)
//!
//! Thematic grouping under 19 canonical submodules:
//!
//! - [`oncall`] ← `corelink-oncall` — PagerDuty oncall scheduler +
//!   fatigue tracking + PagerDuty integration trait (WI-S17-005).
//! - [`statuspage`] ← `corelink-statuspage-real` — Atlassian Statuspage
//!   client (StatuspageBackend trait + DsrCompletionReport 24h-rolling
//!   payload + reqwest::blocking POST + 1-per-5-min rate-limiter +
//!   3-retry exp-backoff + fail-CLOSED audit envelope). **HTTPS portion
//!   ALSO re-exported by `corelink-adapters-cloud::statuspage` per C.3;
//!   pure-logic vs. binding decomposition deferred to Stage 2.**
//! - [`slack`] ← `corelink-slack-real` — Slack incoming-webhook real
//!   client (Block Kit + per-channel routing + retry + audit envelope).
//!   **HTTPS portion ALSO re-exported by `corelink-adapters-cloud::slack`
//!   per C.3; pure-logic vs. binding decomposition deferred to Stage 2.**
//! - [`admin`] thematic submodule with 4 sub-submodules:
//!   - [`admin::api`] ← `corelink-admin-api`
//!   - [`admin::dry_run`] ← `corelink-admin-dry-run`
//!   - [`admin::handler`] ← `corelink-handler-admin`
//!   - [`admin::dual_approval`] ← `corelink-dual-approval`
//! - [`alerts`] ← `corelink-customer-alerts` — customer-facing multi-
//!   channel alerts.
//! - [`enterprise`] ← `corelink-enterprise-inquiry` — enterprise inquiry
//!   form + Slack/CRM atomic outbox + 24h auto-reply SLA (WI-S19-005).
//! - [`survey`] ← `corelink-survey` — NPS / CSAT / free-text / multi-
//!   choice survey infra (HMAC-SHA256 signed invite tokens + TTL +
//!   audit-fail-CLOSED record path).
//! - [`runbook`] ← `corelink-runbook-tracker` — runbook dry-run
//!   tracker (PAT-RUNBOOK-DRILL-001 + FM-202; WI-S17-003).
//! - [`tenant_offboarding`] ← `corelink-tenant-offboarding` — tenant
//!   offboarding 5-state machine (ACTIVE → CANCEL_REQUESTED →
//!   GRACE_PERIOD → READ_ONLY → SUSPENDED → ERASED) + 30d grace
//!   export + 90d cryptographic erasure.
//! - [`deploy`] ← `corelink-deploy-verifier` — deploy verifier
//!   (artifact integrity + SBOM cross-check).
//! - [`terraform`] ← `corelink-terraform-drift-consumer` — Terraform
//!   drift consumer (admin-dry-run sibling).
//! - [`dr`] thematic submodule with 2 sub-submodules:
//!   - [`dr::drill`] ← `corelink-dr-drill` — DR drill scheduler + CF
//!     region outage simulator (WI-S17-002).
//!   - [`dr::backup_verify`] ← `corelink-backup-verify` — continuous
//!     daily backup verification (complements GAP-15 cold-restore).
//! - [`chaos`] ← `corelink-chaos-scheduler` — chaos scheduler (8
//!   experiment types + safe-mode auto-abort + deterministic seed +
//!   7y archive; WI-S17-001 foundation for WI-S17-002..006).
//! - [`rotation`] thematic submodule with 2 sub-submodules:
//!   - [`rotation::adapters`] ← `corelink-rotation-adapters`
//!   - [`rotation::worker`] ← `corelink-rotation-worker`
//! - [`dt`] (Drift Tracker) thematic submodule. `corelink-dt-cli` and
//!   `corelink-dt-reconcile` are binary-only workspace targets (no
//!   `src/lib.rs`) so they cannot be re-exported into the library
//!   surface; they remain canonical workspace binaries consumed via
//!   `cargo run --bin`. Only the library `corelink-dt-webhook` is
//!   re-exported at [`dt::webhook`].
//! - [`drata`] ← `corelink-drata-sync` — SOC 2 Drata evidence collection
//!   pipeline (audit_outbox / tenants RBAC / PAT issuance / GH PR-merge /
//!   PD incident timeline / pentest findings → Drata REST API with
//!   Bearer auth + SHA-256 idempotency + retry + fail-CLOSED).
//! - [`supply_chain`] thematic submodule with 2 sub-submodules:
//!   - [`supply_chain::policy`] ← `corelink-supply-chain-policy`
//!   - [`supply_chain::verify`] ← `corelink-supply-verify`
//! - [`config`] thematic submodule with 2 sub-submodules:
//!   - [`config::api`] ← `corelink-config-api`
//!   - [`config::durable_object`] ← `corelink-config-do`
//! - [`migrations`] ← `corelink-d1-migrations` — D1 migration replay
//!   harness (rusqlite-backed; CI gate that every migrations/d1/*.sql
//!   applies cleanly in numeric order against a fresh in-memory SQLite).
//!
//! ### Why aggregator rather than physical move
//!
//! 1. **Admin dual-approval flow** — the admin api + dry-run + handler +
//!    dual-approval crates compose a 4-way invariant
//!    (`require_two_distinct_approvers` + `dry_run_preview_diff_only` +
//!    audit envelope ordering); physical relocation requires atomic
//!    consumer migration across `apps/server`. Charter Hard Pause
//!    Trigger 2 (test goes red).
//! 2. **PAT-RUNBOOK-DRILL-001 invariant** — the runbook-tracker is
//!    coupled to chaos-scheduler experiments by reference; physically
//!    moving src/ risks breaking the 24h drill aggregation window.
//! 3. **D1 migrations replay harness** — `corelink-d1-migrations`
//!    consumes every migrations/d1/*.sql across the workspace; moving
//!    src/ here requires updating the migrations search path which is
//!    Stage 2 / consumer-migration territory.
//! 4. **Tenant offboarding 5-state machine** — composes DSR erasure
//!    worker + billing cancel + audit emit + 30d grace export across
//!    crate boundaries; physical relocation requires atomic D1 schema
//!    update of `tenant_state` enum (Stage 2 follows).
//!
//! ## Behaviour preservation
//!
//! Every public symbol of the 28 absorbed crates remains reachable at
//! its original path AND at the new canonical path. No public-API
//! contract is broken.
//!
//! ## Charter compliance (preserved by reference)
//!
//! - Audit-emit-BEFORE-mutation fail-CLOSED envelopes across every
//!   absorbed crate — preserved by reference.
//! - `SecretString` on Drata Bearer / Slack webhook / Statuspage page-
//!   id / oncall PagerDuty integration secrets — preserved by reference.
//! - `subtle::ConstantTimeEq` on survey invite-token HMAC compare —
//!   preserved by reference.
//! - Admin 2-of-N dual-approval invariant — preserved by reference.
//! - `#[non_exhaustive]` on every public enum/struct — inherited via
//!   re-export.
//!
//! ## Wave 35 Phase 2 absorption (LARGEST Wave-35 batch)
//!
//! Per `specs/_audits/2026-05-26-w35-p2-ops-absorption.md` (SEALED
//! 2026-05-26), 15 of the original 28 Wave-33 Option-A re-export
//! tenants were physically absorbed into this crate as inline
//! submodules; workspace.members dropped 100 → 85 (-15). Public-API
//! paths (`corelink_ops::<mod_path>::*`) are preserved 1:1; the INV-S17-OPS family,
//! admin 2-of-N dual-approval, oncall PagerDuty SecretString,
//! supply-chain SBOM + attestation verification, drift-tracker webhook
//! HMAC, D1 migrations replay determinism, audit-emit-BEFORE-mutation
//! fail-CLOSED envelopes, `#![forbid(unsafe_code)]`, and
//! `#[non_exhaustive]` discipline all preserved by reference. 405
//! tests green post-absorption (201 unit + 179 integration + 25 doc).
//!
//! Absorbed crates (15):
//!
//! - `corelink-admin-api` → [`admin::api`] — Admin REST API surface.
//! - `corelink-admin-dry-run` → [`admin::dry_run`] — Admin dry-run
//!   preview (+ 3 bins).
//! - `corelink-backup-verify` → [`dr::backup_verify`] — Continuous
//!   daily backup verification.
//! - `corelink-config-api` → [`config::api`] — Config REST API.
//! - `corelink-customer-alerts` → [`alerts`] — Customer-facing
//!   multi-channel alerts.
//! - `corelink-d1-migrations` → [`migrations`] — D1 migrations replay
//!   harness.
//! - `corelink-deploy-verifier` → [`deploy`] — Deploy verifier
//!   (artifact integrity).
//! - `corelink-dr-drill` → [`dr::drill`] — DR drill scheduler (CF
//!   region outage sim).
//! - `corelink-drata-sync` → [`drata`] — Drata SOC 2 evidence sync.
//! - `corelink-oncall` → [`oncall`] — PagerDuty oncall scheduler +
//!   fatigue tracking.
//! - `corelink-rotation-worker` → [`rotation::worker`] — Key rotation
//!   worker.
//! - `corelink-supply-chain-policy` → [`supply_chain::policy`] —
//!   Supply-chain policy engine.
//! - `corelink-supply-verify` → [`supply_chain::verify`] —
//!   Supply-chain verification (SBOM + attestation) (+ 1 bin).
//! - `corelink-survey` → [`survey`] — NPS / CSAT / free-text survey.
//! - `corelink-tenant-offboarding` → [`tenant_offboarding`] — Tenant
//!   offboarding 5-state machine.
//!
//! The remaining 13 Wave-33 Option-A tenants stay external:
//! `statuspage`, `slack`, `handler-admin`, `dual-approval`,
//! `enterprise-inquiry`, `runbook`, `terraform`, `chaos`,
//! `rotation::adapters`, `dt::webhook`, `config::durable_object`
//! (W36 Stage 2.C zones / wasm32 coupling / live external consumer
//! pins), plus `dt-cli` + `dt-reconcile` (binary-only — cannot be
//! re-exported into a library surface).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod admin;
pub mod alerts;
pub mod chaos;
pub mod config;
pub mod deploy;
pub mod dr;
pub mod drata;
pub mod dt;
pub mod enterprise;
pub mod migrations;
pub mod oncall;
pub mod rotation;
pub mod runbook;
pub mod slack;
pub mod statuspage;
pub mod supply_chain;
pub mod survey;
pub mod tenant_offboarding;
pub mod terraform;

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
    fn oncall_path_resolves() {
        #[allow(unused_imports)]
        use crate::oncall as _o;
    }

    #[test]
    fn statuspage_path_resolves() {
        #[allow(unused_imports)]
        use crate::statuspage as _sp;
    }

    #[test]
    fn slack_path_resolves() {
        #[allow(unused_imports)]
        use crate::slack as _s;
    }

    #[test]
    fn admin_paths_resolve() {
        #[allow(unused_imports)]
        use crate::admin::{api as _a, dry_run as _d, dual_approval as _da, handler as _h};
    }

    #[test]
    fn alerts_path_resolves() {
        #[allow(unused_imports)]
        use crate::alerts as _a;
    }

    #[test]
    fn enterprise_path_resolves() {
        #[allow(unused_imports)]
        use crate::enterprise as _e;
    }

    #[test]
    fn survey_path_resolves() {
        #[allow(unused_imports)]
        use crate::survey as _s;
    }

    #[test]
    fn runbook_path_resolves() {
        #[allow(unused_imports)]
        use crate::runbook as _r;
    }

    #[test]
    fn tenant_offboarding_path_resolves() {
        #[allow(unused_imports)]
        use crate::tenant_offboarding as _t;
    }

    #[test]
    fn deploy_path_resolves() {
        #[allow(unused_imports)]
        use crate::deploy as _d;
    }

    #[test]
    fn terraform_path_resolves() {
        #[allow(unused_imports)]
        use crate::terraform as _t;
    }

    #[test]
    fn dr_paths_resolve() {
        #[allow(unused_imports)]
        use crate::dr::{backup_verify as _b, drill as _d};
    }

    #[test]
    fn chaos_path_resolves() {
        #[allow(unused_imports)]
        use crate::chaos as _c;
    }

    #[test]
    fn rotation_paths_resolve() {
        #[allow(unused_imports)]
        use crate::rotation::{adapters as _a, worker as _w};
    }

    #[test]
    fn dt_paths_resolve() {
        #[allow(unused_imports)]
        use crate::dt::webhook as _w;
    }

    #[test]
    fn drata_path_resolves() {
        #[allow(unused_imports)]
        use crate::drata as _d;
    }

    #[test]
    fn supply_chain_paths_resolve() {
        #[allow(unused_imports)]
        use crate::supply_chain::{policy as _p, verify as _v};
    }

    #[test]
    fn config_paths_resolve() {
        #[allow(unused_imports)]
        use crate::config::{api as _a, durable_object as _do};
    }

    #[test]
    fn migrations_path_resolves() {
        #[allow(unused_imports)]
        use crate::migrations as _m;
    }
}
