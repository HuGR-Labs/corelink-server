/// Build the OCI `/v2/*` + `/token` sub-router from the shared CAS
/// handlers + the url→hash map + the durable manifest KV + the PAT
/// verifier + the OCI session HMAC key.
///
/// `cas_read`/`cas_write` are the SAME trait objects cas/ac/bazel/turbo/
/// brew use; `map` is the D1-backed url→content-hash store; `manifest_kv`
/// is the durable mutable manifest/tag store
/// ([`crate::adapter_oci_kv::OciKvStore`] in prod); `verifier` is the
/// shared Option-B PAT verifier; `token_signing_key` is the raw
/// (≥32-byte) HMAC key for the adapter's realm bearer tokens (from
/// [`OCI_TOKEN_KEY_ENV`]). On any construction error the route is simply
/// NOT mounted (empty sub-router + logged) so the container still boots.
///
/// No `x-corelink-scope` gate is layered here: the Worker forwards OCI
/// RAW (pass-through — it cannot resolve a PAT scope for the two-leg flow),
/// so per-op authorization is the adapter's OWN bearer-scope enforcement
/// (`scope.allows(repo, action)` on every `/v2` op) plus the `/token`
/// downscope to the PAT's capability. A header gate would 403 every
/// request under pass-through.
// Wiring/DI constructor: each argument is a distinct production collaborator
// (CAS read/write handlers, URL-map + manifest KV stores, PAT verifier, realm
// signing key, and the two optional quota gates). Bundling them into a params
// struct adds indirection without removing any real coupling, so the
// too-many-arguments lint is suppressed here by intent.
#[allow(clippy::too_many_arguments)]
pub fn router(
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    map: Arc<dyn UrlMapStore>,
    manifest_kv: Arc<dyn ManifestKvStore>,
    verifier: Arc<PatVerifier>,
    token_signing_key: SecretWrap,
    quota: Option<crate::routes::QuotaGate>,
    request_count: Option<crate::request_count::RequestCountGate>,
    cap_resolver: Option<Arc<dyn crate::oci_cap::TenantCapResolver>>,
    suspend_resolver: Option<Arc<dyn crate::oci_suspend::SuspendResolver>>,
) -> Router {
    // rt-nuclear #2/#8/#9: the OCI $-ceiling gate must resolve the cost-attribution
    // tenant from the VERIFIED HMAC bearer (the Worker strips `x-corelink-tenant-id`
    // on the OCI pass-through, and an unverified header/claim would let a tenant
    // charge a victim). Capture a copy of the realm signing key for the gate BEFORE
    // `token_signing_key` is moved into the adapter config below. `SecretWrap` is not
    // `Clone` (secret-copy discipline); reconstruct one explicit copy via `new`.
    let gate_realm_key = SecretWrap::new(token_signing_key.expose().to_owned());

    let moat = Arc::new(MoatCache::production(
        cas_read,
        cas_write,
        map,
        OCI_SERVICE_PRINCIPAL,
    ));

    // Read the F3.2 cross-tenant dedup flag + load the owner-pinned allowlist
    // ONCE, then SHARE both with the resolver (M2 `_public` closure promote /
    // read) and the blob store (`finalize_upload` write routing). Fail-CLOSED
    // allowlist load: a malformed baked manifest degrades to deny-all (empty).
    let oci_dedup = crate::public_flags::oci_public_dedup_enabled();
    let public_allowlist = PublicBaseAllowlist::from_baked_manifest().unwrap_or_default();

    // WP-G (M1 `OCI_UPSTREAM_ON_MISS` + M2 `OCI_PUBLIC_DEDUP_ENABLED`): build the
    // manifest upstream-on-miss resolver ONLY when the on-miss flag is ON at boot
    // AND the shared SSRF-safe upstream client builds; else `None` — the manifest
    // handlers then 404 a KV miss exactly as today (flag OFF ⇒ byte-identical). It
    // SHARES the moat, the manifest KV, the tenant cap resolver, and (M2) the
    // owner-pinned allowlist + the dedup flag: when dedup is ON a by-digest resolve
    // reads / closure-promotes the shared `_public` namespace cross-tenant; when
    // OFF it is the M1 per-tenant path only. Fail-open at every step. Boot-read, so
    // activation is a repin, not a live flip.
    let manifest_resolver: Option<Arc<dyn ManifestResolver>> =
        if crate::public_flags::oci_upstream_on_miss() {
            crate::routes::public_pullthrough::UpstreamManifestResolver::new(
                Arc::clone(&manifest_kv),
                Arc::clone(&moat),
                cap_resolver.clone(),
                public_allowlist.clone(),
            )
            .map(|r| Arc::new(r) as Arc<dyn ManifestResolver>)
        } else {
            None
        };

    let blob_size_limit_bytes = defaults::BLOB_SIZE_LIMIT_BYTES;
    let cas: Arc<dyn BlobStore> = Arc::new(OciMoatStore::with_allowlist_and_blob_limit(
        moat,
        oci_dedup,
        public_allowlist,
        blob_size_limit_bytes,
    ));
    let resolver: Arc<dyn TenantResolver> = Arc::new(OciPatResolver {
        verifier,
        cap_resolver,
        suspend_resolver: suspend_resolver.clone(),
    });
    let auditor: Arc<dyn AuditEmitter> = Arc::new(InMemoryAuditEmitter::new());

    let config = OciAdapterConfig::new(
        // bind_addr is unused by `router`/`build_router` (only
        // `run_oci_adapter` binds).
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        OCI_BEARER_REALM.to_owned(),
        blob_size_limit_bytes,
        defaults::MULTIPART_CHUNK_SIZE_BYTES,
        // _catalog stays OFF (cross-tenant repo-name leak); the adapter
        // also structurally rejects `true` in `sanity_check`.
        defaults::ENABLE_CATALOG,
        defaults::TOKEN_TTL_SECS,
        token_signing_key,
        cas,
        manifest_kv,
        resolver,
        manifest_resolver,
        auditor,
    );

    // Fail-CLOSED on a bad config (e.g. token key <32 bytes): do NOT
    // mount rather than serve an adapter with a weak session-HMAC key.
    if let Err(e) = config.sanity_check() {
        tracing::error!(error = %e, "/v2 OCI config sanity_check failed; OCI NOT mounted");
        return Router::new();
    }

    let state = AppState::new(
        Arc::new(config),
        corelink_adapter_host::oci::wallclock_unix_ms,
    );
    // `.merge` (NOT `nest_service`): the adapter owns `/v2/*` + `/token`
    // verbatim and the Worker forwards the path unchanged.
    let mut router = Router::new().merge(oci_router(state));
    // Per-tenant monthly $-ceiling gate (ADR-0068): OCI previously bypassed the
    // Worker `$`-ceiling/quota path entirely (cluster B). Charge the flat per-op
    // cost on EVERY method (reads AND writes — `docker pull` GET/HEAD of
    // manifests/blobs has real R2 Class-B/egress COGS, exactly like the native
    // CAS/AC read path) so OCI is subject to the SAME ceiling as
    // CAS/AC/Bazel/Turbo. The $-ceiling charge is fail-CLOSED (402 over). Blob
    // BYTE accrual is already enforced by the `AccountingCasHandler` decorator
    // wrapping the shared `cas_write`.
    //
    // rt-nuclear #2/#8/#9: the cost-attribution tenant is resolved from the
    // VERIFIED HMAC bearer (`gate_realm_key`), NOT a request header. The Worker
    // strips `x-corelink-tenant-id` on the OCI pass-through (so the old header
    // read was always empty ⇒ the charge was always skipped — a total $-ceiling
    // bypass), and trusting an unverified header/token claim would let a tenant
    // charge a victim. A write with no VALID bearer is left uncharged because the
    // adapter's data plane 401s it (no billable work succeeds). 402 over-ceiling.
    //
    // rt-nuclear #8 (request-count half): the SAME middleware also meters OCI
    // requests on EVERY method (reads AND writes) against the tenant's monthly
    // REQUEST-count cap (`monthly_request_counts`, migration 0071) — keyed on
    // the SAME verified bearer. The Worker forwards OCI RAW and returns before
    // its `checkRequestQuota` block, so OCI requests bypassed the request cap
    // exactly as they bypassed the $-ceiling; this closes the sibling gap
    // container-side. 429 over the cap (SLO-style, fail-OPEN — unlike the
    // fail-CLOSED $-ceiling).
    //
    // Wire the layer when EITHER gate is present (they are independent axes; in
    // dev/CI without a D1 storage env BOTH are `None` and no layer mounts).
    if quota.is_some() || request_count.is_some() || suspend_resolver.is_some() {
        let gate_state = OciCostGate {
            gate: quota,
            request_count,
            suspend_resolver,
            realm_key: Arc::new(gate_realm_key),
        };
        router = router.layer(axum::middleware::from_fn_with_state(
            gate_state,
            oci_quota_gate,
        ));
    }
    router
}

