/// HTTP header carrying the shared internal-auth secret (mirrors
/// [`crate::routes::internal_pat`] byte-for-byte).
const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Canonical erase route path (matchit-0.7 `:name` captures — see the DEBT-029
/// note in [`crate::routes::cas`]).
pub const CAS_ERASE_ROUTE: &str = "/_internal/cas/{tenant}/{hash}/erase";

/// Max accepted audit reason length (bounds the D1 row; never PII).
const MAX_REASON_LEN: usize = 256;

// ──────────────────────────────────────────────────────────────────────────────
// Collaborator traits
// ──────────────────────────────────────────────────────────────────────────────

/// Durable store for the 410-Gone tombstones (`cas_tombstone`, migration 0067).
#[async_trait]
pub trait TombstoneStore: Send + Sync + std::fmt::Debug {
    /// Is `(tenant, digest)` tombstoned? Drives the read-path 410 gate.
    async fn is_tombstoned(&self, tenant: &str, digest: &str) -> Result<bool, String>;

    /// AUTHORITATIVE tombstone check — MUST consult the durable store on every
    /// call, NEVER a cached/bloom fast-path negative.
    ///
    /// The **write** gate uses this (finding H3). A within-window bloom
    /// false-negative is *tolerable on the READ path* (invariant 1: the bytes
    /// are already gone from R2, so a GET that slips past the gate 404s rather
    /// than serving erased content), but on the **WRITE path** the same
    /// false-negative would let a re-PUT **RESURRECT legally-erased bytes** at
    /// the same content address — the erasure attestation becomes false. So a
    /// write must never trust a fast-path `Ok(false)`.
    ///
    /// The default delegates to [`Self::is_tombstoned`] — already authoritative
    /// for the D1 / in-memory stores. [`BloomTombstoneStore`] overrides it to
    /// bypass its own bloom fast-path and always hit the inner (D1) store.
    async fn is_tombstoned_authoritative(
        &self,
        tenant: &str,
        digest: &str,
    ) -> Result<bool, String> {
        self.is_tombstoned(tenant, digest).await
    }

    /// Upsert a tombstone (idempotent). Returns whether a row already existed
    /// (so the route can report `AlreadyErased` for an idempotent re-erase).
    async fn upsert(
        &self,
        tenant: &str,
        digest: &str,
        reason: &str,
        erased_at_ms: i64,
    ) -> Result<bool, String>;

    /// List ALL tombstoned digests for `tenant` (the durable per-tenant set).
    ///
    /// Backs the [`BloomTombstoneStore`] (re)load so its in-memory bloom is
    /// seeded from the AUTHORITATIVE D1 set — including tombstones written by
    /// ANOTHER container instance (or by the separate erase-route `D1TombstoneStore`
    /// that does not share this bloom). Without this seed the bloom only knew its
    /// OWN in-process writes, so a cross-writer tombstone read as NOT-present for
    /// the whole refresh window after its first read (F-010 false-negative — a
    /// GDPR no-false-negative invariant violation). Erasures are RARE (Art.17 +
    /// per-blob operator erases), so a per-tenant enumerate is cheap and bounded.
    ///
    /// # Errors
    /// Propagates the underlying store transport error; the bloom (re)load treats
    /// an `Err` as "could not seed" and downgrades to the authoritative
    /// fall-through path (never a silent stale `Ok(false)`).
    async fn list_tenant_tombstones(&self, tenant: &str) -> Result<Vec<String>, String>;
}

/// Hard-deletes a single content-addressed blob's bytes from R2.
///
/// The production impl composes the DSR Wave 1 R2 CAS primitives (PR #254);
/// see the module-level "Composition with DSR Wave 1" note.
#[async_trait]
pub trait CasBlobEraser: Send + Sync + std::fmt::Debug {
    /// Delete every R2 object for `(tenant, digest)` across the CAS regions.
    /// Idempotent: deleting an absent object is a no-op success.
    async fn erase_blob(&self, tenant: &str, digest: &str) -> Result<(), String>;
}

// ──────────────────────────────────────────────────────────────────────────────
// Route state
// ──────────────────────────────────────────────────────────────────────────────

/// Shared state for the erase route.
#[derive(Clone)]
#[non_exhaustive]
pub struct CasEraseRouteState {
    /// Tombstone persistence (D1 in prod).
    pub tombstones: Arc<dyn TombstoneStore>,
    /// R2 blob byte-deletion (the #254 seam).
    pub eraser: Arc<dyn CasBlobEraser>,
    /// Shared internal-auth secret (constant-time compared).
    pub internal_auth_key: Arc<str>,
    /// DSR legitimacy pre-check (rt-nuclear #18/#19, r34 #8/#9). Binds the
    /// per-blob erase to a durable, D1-authenticated `dsr_requested` row so a
    /// leaked internal/erase key alone CANNOT erase arbitrary blobs: the
    /// attacker cannot forge a `dsr_requested` row (written only by the
    /// D1-authenticated Clerk `user.deleted` path). `None` only in the
    /// in-memory/test fallback; `build_state_from_env` fail-CLOSES (route
    /// unmounted) if it cannot be built in prod, so a `None` here on a
    /// mounted prod route is unreachable — and the handler treats `None` as
    /// DENY (503) regardless.
    pub legitimacy: Option<Arc<dyn DsrLegitimacyStore>>,
}

impl core::fmt::Debug for CasEraseRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CasEraseRouteState")
            .field("internal_auth_key", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Build the erase router. Mounted top-level so the path is directly
/// addressable from the DO (mirrors `internal_pat`).
pub fn router(state: CasEraseRouteState) -> Router {
    Router::new()
        .route(CAS_ERASE_ROUTE, post(handle_erase))
        .with_state(state)
}

/// Erase request body. `tenant` is the authenticated tenant; `dsr_id` is the
/// canonical UUID of the DSR ticket that legitimately requested this erasure
/// (REQUIRED — missing/invalid ⇒ 400 at deserialization); `reason` is a
/// bounded, free-form audit string.
#[derive(Debug, serde::Deserialize)]
struct EraseBody {
    tenant: String,
    /// DSR request id. The erase is authorised ONLY if a live `dsr_requested`
    /// row exists for `(dsr_id, tenant)` — see [`CasEraseRouteState::legitimacy`].
    dsr_id: Uuid,
    #[serde(default)]
    reason: String,
}

/// `POST /_internal/cas/:tenant/:hash/erase`.
///
/// Order (fail-CLOSED): internal-auth gate (constant-time, BEFORE body parse) →
/// parse body → cross-tenant + digest validation (pure handler) → R2 delete →
/// tombstone upsert. The tombstone is written AFTER the bytes are gone so a
/// crash between the two leaves bytes-gone-without-tombstone (a subsequent GET
/// 404s, never serves bytes) rather than tombstone-without-bytes — the safe
/// failure direction; the operation is idempotent so a retry completes it.
async fn handle_erase(
    State(state): State<CasEraseRouteState>,
    Path((path_tenant, hash)): Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // 1. Internal-auth gate — BEFORE the body is parsed (mirrors internal_pat).
    if !internal_auth_ok(state.internal_auth_key.as_bytes(), &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    // 2. Parse the body only after auth passed.
    let req: EraseBody = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "cas_erase: invalid request body");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid_body" })),
            )
                .into_response();
        }
    };

    // 3. Bound the audit reason.
    if req.reason.len() > MAX_REASON_LEN {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "reason_too_long" })),
        )
            .into_response();
    }

    // 4. Pure validation → tombstone marker (cross-tenant + digest shape).
    let erase_req = corelink_handler_cas_erase::CasEraseRequest::new(
        req.tenant.clone(),
        path_tenant,
        hash.clone(),
    );
    let _marker = match prepare_erase(&erase_req, &req.reason) {
        Ok(m) => m,
        Err(corelink_handler_cas_erase::CasEraseError::CrossTenantDenied { .. }) => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "cross-tenant" })),
            )
                .into_response();
        }
        Err(corelink_handler_cas_erase::CasEraseError::InvalidDigest { .. }) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid_digest" })),
            )
                .into_response();
        }
        Err(corelink_handler_cas_erase::CasEraseError::Transport(_)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal" })),
            )
                .into_response();
        }
        // `CasEraseError` is `#[non_exhaustive]`: a future variant must
        // fail-CLOSED (never silently fall through to the erase) — map any
        // unknown error to 500 so a new error kind can never ship without an
        // explicit decision here.
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal" })),
            )
                .into_response();
        }
    };

    // 4b. DSR legitimacy gate (rt-nuclear #18/#19, r34 #8/#9). The
    //     internal-auth gate alone proves only "holds the shared/erase key";
    //     it does NOT prove "this erasure was legitimately requested". A
    //     leaked key would otherwise allow arbitrary cross-tenant blob
    //     deletion + permanent 410-Gone poison with no proof-of-request.
    //     Bind the erase to a durable, D1-authenticated `dsr_requested` row
    //     for THIS (dsr_id, tenant) — which a leaked-key attacker cannot
    //     forge. Placed AFTER cross-tenant/digest validation and BEFORE the
    //     irreversible R2 delete. Fail-CLOSED on every ambiguity.
    //
    //     The tenant is bound as a UUID so a `dsr_id` legitimate for tenant A
    //     cannot authorise erasing tenant B (`is_requested` keys on BOTH).
    let tenant_uuid = match Uuid::try_parse(&req.tenant) {
        Ok(u) => u,
        Err(_) => {
            // A non-UUID tenant can never match a `dsr_requested` row (the
            // table stores canonical UUID tenant ids written by the
            // D1-authenticated Clerk path) → DENY rather than bypass the gate.
            tracing::warn!(
                "cas_erase: non-UUID tenant on erase — denying (no legitimacy possible)"
            );
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "forbidden" })),
            )
                .into_response();
        }
    };
    match &state.legitimacy {
        Some(store) => match store.is_requested(req.dsr_id, tenant_uuid) {
            Ok(true) => { /* legitimate — proceed */ }
            Ok(false) => {
                // No live `dsr_requested` row for (dsr_id, tenant): not a
                // legitimate erasure request (or wrong tenant). 403.
                tracing::warn!(
                    dsr_id = %req.dsr_id,
                    "cas_erase: no dsr_requested row for (dsr_id, tenant) — forbidden"
                );
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "forbidden" })),
                )
                    .into_response();
            }
            Err(e) => {
                // Ambiguous legitimacy (D1 fault). Erasure is IRREVERSIBLE,
                // so a store error MUST DENY (fail-CLOSED), never proceed.
                tracing::error!(error = %e, "cas_erase: legitimacy lookup failed — fail-CLOSED (503)");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({ "error": "legitimacy_unavailable" })),
                )
                    .into_response();
            }
        },
        None => {
            // No legitimacy store wired. In prod this is unreachable
            // (`build_state_from_env` fail-CLOSES to an unmounted route), but
            // if it ever happens the irreversible erase MUST NOT run.
            tracing::error!("cas_erase: legitimacy store absent — fail-CLOSED (503)");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "legitimacy_unavailable" })),
            )
                .into_response();
        }
    }

    // 5. Sample prior tombstone presence (for idempotent-outcome reporting).
    let was_tombstoned = match state.tombstones.is_tombstoned(&req.tenant, &hash).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "cas_erase: tombstone lookup failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal" })),
            )
                .into_response();
        }
    };

    // 6. Delete the R2 bytes (idempotent).
    if let Err(e) = state.eraser.erase_blob(&req.tenant, &hash).await {
        tracing::error!(error = %e, "cas_erase: R2 blob erase failed");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "internal" })),
        )
            .into_response();
    }

    // 7. Upsert the tombstone (idempotent).
    let now_ms = now_unix_ms();
    if let Err(e) = state
        .tombstones
        .upsert(&req.tenant, &hash, &req.reason, now_ms)
        .await
    {
        tracing::error!(error = %e, "cas_erase: tombstone upsert failed");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "internal" })),
        )
            .into_response();
    }

    let outcome = erase_outcome(was_tombstoned);
    let outcome_str = match outcome {
        EraseOutcome::Erased => "erased",
        EraseOutcome::AlreadyErased => "already_erased",
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": outcome_str })),
    )
        .into_response()
}

