//! Router assembly — the composed data-plane router the container serves.
//!
//! Split out of `routes.rs` under B-126: that file has two jobs, declaring the
//! module tree and assembling the router, and the second was 649 of its 1273
//! lines. The module declarations stay where they were — a route module is
//! reached as `crate::routes::cas`, and moving the list would change every
//! path in the crate for no gain.
//!
//! What lives here is the ONE function that wires every surface together, plus
//! the two tests that exercise it end to end. Everything it needs from the
//! parent — `QuotaGate`, `approver_key_distinct_or_none` — is imported rather
//! than duplicated, so the parent stays the single definition site.

use super::*;

/// Build the composed router with an explicit
/// [`ShadowSinkFactory`] — used by the production boot path to swap
/// in the D1-backed `D1ShadowSinkFactory` while keeping every other
/// route shape identical. (The Neon-backed factory this once named was
/// retired with #71 — see [`build`].)
pub fn build_with_factory(shadow_factory: Arc<dyn ShadowSinkFactory>) -> Router {
    // Per-tenant monthly $-ceiling gate (ADR-0068; hugit-P2 WP-G1). ONE
    // gate (D1-backed) shared across every billable data-plane surface
    // (CAS/AC, Bazel REAPI, Turbo, sccache), exactly like the rate-limit
    // gate. `None` in dev/CI (no D1 storage env) — the ceiling is then not
    // enforced, mirroring the adapter routes' fail-CLOSED env-gate.
    let quota = QuotaGate::from_env();
    if quota.is_none() {
        tracing::warn!(
            "tenant-quota: D1 storage env absent; per-tenant $-ceiling gate NOT \
             enforced on billable routes (dev/CI mode)"
        );
    }

    // Per-tenant monthly REQUEST-count gate (rt-nuclear #8 — the request-count
    // half). Backs the OCI surface, which the Worker forwards RAW and so never
    // counts against `monthly_request_counts` (migration 0071). D1-backed;
    // `None` in dev/CI (no D1 storage env), mirroring the $-ceiling gate above.
    // Only the OCI router consumes it — the native + other-adapter surfaces are
    // already metered at the Worker edge by `checkRequestQuota`
    // (`incrementMonthlyRequestCount`, exactly once per request). DELIBERATELY
    // not cloned into the native CAS/AC/Bazel/Turbo states like `quota` /
    // `pat_gate` are: unlike the $-ceiling (a SUM read) and byte-cap (an
    // idempotent reserve→commit on real bytes), the request counter is a raw
    // per-request INCREMENT, so wiring it here too would double-count every
    // native op (Worker edge + container) and false-deny legitimate traffic at
    // half the contracted cap. OCI is the ONE surface the Worker forwards RAW
    // (never counted at the edge), so the container gate is its sole enforcer —
    // together the two planes meter the fleet exactly once each. The boot
    // watchdog (`main.rs`) asserts THIS gate must arm in prod for exactly that
    // reason, scoped to OCI (not a fleet-wide-native claim).
    let request_count = crate::request_count::RequestCountGate::from_env();
    if request_count.is_none() {
        tracing::warn!(
            "request-count: D1 storage env absent; OCI monthly request-count cap NOT \
             enforced (dev/CI mode)"
        );
    }

    // Display usage aggregator (usage-metering-roi — the customer ROI surface):
    // ONE in-process meter shared across the instrumented flagship cache surfaces
    // (native CAS, Bazel REAPI, Turbo). Unlike the gates above, `record` is a
    // cheap fire-and-forget lock+increment at each hit/miss/write decision — NEVER
    // a D1 round-trip on the hot path — and a background flusher drains the
    // accumulated deltas to `usage_daily` every ~30s. `from_env()` wires the
    // D1-backed sink when StorageEnv is present; in dev/CI it is INERT (`record`
    // is a no-op, `spawn_flusher` a no-op), mirroring the gates above. It is NEVER
    // read on a request path and never bills anything (display telemetry only).
    let usage_meter = crate::usage_meter::UsageMeter::from_env();
    usage_meter
        .clone()
        .spawn_flusher(std::time::Duration::from_secs(30));

    // Native data-plane PAT possession gate (red-team #4) + per-tenant storage
    // byte accounting (#1). Built from env (PAT_SIGNING_KEY + StorageEnv / D1);
    // `None` in dev/CI ⇒ the native plane skips the Argon2id backstop and the
    // storage accrual, mirroring the quota/tombstone fail-safe env-gates above.
    // ONE gate/accountant is cloned (cheap `Arc`) into every billable native
    // state (CAS/AC/Bazel/Turbo) so all four surfaces re-verify the bearer PAT
    // against D1 (never trusting the Worker-injected tenant header) and accrue
    // bytes against the same `tenant_storage_state` row.
    let native_pat_gate = crate::native_pat_gate::native_pat_gate_from_env();
    if native_pat_gate.is_none() {
        tracing::warn!(
            "native PAT gate disabled: PAT_SIGNING_KEY/StorageEnv absent — native \
             CAS/AC/Bazel/Turbo do NOT re-verify the bearer PAT (dev/CI mode)"
        );
    }
    let byte_accountant = crate::byte_accounting::byte_accountant_from_env();
    if byte_accountant.is_none() {
        tracing::warn!(
            "storage byte-accounting disabled: D1 StorageEnv absent — CAS/AC writes \
             NOT accrued against tenant_storage_state.bytes_used (dev/CI mode)"
        );
    }

    let (cas_read_raw, cas_write_raw, cas_delete_raw, cas_list) = cas::build_handlers();
    // Tenant bulk-export (SEAM): the export source reads blobs through the SAME
    // CAS read/list handlers (one R2 connection, no forked store) — capture the
    // `Arc`s BEFORE they are moved into `cas_state` / the Bazel bridge below.
    let export_cas_read = cas_read_raw.clone();
    let export_cas_list = cas_list.clone();
    // Storage byte accounting (red-team #1 / cluster B+C): wrap the CAS write +
    // delete trait objects in the reserve→commit→release decorator at the SINGLE
    // chokepoint every CAS write surface flows through (native CAS, Bazel REAPI,
    // OCI, and the cargo/brew/npm/pip language adapters all drive these SAME
    // `Arc<dyn …>` objects). Wrapping here means all of them inherit identical,
    // atomic, fail-CLOSED byte accounting — no per-surface duplication, and the
    // previously-uncounted sibling planes (cluster B) are closed for free. When
    // the accountant is absent (dev/CI, no D1) the raw handlers pass through
    // unwrapped. read/list are not write surfaces and stay unwrapped.
    let (cas_write_acct, cas_delete_acct): (
        Arc<dyn corelink_handler_cas::CasWriteHandler>,
        Arc<dyn corelink_handler_cas::CasDeleteHandler>,
    ) = match byte_accountant.as_ref() {
        Some(acc) => {
            let acct = Arc::new(crate::byte_accounting::AccountingCasHandler::new(
                cas_write_raw.clone(),
                cas_delete_raw.clone(),
                acc.clone(),
            ));
            (
                acct.clone() as Arc<dyn corelink_handler_cas::CasWriteHandler>,
                acct as Arc<dyn corelink_handler_cas::CasDeleteHandler>,
            )
        }
        None => (cas_write_raw, cas_delete_raw),
    };
    // 410-Gone tombstone read gate (hugit-P2 seam B, WP-B). Wired from env
    // (D1-backed) when D1 creds are present so an erased hash answers 410 even
    // before the (#254-gated) erase WRITE route is mounted; `None` in dev/CI ⇒
    // classic 200/404 (fail-safe — no false 410s without a real store).
    // WP-2b integration: front the D1 tombstone store with an in-memory bloom so
    // the read hot path skips the per-request D1-over-HTTP round-trip for the
    // (overwhelmingly common) non-erased digest. Safe because the erase write
    // side deletes the R2 bytes BEFORE writing the tombstone row (cas_erase.rs
    // ordering): a bloom miss on a cross-instance-erased digest falls through to
    // an R2 read that 404s (bytes gone) — it never serves erased content, at
    // worst returns 404 instead of 410 within the bounded refresh window. See
    // `BloomTombstoneStore`.
    let cas_tombstones: Option<Arc<dyn cas_erase::TombstoneStore>> =
        cas_erase::D1TombstoneStore::from_env().map(|s| {
            Arc::new(cas_erase::BloomTombstoneStore::new(
                Arc::new(s) as Arc<dyn cas_erase::TombstoneStore>
            )) as Arc<dyn cas_erase::TombstoneStore>
        });
    // F-004 — CENTRALIZE the 410 erasure gate at the shared CAS seam. The native
    // route's inline tombstone gate (`cas::handle_read`) was the ONLY gate, so
    // GDPR-erased bytes were re-PUT-able (writes were ungated) and readable via
    // every OTHER surface (cargo/sccache/brew/npm/pip/Bazel/Turbo/OCI all drive
    // these SAME shared `Arc<dyn …>` handlers but never checked a tombstone).
    // Wrap the shared read/write/delete trait objects in
    // `TombstoneGatedCasHandler` HERE — the same chokepoint `AccountingCasHandler`
    // uses — so ALL surfaces inherit the erasure gate by construction: a
    // tombstoned read 404s (never serves erased bytes), a re-PUT of a tombstoned
    // hash is refused 410 (no resurrection), a gate fault fails CLOSED 503. The
    // bloom-fronted store (above) keeps this off the hot D1 path for the common
    // non-erased digest. When no tombstone store is wired (dev/CI) the handlers
    // pass through ungated. The native route keeps `cas_tombstones` too, so it
    // still answers a precise 410 (not 404) BEFORE reaching the handler.
    let (cas_read, cas_write, cas_delete): (
        Arc<dyn corelink_handler_cas::CasReadHandler>,
        Arc<dyn corelink_handler_cas::CasWriteHandler>,
        Arc<dyn corelink_handler_cas::CasDeleteHandler>,
    ) = match cas_tombstones.as_ref() {
        Some(ts) => {
            let gated = Arc::new(cas_erase::TombstoneGatedCasHandler::new(
                cas_read_raw.clone(),
                cas_write_acct.clone(),
                cas_delete_acct.clone(),
                ts.clone(),
            ));
            (
                gated.clone() as Arc<dyn corelink_handler_cas::CasReadHandler>,
                gated.clone() as Arc<dyn corelink_handler_cas::CasWriteHandler>,
                gated as Arc<dyn corelink_handler_cas::CasDeleteHandler>,
            )
        }
        None => (cas_read_raw, cas_write_acct, cas_delete_acct),
    };
    let cas_state = cas::CasRouteState {
        read: cas_read.clone(),
        write: cas_write.clone(),
        delete: cas_delete,
        list: cas_list,
        tombstones: cas_tombstones,
        quota: quota.clone(),
        pat_gate: native_pat_gate.clone(),
        put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        read_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        // usage-metering-roi: the ONE shared display meter (clone = cheap Arc).
        usage_meter: usage_meter.clone(),
    };
    let (ac_lookup, ac_update_raw, ac_delete_raw, ac_list) = ac::build_handlers();
    // Tenant bulk-export (SEAM): reuse the SAME AC lookup/list handlers for the
    // AC half of the bundle — capture BEFORE they are moved into `ac_state` /
    // the Bazel bridge.
    let export_ac_lookup = ac_lookup.clone();
    let export_ac_list = ac_list.clone();
    // Storage byte accounting (cluster B+C) for the AC plane: same decorator
    // chokepoint over the AC update + delete trait objects, shared with the
    // Bazel REAPI AC write surface. `None` (dev/CI) ⇒ raw handlers pass through.
    let (ac_update, ac_delete): (
        Arc<dyn corelink_handler_ac::AcUpdateHandler>,
        Arc<dyn corelink_handler_ac::AcDeleteHandler>,
    ) = match byte_accountant.as_ref() {
        Some(acc) => {
            let acct = Arc::new(crate::byte_accounting::AccountingAcHandler::new(
                ac_update_raw.clone(),
                ac_delete_raw.clone(),
                acc.clone(),
            ));
            (
                acct.clone() as Arc<dyn corelink_handler_ac::AcUpdateHandler>,
                acct as Arc<dyn corelink_handler_ac::AcDeleteHandler>,
            )
        }
        None => (ac_update_raw, ac_delete_raw),
    };
    let ac_state = ac::AcRouteState {
        lookup: ac_lookup.clone(),
        update: ac_update.clone(),
        delete: ac_delete,
        list: ac_list,
        quota: quota.clone(),
        pat_gate: native_pat_gate.clone(),
        put_inflight: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    };
    // Cache adapters share the SAME CAS trait objects (one R2 connection) —
    // clone BEFORE they are moved into the Bazel bridge below. cargo writes
    // per-tenant via CargoCasBridge; brew/npm/pip dedup through the 2-level moat.
    let cargo_cas_read = cas_read.clone();
    let cargo_cas_write = cas_write.clone();
    let brew_cas_read = cas_read.clone();
    let brew_cas_write = cas_write.clone();
    let npm_cas_read = cas_read.clone();
    let npm_cas_write = cas_write.clone();
    let pip_cas_read = cas_read.clone();
    let pip_cas_write = cas_write.clone();
    let oci_cas_read = cas_read.clone();
    let oci_cas_write = cas_write.clone();
    // Phase 0 Stream B1: Bazel REAPI v2 routes share the same CAS/AC
    // trait objects so all four route surfaces read from / write to the
    // same backing store. No new R2 connections are opened.
    let mut bazel_state = bazel_v2::build_handlers_from(cas_read, cas_write, ac_lookup, ac_update);
    bazel_state.quota = quota.clone();
    bazel_state.pat_gate = native_pat_gate.clone();
    bazel_state.usage_meter = usage_meter.clone();
    // SECURITY (admin control-plane gate): the `/v1/admin/*` and
    // `/v1/admin/pilots/*` surfaces are OPERATOR-ONLY — they must NOT be
    // reachable by any authenticated tenant PAT. We gate them behind the
    // `CORELINK_INTERNAL_AUTH_KEY` shared secret, exactly like
    // `/_internal/pat/mint`. The key is threaded into each route state;
    // when it is unset at boot the handlers fail CLOSED (403) and never
    // run privileged logic. (We always-mount and fail-closed rather than
    // conditionally mount, because `build_with_factory` returns a
    // non-optional `Router` and dev/CI must still construct it.)
    let admin_internal_auth_key = admin::internal_auth_key_from_env();
    if admin_internal_auth_key.is_none() {
        tracing::warn!(
            "CORELINK_INTERNAL_AUTH_KEY unset; /v1/admin/* and /v1/admin/pilots/* \
             mounted but FAIL CLOSED (403) — operator gate not configured (dev/CI mode)"
        );
    }
    let admin_stack = admin::build_handler_stack();
    // Dedicated approve-gate key (finding H5, hardened 2026-08-19) — a DIFFERENT
    // credential from the mutate/admin key so approve and mutate require distinct
    // keys (real two-person control). DEDICATED-ONLY (no shared-key fallback), and
    // a byte-equal-to-internal key is rejected at boot by
    // `approver_key_distinct_or_none` so credential separation is enforced in code.
    let admin_approver_auth_key = approver_key_distinct_or_none(
        admin::approver_auth_key_from_env(),
        admin_internal_auth_key.as_ref(),
    );
    if admin_approver_auth_key.is_none() {
        tracing::warn!(
            "CORELINK_ADMIN_APPROVER_AUTH_KEY unset, < 32 chars, or byte-equal to \
             CORELINK_INTERNAL_AUTH_KEY; POST /v1/admin/approve mounted but FAILS \
             CLOSED (403) — dual-approval cannot be recorded until a DISTINCT \
             dedicated approver key is provisioned (no shared-key fallback)"
        );
    }
    let admin_state = admin::AdminRouteState {
        read: admin_stack.read,
        mutate: admin_stack.mutate,
        internal_auth_key: admin_internal_auth_key.clone(),
        approval_writer: admin_stack.approval_writer,
        approver_auth_key: admin_approver_auth_key,
        approvals_durable: admin_stack.durable,
    };
    let (default_pilot_store, pilot_audit) = admin_pilot::build_handlers();
    // Durable pilot-tenant store (hugit-P2 WP-FOUND): D1-backed when the
    // D1 env is present so pilots survive container restarts; the
    // non-durable InMemory store is the dev/CI fallback (fail-closed
    // env-gate, mirroring `customer::build_handlers_from_env`).
    let pilot_store: std::sync::Arc<dyn admin_pilot::PilotStore> =
        match admin_pilot::D1PilotStore::from_env() {
            Some(d1_store) => d1_store,
            None => {
                tracing::warn!(
                    "admin_pilot: D1 env absent; /v1/admin/pilots backed by \
                     NON-durable InMemoryPilotStore (dev/CI mode)"
                );
                default_pilot_store
            }
        };
    let pilot_admin_state = admin_pilot::PilotAdminRouteState {
        store: pilot_store,
        audit_sink: pilot_audit,
        wall_clock: crate::wall_clock::default_wall_clock(),
        internal_auth_key: admin_internal_auth_key,
    };
    // BYOK activation control plane (operator-gated, same secret as `/v1/admin/*`).
    // Writer is `None` in dev/CI (no D1 creds) → routes fail CLOSED (503).
    let byok_admin_state = byok_admin::ByokAdminRouteState::from_env();
    let mut audit_export_state = audit_export::build_state();
    // Native PAT possession backstop (rt-nuclear #17): the audit-export +
    // analytics surfaces were UN-gated, so a leaked PAT_SIGNING_KEY could forge a
    // bearer to exfiltrate any victim tenant's audit log / analytics. Wire the
    // SAME gate the native CAS/AC/Bazel/Turbo states carry.
    audit_export_state.pat_gate = native_pat_gate.clone();
    let mut audit_analytics_state = audit_analytics::build_state(shadow_factory);
    audit_analytics_state.pat_gate = native_pat_gate.clone();
    // Customer dashboard (WP-3): D1-backed handler when the D1 env is
    // present; InMemory fallback for dev/CI (fail-closed env-gate,
    // mirroring `adapter_pat::PatVerifier::from_env`).
    let mut customer_state = customer::build_handlers_from_env();
    // Native PAT possession backstop (cycle-2 nuclear red-team, cluster A — the
    // customer control plane was UN-gated, so a leaked PAT_SIGNING_KEY could
    // HMAC-forge a PAT that minted a genuine cas:rw PAT for any victim tenant).
    // Wire the SAME gate the native CAS/AC/Bazel/Turbo states carry — EXACTLY as
    // `bazel_state.pat_gate = native_pat_gate.clone();` above.
    customer_state.pat_gate = native_pat_gate.clone();
    // Self-serve account-deletion (C-ACCTDEL): wire the GDPR erasure requester
    // over the same D1 source + the in-process DSR erasure worker (the Clerk
    // `user.deleted` path's engine). Env-gated: `None` in dev/CI → the
    // `POST /v1/customer/account/delete` route fails CLOSED (503). Mirrors the
    // `pat_gate` wiring above (a cross-module collaborator composed at the root).
    customer_state.account_deletion = customer::account_deletion_from_env();
    // Tenant bulk-export (SEAM): wire the portability-bundle source over the SAME
    // CAS/AC read+list handlers (blob bytes) + a D1 row source (RBAC/DPA/audit).
    // Env-gated: `None` in dev/CI (no D1) → `POST /v1/customer/account/export`
    // fails CLOSED (503). Mirrors the `account_deletion` wiring above.
    customer_state.export = customer_export::from_handlers_and_env(
        export_cas_read,
        export_cas_list,
        export_ac_lookup,
        export_ac_list,
    );
    // Customer-facing DSR self-service portal (`/v1/privacy/dsr/*`): the intake
    // surface the admin-ui `dsr-client.ts` posts to. It DRIVES the live Wave-1
    // DSR pipeline (access/portability/rectification + the erasure worker) and
    // persists a tenant-scoped ticket store (D1). Env-gated: the data rights
    // fail CLOSED (503) until storage is configured. Wire the SAME native-PAT
    // possession backstop the customer plane carries.
    let mut privacy_dsr_state = dsr::portal::build_state_from_env();
    privacy_dsr_state.pat_gate = native_pat_gate.clone();
    // Customer Runners (BE-10) + Workspaces (BE-11): tenant-scoped, own-tenant
    // only (tenant derived from the session header, never a client param). Wire
    // the SAME native-PAT possession backstop the customer/CAS states carry — a
    // leaked PAT_SIGNING_KEY must not forge access to these surfaces either.
    let mut customer_runners_state = customer_runners::build_state_from_env();
    customer_runners_state.pat_gate = native_pat_gate.clone();
    let mut workspaces_state = workspaces::build_handlers_from_env();
    workspaces_state.pat_gate = native_pat_gate.clone();
    // `/v1/users/me` gets the identical backstop (it reflected a forged PAT's
    // claimed identity un-gated).
    let users_state = users::UsersRouteState {
        pat_gate: native_pat_gate.clone(),
    };
    let mut turbo_state = turbo_v8::build_handlers();
    turbo_state.quota = quota.clone();
    turbo_state.pat_gate = native_pat_gate.clone();
    turbo_state.bytes = byte_accountant.clone();
    turbo_state.usage_meter = usage_meter.clone();
    let mut router = Router::new()
        .merge(cas::router(cas_state))
        .merge(ac::router(ac_state))
        .merge(admin::router(admin_state))
        .merge(admin_tenant_detail::router(
            admin_tenant_detail::AdminTenantDetailState::from_env(),
        ))
        .merge(admin_pilot::router(pilot_admin_state))
        .merge(byok_admin::router(byok_admin_state))
        .merge(audit_export::router(audit_export_state))
        .merge(audit_analytics::router(audit_analytics_state))
        .merge(users::router(users_state))
        .merge(customer::router(customer_state))
        .merge(dsr::portal::router(privacy_dsr_state))
        .merge(customer_runners::router(customer_runners_state))
        .merge(workspaces::router(workspaces_state))
        .merge(bazel_v2::router(bazel_state))
        .merge(turbo_v8::router(turbo_state));

    // Pilot-signup route — env-gated, fail-CLOSED (mirrors `internal_pat`).
    // Mounted only when `SIGNUP_TOKEN_KEY` is present + valid (hex, ≥ 32
    // bytes decoded); in dev/CI without the secret it is absent (404)
    // rather than running with the public hardcoded dev key, which would
    // make the pilot-token HMAC forgeable.
    if let Some(signup_state) = signup::build_state_from_env() {
        router = router.merge(signup::router(signup_state));
    } else {
        tracing::warn!(
            "SIGNUP_TOKEN_KEY unset/invalid; /v1/signup/pilot NOT mounted (fail-CLOSED)"
        );
    }

    // Cache-adapter surfaces (Option B — the container re-verifies the bearer
    // PAT against D1 via the SHARED `adapter_pat::PatVerifier`, never trusting
    // the Worker-injected tenant header). Mounted only when the verifier builds
    // from env (PAT_SIGNING_KEY + StorageEnv); in dev/CI it is absent and these
    // routes 404 (fail-CLOSED) rather than running with an unconfigured
    // validator. Mirrors `internal_pat`'s env-gate.
    //
    // ONE `PatVerifier` is shared across cargo/brew/npm/pip: cargo wraps it via
    // `cargo::resolver_from_verifier`; brew/npm/pip take it directly. cargo needs
    // only the verifier (sccache keys are client-content-addressed, stored
    // per-tenant via CargoCasBridge — no cross-tenant dedup). brew/npm/pip add
    // the 2-level content-dedup moat: they need the D1-over-HTTP client, which
    // doubles as the url→content-hash map (`adapter_cache::UrlMapStore`) AND
    // pip's index-KV backend. A per-adapter dependency that fails to build from
    // env skips ONLY that adapter (fail-CLOSED per-adapter).
    if let Some(verifier) = crate::adapter_pat::PatVerifier::from_env() {
        let verifier = Arc::new(verifier);

        // cargo/brew/npm/oci/pip ALL ride the 2-level moat map (the D1-over-HTTP
        // client). cargo JOINED the moat (private per-tenant namespace) to fix the
        // sccache-key-as-CAS-digest HashMismatch 502, so it now also needs the D1
        // map. A per-adapter env builder that returns None skips ONLY that adapter
        // (fail-CLOSED). The $-ceiling gate + the resolver's two-layer write check
        // (F27: scope header AND the PAT-derived `can_write`) are threaded into each.
        match crate::adapter_cache::d1_map_from_env() {
            Some(d1) => {
                // cargo (sccache): PRIVATE per-tenant moat namespace (no _public).
                // Thread the SAME D1-backed per-tier cap resolver OCI uses so a
                // FRESH tenant's first cargo write auto-seeds its
                // `tenant_storage_state` row with the REAL cap (else the
                // indeterminate-cap reservation fails CLOSED → 502 on a brand-new
                // sccache user's first PUT). Reuses the moat's D1 client.
                let cargo_map: Arc<dyn crate::adapter_cache::UrlMapStore> = d1.clone();
                let cargo_cap_resolver: Arc<dyn crate::oci_cap::TenantCapResolver> =
                    Arc::new(crate::oci_cap::D1TenantCapResolver::new(d1.clone()));
                router = router.merge(cargo::router(
                    cargo_cas_read,
                    cargo_cas_write,
                    cargo_map,
                    cargo::resolver_from_verifier(verifier.clone()),
                    quota.clone(),
                    Some(cargo_cap_resolver),
                ));

                // brew: shared map + verifier (public bottles, cross-tenant dedup).
                let brew_map: Arc<dyn crate::adapter_cache::UrlMapStore> = d1.clone();
                router = router.merge(brew::router(
                    brew_cas_read,
                    brew_cas_write,
                    brew_map,
                    verifier.clone(),
                    quota.clone(),
                ));

                // npm: shared map + verifier + the D1-backed metadata KV table
                // (`adapter_npm_meta`). If its env builder returns None, skip npm only.
                match crate::adapter_kv::npm_kv_from_env() {
                    Some(npm_meta_kv) => {
                        let npm_map: Arc<dyn crate::adapter_cache::UrlMapStore> = d1.clone();
                        router = router.merge(npm::router(
                            npm_cas_read,
                            npm_cas_write,
                            npm_map,
                            npm_meta_kv,
                            verifier.clone(),
                            quota.clone(),
                        ));
                    }
                    None => {
                        tracing::warn!(
                            "npm metadata KV unavailable from env; /npm/* NOT mounted \
                             (fail-CLOSED) — cargo/brew/pip unaffected"
                        );
                    }
                }

                // oci: shared moat map (blobs) + the durable D1 manifest KV
                // (`adapter_oci_kv`, migration 0061) + verifier + the OCI session
                // HMAC key (CORELINK_OCI_TOKEN_KEY). All four required; skip oci
                // (fail-CLOSED) if the KV or the key is absent.
                match (
                    crate::adapter_oci_kv::oci_kv_from_env(),
                    // CAA-360 #8: canonical name first, then the legacy
                    // HUGR_OCI_TOKEN_KEY prod was deployed with (name drift), so the
                    // OCI route mounts regardless of which secret name is set.
                    crate::storage::non_empty_env(oci::OCI_TOKEN_KEY_ENV)
                        .or_else(|| crate::storage::non_empty_env(oci::OCI_TOKEN_KEY_ENV_LEGACY)),
                ) {
                    (Some(oci_kv), Some(token_key)) => {
                        let oci_map: Arc<dyn crate::adapter_cache::UrlMapStore> = d1.clone();
                        let oci_manifest_kv: Arc<
                            dyn corelink_adapter_host::oci::ports::ManifestKvStore,
                        > = oci_kv;
                        // WP #10: resolve the tenant's RESOLVED per-tier storage
                        // cap at `/token` mint (the only seam where OCI knows the
                        // tenant). Reuses the SAME D1 client the moat map uses;
                        // the cap is embedded in the signed bearer and reserved
                        // against at finalize. The Worker forwards OCI RAW and
                        // never sets the native `STORAGE_QUOTA_HEADER`, so this is
                        // the sole carrier of the cap for the OCI plane.
                        let oci_cap_resolver: Arc<dyn crate::oci_cap::TenantCapResolver> =
                            Arc::new(crate::oci_cap::D1TenantCapResolver::new(d1.clone()));
                        // G4b: tenant-suspend gate over the SAME D1 client. A
                        // suspended/erased tenant is denied the token mint AND
                        // every `/v2` op (the Worker forwards OCI RAW, so the
                        // worker-side suspend gate never sees it). Single-flight +
                        // 30 s TTL, fail-CLOSED for a known-suspended tenant.
                        let oci_suspend_resolver: Arc<dyn crate::oci_suspend::SuspendResolver> =
                            Arc::new(crate::oci_suspend::CachedSuspendResolver::new(
                                Arc::new(crate::oci_suspend::D1SuspendResolver::new(d1.clone())),
                                Arc::new(crate::wall_clock::SystemWallClock::new()),
                                crate::oci_suspend::DEFAULT_SUSPEND_CACHE_TTL_MS,
                            ));
                        router = router.merge(oci::router(
                            oci_cas_read,
                            oci_cas_write,
                            oci_map,
                            oci_manifest_kv,
                            verifier.clone(),
                            corelink_core::SecretWrap::new(token_key),
                            quota.clone(),
                            request_count.clone(),
                            Some(oci_cap_resolver),
                            Some(oci_suspend_resolver),
                        ));
                    }
                    _ => {
                        tracing::warn!(
                            "CORELINK_OCI_TOKEN_KEY or OCI manifest KV unavailable; \
                             /v2/* + /token (OCI registry) NOT mounted (fail-CLOSED) \
                             — cargo/brew/npm/pip unaffected"
                        );
                    }
                }

                // pip: shared map + verifier + the SAME D1 client (reused for the
                // per-tenant simple-index KV table `adapter_pip_index`).
                let pip_map: Arc<dyn crate::adapter_cache::UrlMapStore> = d1.clone();
                router = router.merge(pip::router(
                    pip_cas_read,
                    pip_cas_write,
                    pip_map,
                    d1,
                    verifier,
                    quota.clone(),
                ));
            }
            None => {
                tracing::warn!(
                    "D1 moat map unavailable from env; /cargo/*, /brew/*, /npm/*, \
                     /v2/* (OCI), /pip/* NOT mounted (fail-CLOSED)"
                );
            }
        }
    } else {
        tracing::warn!(
            "PAT_SIGNING_KEY/StorageEnv unset; cache-adapter routes \
             (/cargo/*, /brew/*, /npm/*, /pip/*) NOT mounted (dev/CI mode)"
        );
    }

    // backlog #29 (Schrems II residency leak): data-residency guard. Runs BEFORE
    // every handler — rejects (409) any request whose trusted
    // x-corelink-primary-region macro does not map to THIS container's
    // R2_CAS_REGION colo. Defence-in-depth backstop for a mis-bound regional
    // Worker; zero storage I/O on the reject path. Disjoint from cas.rs/ac.rs.
    router = router.layer(axum::middleware::from_fn(residency::residency_guard));

    // WI-MULTI-REGION-V1 (failover prod-wiring): read-side failover Tower layer.
    // Mirrors the residency layer above — one `.layer(...)` line, disjoint from
    // the handler bodies. Drives `corelink-failover-router`'s decision core with
    // a REAL `RollingMetricsHealthProbe` over THIS container's live traffic. In a
    // healthy region it is fully inert (the multi-signal AND rule never trips on
    // transient slowness); under a sustained region outage it fail-CLOSED blocks
    // writes (503 `failover_readonly`) and stamps a sibling read-region hint so
    // the edge Worker re-routes reads via its cross-region Service Binding. Inert
    // on nrt/syd (no sibling in the 4-macro graph) + dev/CI. Primary region is
    // resolved from `R2_CAS_REGION` (same env as residency + cas.rs).
    let failover_state = failover::FailoverLayerState::from_env();
    router = router.layer(axum::middleware::from_fn_with_state(
        failover_state,
        failover::failover_guard,
    ));

    // audit #14/#16 (request-rate limiting): per-tenant token-bucket gate over
    // the metered data plane. Charges one token per request against the
    // DO-injected `x-corelink-tenant-id` tenant's bucket (100 req/s sustained,
    // burst 200 — see `ratelimit_layer` consts) and 429s + Retry-After over the
    // limit. Applied here (NOT in `main.rs`) so it wraps exactly the data-plane
    // routers — the `/_health` readiness probe and `/_internal/*` surfaces are
    // merged later in `main.rs` and stay OUTSIDE this layer by construction.
    // Fail-OPEN on absent tenant (non-billable traffic) + on the limiter's own
    // internal fault (logged), prioritising paid-plane availability.
    //
    // F-017: wire the D1-backed per-tenant tier resolver so the per-tier RPS
    // ladder ACTUALLY enforces (without it every tenant sits on the team default).
    // Build a dedicated D1 client from env — the moat `d1` above is scoped to the
    // cache-adapter block — and reuse `oci_cap::D1TenantCapResolver` (one source
    // of truth for the tenant→tier lookup). If StorageEnv/D1 init is unavailable,
    // fall back to the resolver-less state: a config gap must NEVER brick the data
    // plane (fail-SAFE → team default for all), and the resolver only TIGHTENS
    // free/solo + loosens paid, so its absence is non-fatal.
    let rate_limit_state = match crate::storage::StorageEnv::from_env()
        .and_then(|env| crate::storage::d1_http::D1HttpClient::new(&env).ok())
    {
        Some(client) => {
            let tier_resolver: Arc<dyn ratelimit_layer::TenantTierResolver> =
                Arc::new(crate::oci_cap::D1TenantCapResolver::new(Arc::new(client)));
            ratelimit_layer::RateLimitLayerState::with_tier_resolver(
                crate::wall_clock::default_wall_clock(),
                tier_resolver,
            )
        }
        None => {
            tracing::warn!(
                "F-017: D1 tier-resolver unavailable (StorageEnv/D1 init); per-tenant \
                 rate-limit stays on the team default for all tenants (no per-tier ladder)"
            );
            ratelimit_layer::RateLimitLayerState::new()
        }
    };
    router = router.layer(axum::middleware::from_fn_with_state(
        rate_limit_state,
        ratelimit_layer::rate_limit_layer,
    ));

    // OTel-export seam: stream a canonical `MetricPoint` + `TraceSpan` per
    // data-plane request through the configured exporter's fail-OPEN boundary.
    // Wired as the OUTERMOST data-plane layer (added last ⇒ observes the FINAL
    // response status, including a rate-limit 429 / residency 409). Mounted ONLY
    // when `CORELINK_OBSERVABILITY_EXPORT_VARIANT` selects a configured vendor
    // (`otel_collector` / `datadog` / `grafana_cloud`); unset / `disabled` /
    // malformed ⇒ NOT mounted (dev/CI: zero overhead, behaviour unchanged; a bad
    // observability config must never brick the data plane). See
    // `routes/otel_layer.rs` for the full env surface + the deferred-real egress
    // residual (the collector endpoint is an operator step).
    if let Some(otel_state) = otel_layer::OtelExportState::from_env() {
        router = router.layer(axum::middleware::from_fn_with_state(
            otel_state,
            otel_layer::otel_export_layer,
        ));
    }

    // `origin` sub-phase timing. Added LAST ⇒ the OUTERMOST data-plane layer, so
    // its clock spans everything the container does (routing, the rate-limit
    // layer, the OTel layer above, the handler) — which is what makes the
    // `oother` residue it publishes a TRUE residue rather than a partial one.
    // Unconditional, unlike the OTel layer: it needs no config, does no I/O, and
    // costs a couple of microseconds (see `crate::origin_timing`). It reports on
    // the response's own `Server-Timing`; the Worker parses that and merges it
    // under `origin` alongside the `ohop` residue it derives.
    router = router.layer(axum::middleware::from_fn(
        crate::origin_timing::origin_timing_layer,
    ));

    router
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_with_factory_accepts_in_memory_factory() {
        // #71: the production boot path uses build_with_factory() to
        // swap in the D1-backed D1ShadowSinkFactory; tests + dev use the
        // in-memory factory. (It swapped in a TokioPgShadowSinkFactory
        // under --feature neon-real until #71 retired that path.) Both
        // branches must construct without panic.
        let factory: Arc<dyn ShadowSinkFactory> = Arc::new(InMemoryShadowSinkFactory::new());
        let _router = build_with_factory(factory);
    }

    #[tokio::test]
    async fn build_with_factory_produces_a_routing_router() {
        use axum::body::Body;
        use axum::http::{Method, Request, StatusCode};
        use tower::ServiceExt;

        // A real router has the routes mounted; the degenerate
        // `build_with_factory -> Default::default()` mutant returns an EMPTY
        // `Router` that 404s every path. GET on the POST-only `/v1/admin/mutate`
        // route → axum 405 (path registered, method mismatch) on the real
        // router, vs 404 on the empty one — kills that mutant.
        let factory: Arc<dyn ShadowSinkFactory> = Arc::new(InMemoryShadowSinkFactory::new());
        let router = build_with_factory(factory);
        let resp = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/v1/admin/mutate")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "build_with_factory must mount real routes (an empty router would 404)"
        );
    }
}