/// State for [`oci_quota_gate`]: the per-tenant `$`-ceiling
/// [`QuotaGate`](crate::routes::QuotaGate) and the monthly request-count
/// [`RequestCountGate`](crate::request_count::RequestCountGate) (each optional /
/// independent), plus a copy of the OCI realm HMAC signing key used to VERIFY
/// the bearer token and recover its tenant (the attribution key) — see the
/// `router` doc.
#[derive(Clone)]
struct OciCostGate {
    gate: Option<crate::routes::QuotaGate>,
    request_count: Option<crate::request_count::RequestCountGate>,
    /// G4b RESIDUAL suspend gate on the `/v2/*` legs. A bearer minted BEFORE
    /// suspension stays valid for `TOKEN_TTL_SECS` (300 s), so the token-leg gate
    /// alone would leak up to 5 min on a mid-session suspend. Re-checking here
    /// 403s a suspended tenant before `next.run`. Same fail-CLOSED-for-known /
    /// fail-OPEN-for-unknown posture as the token leg.
    suspend_resolver: Option<Arc<dyn crate::oci_suspend::SuspendResolver>>,
    realm_key: Arc<SecretWrap>,
}

/// Recover the tenant from a `/v2/*` request's `Authorization: Bearer <token>`
/// header by VERIFYING the adapter's HMAC realm token (`corelink_adapter_host::
/// oci::auth::verify`). Returns `None` when there is no bearer or it fails
/// verification (expired / forged / malformed) — the data plane 401s those, so
/// the gate leaves them uncharged. Verifying (not just parsing) is load-bearing:
/// the tenant segment is attacker-controlled, so an unverified read would let one
/// tenant bill another.
// SECURITY-REVIEW (OCI trust path, audit #7 — INFO, pending review): OCI resolves
// the attribution tenant from the HMAC-verified realm bearer here — a SEPARATE
// trust path from the header-based planes (native CAS/AC trust a Worker-injected
// `x-corelink-tenant-id`). Accepted pending review: anon reads with no resolvable
// bearer go UNCOUNTED. Note: for a RESOLVED bearer the $-ceiling is now charged
// fail-CLOSED on reads too (402 over); only the request-count axis is fail-OPEN
// (429). See the findings doc (#7) — the actual trust-path review is a separate task.
fn oci_bearer_tenant(realm_key: &SecretWrap, headers: &axum::http::HeaderMap) -> Option<String> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    corelink_adapter_host::oci::auth::verify(realm_key, token, now)
        .ok()
        .map(|vt| vt.tenant.to_canonical_text())
}