/// Constant-time internal-auth check (mirrors
/// [`crate::routes::internal_pat`] `internal_auth_ok` byte-for-byte).
#[must_use]
fn internal_auth_ok(expected: &[u8], headers: &HeaderMap) -> bool {
    let provided = headers
        .get(INTERNAL_AUTH_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let provided_bytes = provided.as_bytes();
    let provided_padded: Vec<u8> = if provided_bytes.len() >= expected.len() {
        provided_bytes.get(..expected.len()).unwrap_or(&[]).to_vec()
    } else {
        let mut v = provided_bytes.to_vec();
        v.resize(expected.len(), 0);
        v
    };
    let content_ok = expected.ct_eq(&provided_padded).unwrap_u8();
    let len_ok = u8::from(expected.len() == provided_bytes.len());
    (content_ok & len_ok) == 1
}

/// Current Unix time in milliseconds (saturating; 0 on a pre-epoch clock).
#[must_use]
fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

// ──────────────────────────────────────────────────────────────────────────────
// D1-backed tombstone store
// ──────────────────────────────────────────────────────────────────────────────

/// `cas_tombstone` D1-over-HTTP tombstone store (migration 0067).
#[non_exhaustive]
pub struct D1TombstoneStore {
    d1: Arc<crate::storage::d1_http::D1HttpClient>,
}

impl std::fmt::Debug for D1TombstoneStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D1TombstoneStore").finish_non_exhaustive()
    }
}

