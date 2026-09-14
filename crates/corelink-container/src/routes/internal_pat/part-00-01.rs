// ──────────────────────────────────────────────────────────────────────────────
// State
// ──────────────────────────────────────────────────────────────────────────────

/// Route state injected at boot time.
#[derive(Clone)]
#[non_exhaustive]
pub struct InternalPatRouteState {
    /// Shared secret for `X-Corelink-Internal-Auth` header.
    pub internal_auth_key: Arc<str>,
    /// PAT HMAC signing key (sourced from `PAT_SIGNING_KEY` env var).
    pub signing_key: Arc<PatSigningKey>,
    /// Signing key generation (monotonic counter; 1 at boot).
    pub signing_key_id: u32,
    /// Concurrency backstop bounding simultaneous Argon2id mints
    /// (red-team #7). Shared across clones so the ceiling is process-wide.
    pub inflight: Arc<MintInflightLimiter>,
    /// Fixed-window RATE limiter bounding mint THROUGHPUT (cluster G).
    /// Complements `inflight` (concurrency): an internal-auth holder firing
    /// SERIAL mints stays under any concurrency cap but is bounded here.
    /// Shared across clones so the window is process-wide.
    pub rate: Arc<MintRateLimiter>,
}

impl std::fmt::Debug for InternalPatRouteState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InternalPatRouteState")
            .field("internal_auth_key", &"[REDACTED]")
            .field("signing_key", &"[REDACTED]")
            .field("signing_key_id", &self.signing_key_id)
            .field("inflight", &self.inflight)
            .field("rate", &self.rate)
            .finish()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Request / response shapes
// ──────────────────────────────────────────────────────────────────────────────

/// JSON request body.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct MintRequest {
    /// Tenant UUID (UUIDv7 preferred; any valid UUID accepted).
    pub tenant_id: Uuid,
    /// Principal UUID (typically same as Clerk user UUID at first PAT mint).
    pub principal_id: Uuid,
    /// Scope label: `"admin"` (full admin + cache) or `"cas:rw"` (cache only).
    pub scopes: String,
    /// Token TTL in seconds. 0 or absent → no expiry (not recommended for
    /// production; use 365 * 86400 = 31536000 for annual rotation).
    pub ttl_seconds: Option<u64>,
}

/// JSON response body. NEVER log `token_plaintext`.
#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MintResponse {
    /// The PAT plaintext. Returned ONCE to the caller; caller writes to
    /// Clerk metadata and then discards.
    pub token_plaintext: String,
    /// UUID of the newly minted PAT row (D1 `pat.pat_id`).
    pub pat_id: String,
    /// 16-char Crockford b32 D1 lookup key (D1 `pat.token_id`).
    pub token_id: String,
    /// Expiry epoch milliseconds (0 if no-expiry).
    pub expires_ms: u64,
    /// Argon2id PHC hash string for D1 `pat.pat_hash` column.
    pub hash: String,
}

// ──────────────────────────────────────────────────────────────────────────────
// Security helpers
// ──────────────────────────────────────────────────────────────────────────────