/// Per-tenant quota charge for OCI requests. BOTH axes are metered on EVERY
/// method (reads AND writes) — `docker pull` (GET/HEAD of manifests/blobs) does
/// real, billable work on the shared multi-tenant cache (R2 Class-B GETs +
/// egress), so leaving it uncharged let a free tenant loop pulls to evade the
/// monthly request cap and burn unmetered egress (rt-nuclear r34 #1/#11). The
/// `$`-ceiling axis is fail-CLOSED (402 over, charged on reads too — matching
/// the native CAS/AC read path; a prior write-only carve-out, PR #318, let an
/// authenticated tenant pull unlimited blobs without ever hitting their ceiling);
/// the request-count axis is fail-OPEN (429 over). See the `router` doc.
async fn oci_quota_gate(
    axum::extract::State(st): axum::extract::State<OciCostGate>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Attribution tenant comes from the VERIFIED HMAC bearer, never a request
    // header: the Worker strips `x-corelink-tenant-id` on the OCI pass-through
    // (the old header read was always empty ⇒ charge always skipped — a total
    // bypass, rt-nuclear #2/#8/#9). A request with no valid bearer is 401'd by
    // the adapter data plane (writes) / authorized by bearer-scope (reads), so it
    // is left unmetered exactly as before — we only ADD metering when a tenant is
    // resolvable, preserving the current auth semantics for reads.
    if let Some(tenant) = oci_bearer_tenant(&st.realm_key, req.headers()) {
        // G4b RESIDUAL suspend gate (403, fail-CLOSED for a KNOWN-suspended
        // tenant): checked FIRST, before any billable work. A bearer minted
        // BEFORE the tenant was suspended stays HMAC-valid for TOKEN_TTL_SECS
        // (300 s), so the `/token`-leg gate alone leaks a mid-session suspend for
        // up to 5 min; this re-check closes that window on every op. A
        // known-suspended tenant stays denied even through a D1 read fault (the
        // resolver folds that into a deny); an unknown tenant whose read errors
        // fails OPEN (never `Ok(true)`) so a D1 outage cannot 403 the fleet.
        if let Some(sr) = st.suspend_resolver.as_ref() {
            if matches!(sr.suspended_state(&tenant).await, Ok(true)) {
                return axum::response::IntoResponse::into_response((
                    axum::http::StatusCode::FORBIDDEN,
                    "tenant suspended; OCI registry access is revoked",
                ));
            }
        }
        // $-ceiling (fail-CLOSED, 402 over): charged on EVERY method INCLUDING
        // reads (rt-nuclear cycle-2 #3). R2 Class-B GETs have real COGS, and the
        // native CAS/AC plane (cas.rs / ac.rs) charges reads against the same
        // per-tenant monthly $-ceiling. The previous write-only gate (PR #318)
        // let an authenticated tenant pull unlimited OCI blobs/manifests without
        // ever hitting their ceiling — unmetered egress/R2-GET cost-amplification
        // and a read-path carve-out the native plane does not have. Unauthenticated
        // reads (no resolvable bearer tenant) remain unmetered, exactly as before.
        if let Some(gate) = st.gate.as_ref() {
            if let Some(resp) = gate.check(&tenant).await {
                return resp;
            }
        }
        // Monthly request-count cap (fail-OPEN, 429 over — rt-nuclear #8/#11):
        // charged on EVERY method (reads included). Run AFTER the $-ceiling so an
        // over-budget write rejects 402 before it consumes a request-count slot
        // (the two checks are independent axes; ordering only matters for which
        // response wins on a write that trips both, and the $-cap is the harder
        // business guarantee). It is fail-OPEN per request, so charging GET/HEAD
        // never produces a false 402.
        if let Some(rc) = st.request_count.as_ref() {
            if let Some(resp) = rc.check_and_increment(&tenant).await {
                return resp;
            }
        }
    }
    next.run(req).await
}