impl D1TombstoneStore {
    /// Build over a shared [`D1HttpClient`](crate::storage::d1_http::D1HttpClient).
    #[must_use]
    pub fn new(d1: Arc<crate::storage::d1_http::D1HttpClient>) -> Self {
        Self { d1 }
    }

    /// Build from env (`StorageEnv`); `None` when D1 env is absent.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let env = crate::storage::StorageEnv::from_env()?;
        match crate::storage::d1_http::D1HttpClient::new(&env) {
            Ok(c) => Some(Self::new(Arc::new(c))),
            Err(e) => {
                tracing::warn!(error = %e, "cas_erase: D1 client build failed");
                None
            }
        }
    }
}

#[async_trait]
impl TombstoneStore for D1TombstoneStore {
    async fn is_tombstoned(&self, tenant: &str, digest: &str) -> Result<bool, String> {
        let rows = self
            .d1
            .query(
                "SELECT 1 AS present FROM cas_tombstone \
                 WHERE tenant_id = ?1 AND digest = ?2 LIMIT 1",
                &[serde_json::json!(tenant), serde_json::json!(digest)],
            )
            .await?;
        Ok(!rows.is_empty())
    }

    async fn upsert(
        &self,
        tenant: &str,
        digest: &str,
        reason: &str,
        erased_at_ms: i64,
    ) -> Result<bool, String> {
        let existed = self.is_tombstoned(tenant, digest).await?;
        // INSERT OR REPLACE keyed on (tenant_id, digest) → idempotent re-erase.
        self.d1
            .query(
                "INSERT INTO cas_tombstone (tenant_id, digest, reason, erased_at_ms) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(tenant_id, digest) DO UPDATE SET \
                 reason = excluded.reason, erased_at_ms = excluded.erased_at_ms",
                &[
                    serde_json::json!(tenant),
                    serde_json::json!(digest),
                    serde_json::json!(reason),
                    serde_json::json!(erased_at_ms),
                ],
            )
            .await?;
        Ok(existed)
    }

    async fn list_tenant_tombstones(&self, tenant: &str) -> Result<Vec<String>, String> {
        let rows = self
            .d1
            .query(
                "SELECT digest FROM cas_tombstone WHERE tenant_id = ?1",
                &[serde_json::json!(tenant)],
            )
            .await?;
        let mut digests = Vec::with_capacity(rows.len());
        for row in &rows {
            if let Some(d) = row.get("digest").and_then(serde_json::Value::as_str) {
                digests.push(d.to_owned());
            }
        }
        Ok(digests)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Bloom-fronted tombstone store (WP-2b — the read-hot-path D1 round-trip cut)
// ──────────────────────────────────────────────────────────────────────────────