/// HTTP header carrying the shared internal-auth secret.
const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Constant-time verification of the `X-Corelink-Internal-Auth` header
/// against the shared secret (M2 fix).
///
/// The compare pads the provided value to the expected length and runs a
/// single `ct_eq` over equal-length buffers, then folds in a length-equality
/// bit — so NEITHER the secret length NOR its content is leaked via an early
/// return / branch. Mirrors `admin.rs::internal_auth_ok` (PR #152) exactly so
/// the two internal-auth gates stay byte-for-byte consistent.
///
/// - empty / missing header → `false` (the empty provided value pads to the
///   secret length but the length-equality bit is 0, so it can never match a
///   non-empty secret).
///
/// Exposed `pub(crate)` so the fabric introspection route
/// (`routes::auth_introspect`) reuses this exact constant-time gate against
/// its OWN dedicated secret rather than reinventing the compare.
#[must_use]
pub(crate) fn internal_auth_ok(expected: &[u8], headers: &HeaderMap) -> bool {
    let provided = headers
        .get(INTERNAL_AUTH_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let provided_bytes = provided.as_bytes();
    // Pad provided to expected length to run ct_eq on equal-length slices,
    // then fold in the real length-equality so a longer/shorter provided
    // value can never match. No branch short-circuits on the secret length.
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

/// Hash a value to a short, stable hex correlation handle for logging
/// (M6 fix, INV-NO-PII-IN-LOGS). First 8 bytes of SHA-256, hex-encoded —
/// consistent with the `hashForLog` helper in `worker/src/durable_object.ts`.
/// Never log the raw tenant UUID; log this handle instead.
#[must_use]
fn hash_for_log(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut out = String::with_capacity(16);
    for b in digest.iter().take(8) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

// ──────────────────────────────────────────────────────────────────────────────
// Route handler
// ──────────────────────────────────────────────────────────────────────────────

/// Build the internal-pat router. Mount at the top-level so
/// `/_internal/pat/mint` is directly addressable.
pub fn router(state: InternalPatRouteState) -> Router {
    Router::new()
        .route("/_internal/pat/mint", post(handle_mint))
        .with_state(state)
}

/// `POST /_internal/pat/mint` handler.
///
/// Security gate: constant-time comparison of the `X-Corelink-Internal-Auth`
/// header against the **dedicated** `CORELINK_PAT_MINT_AUTH_KEY` (DD HIGH
/// remediation — never the shared `CORELINK_INTERNAL_AUTH_KEY`; see
/// [`build_state_from_env`]). Any mismatch or missing header → 401,
/// immediately, BEFORE parsing the body.
///
/// M3 fix: the request body is taken as raw [`Bytes`] (NOT the `Json`
/// `FromRequest` body extractor). `HeaderMap` is a `FromRequestParts`
/// extractor and so runs before the body is buffered; the auth gate is
/// evaluated FIRST and an unauthorized caller is rejected with 401 WITHOUT
/// the body ever being JSON-parsed — denying an unauthenticated attacker the
/// CPU/heap cost of parsing a large body. The body is `serde_json`-decoded
/// only AFTER the auth gate passes.
async fn handle_mint(
    State(state): State<InternalPatRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // ── 1. Shared-secret gate (constant-time, M2) ──────────────────────────────
    // Checked BEFORE the body is parsed (M3): `headers` is FromRequestParts,
    // so this gate runs before any work is done on the (raw, still-unparsed)
    // body buffer.
    if !internal_auth_ok(state.internal_auth_key.as_bytes(), &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    // ── 1a-rate. Fixed-window mint RATE limit (cluster G) ──────────────────────
    // The concurrency backstop below bounds SIMULTANEOUS Argon2id work, but a
    // serial loop of mints (each completing before the next) stays under any
    // concurrency cap while still pinning the container's Argon2id CPU/RAM. Cap
    // the THROUGHPUT too: over the per-window ceiling ⇒ 429. Checked AFTER the
    // auth gate (an unauthenticated flood is already shed at 401, cheaply) and
    // BEFORE a concurrency permit / the Argon2id mint is taken.
    if !state.rate.try_admit() {
        tracing::warn!(
            event = "PatMintRateLimited",
            "internal_pat: mint shed — mint rate limit exceeded (429)"
        );
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({ "error": "too_many_requests" })),
        )
            .into_response();
    }

    // ── 1a. Concurrency backstop (red-team #7) ─────────────────────────────────
    // Bound the number of SIMULTANEOUS Argon2id mints so a loop (even an
    // authenticated one) cannot exhaust CPU/RAM. The RAII `_slot` releases
    // the permit on EVERY return path (drop), including the error returns
    // below. Acquired AFTER the auth gate so an unauthenticated flood is
    // already shed at 401 (cheap) and never consumes a mint permit.
    let _slot = match state.inflight.try_acquire() {
        Some(slot) => slot,
        None => {
            tracing::warn!(
                event = "PatMintShed",
                "internal_pat: mint shed — too many concurrent mints (429)"
            );
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({ "error": "too_many_requests" })),
            )
                .into_response();
        }
    };

    // ── 1b. Parse the JSON body — ONLY after the auth gate passed (M3) ─────────
    let req: MintRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "internal_pat: invalid request body");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid_body" })),
            )
                .into_response();
        }
    };

    // ── 2. Map scope label to PatScopes bitset ─────────────────────────────────
    // Canonical label set = the persisted `pat.scope` CHECK domain
    // (`read-only`/`read-write`/`admin`); `cas:rw` is a back-compat alias of
    // `read-write`. See [`scope_label_to_bits`].
    let scopes = match scope_label_to_bits(req.scopes.as_str()) {
        Some(s) => s,
        None => {
            let other = req.scopes.as_str();
            tracing::warn!(scope = other, "internal_pat: unknown scope label");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid_scope", "scope": other })),
            )
                .into_response();
        }
    };

    // ── 3. Mint the PAT ────────────────────────────────────────────────────────
    let ttl = req.ttl_seconds.and_then(|s| {
        if s == 0 {
            None
        } else {
            Some(Duration::from_secs(s))
        }
    });

    let tenant_id = TenantId(req.tenant_id);
    let principal_id = PrincipalId(req.principal_id);

    let (plaintext, pat) = match mint(
        PatEnv::Pat,
        tenant_id,
        principal_id,
        scopes,
        ttl,
        &state.signing_key,
        state.signing_key_id,
    ) {
        Ok(r) => r,
        Err(e) => {
            // Log the real PatError detail SERVER-SIDE only — never in the response.
            // `e.to_string()` discloses signing-key/entropy/hash-corruption internals
            // (e.g. `SigningKeyTooShort`) which are operationally sensitive even to a
            // holder of CORELINK_PAT_MINT_AUTH_KEY. Return an OPAQUE body; operators
            // recover the cause from this structured log line.
            tracing::error!(error = %e, "internal_pat: mint failed");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "mint_failed" })),
            )
                .into_response();
        }
    };

    // ── 4. Compute expires_ms ─────────────────────────────────────────────────
    let expires_ms: u64 = pat
        .expires_at
        .and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_millis() as u64)
        })
        .unwrap_or(0);

    // ── 5. Audit emit BEFORE returning the response ────────────────────────────
    // PatMinted audit is lightweight (tenant_id + pat_id + scopes, no
    // plaintext). The container has no D1 binding on the native path;
    // for now we emit a structured tracing event which the CF Logs
    // pipeline ingests. Full D1 audit emit deferred to Wave-37.
    // M6: log a SHA-256-derived correlation handle, NOT the raw tenant UUID
    // (INV-NO-PII-IN-LOGS). `tenant_hash` is a stable 8-byte hex digest.
    tracing::info!(
        tenant_hash = %hash_for_log(&req.tenant_id.to_string()),
        pat_id = %pat.id,
        token_id = %pat.token_id,
        expires_ms = expires_ms,
        scope_bits = pat.scopes.to_u64(),
        event = "PatMinted",
        "internal_pat: PAT minted (plaintext NEVER logged)"
    );

    let resp = MintResponse {
        token_plaintext: plaintext.into_string(),
        pat_id: pat.id.to_string(),
        token_id: pat.token_id.as_str().to_owned(),
        expires_ms,
        hash: pat.hash.into_string(),
    };

    (StatusCode::OK, Json(resp)).into_response()
}