// No `oci_gate`: per-op authorization is the adapter's bearer-scope
// enforcement (`scope.allows(repo, action)` on every `/v2` op) plus the
// `/token` downscope to the PAT's capability (see `router` doc). The Worker
// forwards OCI raw, so there is no server-set `x-corelink-scope` to gate on —
// a header gate would 403 every request under pass-through.
//
// F26 — security note: OCI `x-corelink-scope` gate exception (two-leg pass-through)
//
// The Worker intentionally does NOT inject `x-corelink-scope` on any OCI request
// (both `/token` and `/v2/*`). The OCI Distribution Spec v1.1 §auth two-leg flow
// requires that the client presents `Authorization: Basic <pat>` to `/token` and
// receives an HMAC bearer. Because the Worker cannot resolve the PAT scope at
// forward time (it does not re-verify the PAT), it cannot set the header; a gate
// here would `403` every OCI request, including the initial token exchange.
//
// Compensating controls that make this safe:
//   1. `/token` runs the full Option-B PAT re-verify (HMAC + D1 lookup + Argon2id
//      via `OciPatResolver` → `PatVerifier::verify_capability`). An invalid or
//      revoked PAT returns 401 before a bearer is minted.
//   2. The minted bearer carries the PAT's REAL capability (read vs read/write) via
//      the `can_write` downscope path (`OciScope::restricted_to_read` on `cas:r`
//      PATs), so a read-only PAT cannot obtain a `push` bearer.
//   3. Every `/v2/*` data-plane op verifies the HMAC bearer locally
//      (`crate::oci::auth::verify`) and enforces `scope.allows(repo, action)`
//      before touching any port. There is no unauthenticated code path.
//   4. The per-tenant upload-session cap (F25, `OCI_MAX_OPEN_SESSIONS_PER_TENANT`)
//      limits in-memory abuse from a valid but malicious authenticated tenant.
//
// Net: the absence of `x-corelink-scope` is a necessary protocol accommodation,
// not a gap. The PAT re-verify + bearer scope-downscope + per-op scope enforcement
// provide equivalent or stronger defence than the header gate would on the other
// adapters (which trust the Worker-injected header rather than re-verifying).
//
// If the Worker is extended to resolve OCI PAT scopes at forward time, the gate
// SHOULD be added for defence-in-depth — but doing so requires the Worker to
// perform Argon2id-equivalent work on every OCI call, which is out of scope.

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    include!("b126_m2_test_1_1.rs");
    include!("b126_m2_test_1_2.rs");
    include!("b126_m2_test_1_3.rs");

    #[test]
    fn b126_m2_test_fragments_are_wired() {
        let _ = [
            B126_M2_TEST_1_1_REANCHOR,
            B126_M2_TEST_1_2_REANCHOR,
            B126_M2_TEST_1_3_REANCHOR,
        ];
    }
}

#[allow(dead_code)]
const B126_M2_IMPL_2_REANCHOR: () = ();