// ──────────────────────────────────────────────────────────────────────────────
// State builder
// ──────────────────────────────────────────────────────────────────────────────

/// Decode a hex string to bytes. Returns `None` on invalid hex.
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_nibble(*bytes.get(i)?)?;
        let lo = hex_nibble(*bytes.get(i + 1)?)?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Some(out)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Resolve the **dedicated** mint auth key from its raw env value
/// (DD HIGH remediation).
///
/// Unlike every other internal surface — which resolves via
/// [`crate::routes::admin::resolve_internal_auth_key`] and falls back to the
/// broad shared `CORELINK_INTERNAL_AUTH_KEY` — the any-tenant PAT mint is gated
/// ONLY by the dedicated `CORELINK_PAT_MINT_AUTH_KEY`. This resolver therefore
/// reads NOTHING but the value it is handed and **never** consults the shared
/// key: a leak of the shared signup-worker secret cannot, by itself, exercise
/// the any-tenant (incl. `SCOPE_ADMIN_ALL`) mint.
///
/// Fail-CLOSED: an absent / blank / `< INTERNAL_AUTH_KEY_MIN_LEN`-char value
/// yields `None`. The caller then declines to mount the route, so the endpoint
/// is unavailable (503) rather than silently widening to the shared key. The
/// `≥ 32`-char floor matches the other internal keys
/// ([`crate::routes::admin::INTERNAL_AUTH_KEY_MIN_LEN`]); the secrets-checklist
/// instructs `openssl rand -hex 32` (64 chars).
///
/// Kept as a pure function (env value in, decision out) so the no-fallback /
/// fail-closed gate is unit-testable without racing the process environment.
#[must_use]
fn resolve_mint_auth_key(dedicated: Option<&str>) -> Option<Arc<str>> {
    match dedicated {
        Some(key) if key.len() >= crate::routes::admin::INTERNAL_AUTH_KEY_MIN_LEN => {
            Some(Arc::from(key))
        }
        _ => None,
    }
}
