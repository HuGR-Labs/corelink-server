// `POST /_internal/pat/mint` — server-side PAT mint for the Clerk
// webhook auto-provision flow (Stream-5).
//
// # Security model
//
// **Reachability warning (F4):** `/_internal/pat/mint` is matched and
// served by the *public* edge Worker (`corelink-api.humangr.com/*`) and
// is reachable from the public internet. The sole gate is a constant-time
// compare of the caller-supplied `x-corelink-internal-auth` header against
// the **dedicated** `CORELINK_PAT_MINT_AUTH_KEY` secret. The compare is
// fail-closed and uses padded `ct_eq` so neither secret length nor content
// leaks via an early branch.
//
// **DD HIGH remediation — dedicated key, NO shared-key fallback:** because
// this surface can mint ANY tenant's PAT (including `SCOPE_ADMIN_ALL`), its
// gate is keyed ONLY to the dedicated `CORELINK_PAT_MINT_AUTH_KEY` and MUST
// NOT fall back to the broad shared `CORELINK_INTERNAL_AUTH_KEY` (unlike the
// other internal surfaces, which resolve via
// [`crate::routes::admin::resolve_internal_auth_key`]). The dedicated key is
// REQUIRED in prod; if it is unset/blank/`< 32` chars the route fails CLOSED
// — it is NOT mounted and the endpoint is unavailable (503) rather than
// silently widening the mint to the shared signup-worker credential.
//
// **Recommended hardening (tracked, not yet implemented):** (1) restrict
// `/_internal/pat/mint` to a Worker-to-Worker Service Binding (no public
// route); (2) add per-tenant authorisation to the mint.
//
// **PAT verification on native routes (F3):** the container's native
// data-plane routes (CAS, AC, Bazel REAPI, Turbo) do NOT perform a
// second Argon2id re-verify. Possession is checked once at the edge
// Worker (HMAC fast-fail + D1 expiry lookup); the container trusts the
// Worker-injected `x-corelink-tenant-id` header. Argon2id is wired only
// to the cache-adapter paths (cargo/brew/npm/pip/OCI) via
// `adapter_pat::PatVerifier`. Wiring Argon2id onto the native CAS/AC
// plane is tracked as a TODO (Option-B extension).
//
// The dedicated mint secret is bound to the container via the
// `CORELINK_PAT_MINT_AUTH_KEY` env var (passed at `container.start({ env })`
// — same mechanism as `R2_S3_ENDPOINT`). The Worker AND signup-worker carry
// the same dedicated secret as a Worker secret
// (`wrangler secret put CORELINK_PAT_MINT_AUTH_KEY`); the broad shared
// `CORELINK_INTERNAL_AUTH_KEY` does NOT authorize the mint.
//
// # Request shape
//
// ```text
// POST /_internal/pat/mint
// X-Corelink-Internal-Auth: <secret>
// Content-Type: application/json
//
// {
//   "tenant_id": "<uuid>",
//   "principal_id": "<uuid>",
//   "scopes": "admin",
//   "ttl_seconds": 31536000
// }
// ```
//
// # Response shape (200)
//
// ```text
// {
//   "token_plaintext": "corelink_pat_<token_id>.<random_secret>.<hmac_sig>",
//   "pat_id": "<uuid>",
//   "token_id": "<16-char crockford b32>",
//   "expires_ms": 1234567890000,
//   "hash": "<argon2id phc string>"
// }
// ```
//
// # Hard rules
//
// - `token_plaintext` is NEVER logged or persisted here. The caller is
//   responsible for writing it to Clerk session metadata ONCE and discarding
//   it (CTRL-CRED-001).
// - The audit emit of `PatMinted` happens BEFORE the response is returned
//   (fail-CLOSED per INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
// - Constant-time comparison for the shared secret — length-padded `ct_eq`
//   that leaks NEITHER the secret length NOR its content (M2).
// - The internal-auth gate runs BEFORE the JSON body is parsed: the body is
//   taken as raw bytes and only `serde_json`-decoded after the gate passes,
//   so an unauthenticated caller cannot force body-parse CPU/heap (M3).
// - The raw tenant UUID is NEVER logged; only a SHA-256 correlation handle
//   (`hash_for_log`) is emitted (INV-NO-PII-IN-LOGS, M6).
//
// # Hash-on-wire (M7) — reclassified, not a security follow-up (WP-D L12a)
//
// - **M7** — the 200 response returns the Argon2id `hash` alongside
//   `token_plaintext`. This was originally flagged as a security follow-up;
//   on review it is **NOT an exposure** and is reclassified as an *optional
//   future consolidation* only:
//   - The `hash` is a one-way Argon2id PHC verifier of a HIGH-ENTROPY random
//     secret (the PAT's random segment), not a user password — it is not
//     replayable, and possessing the hash alone does not let a caller
//     construct a valid PAT (verifying still requires the plaintext secret).
//   - It crosses only the **internal Worker ↔ `_system`-DO boundary**
//     (same CF account/isolate, TLS-internal) — the SAME boundary
//     `token_plaintext` itself already crosses on this exact response. If
//     that boundary is trusted enough to carry the plaintext, it is trusted
//     enough to carry a one-way hash of it.
//   - It is never logged (see the hard rules above) and is not persisted by
//     this route — the caller (signup-worker) writes it once to the D1
//     `pat.pat_hash` column and discards it.
//   - Removing it would require moving the D1 `pat`-row write into the
//     container itself — a high-blast-radius change (new write path, new
//     failure modes, signup-worker/container coupling change) for **zero
//     security gain**, since the hash is already safe to transmit on this
//     boundary. Deferred as a possible future consolidation, not a fix.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use corelink_pat::{
    mint::mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId, SCOPE_ADMIN,
    SCOPE_CACHE_R, SCOPE_CACHE_RW,
};

/// Full admin scope: the admin bit + cache rw.
///
/// Formerly the union of six granular `admin:*` bits. Those were decorative —
/// never individually requestable here (this mint accepts only the four labels
/// in [`scope_label_to_bits`], so the six were only ever set *together*), never
/// storable (`pat.scope` is `CHECK (scope IN ('read-write','read-only','admin'))`),
/// and never read by any enforcement point. Collapsed into the single
/// [`SCOPE_ADMIN`] bit (B-080); the granted capability is unchanged.
const SCOPE_ADMIN_ALL: u64 = SCOPE_CACHE_RW | SCOPE_ADMIN;

/// Map a mint-request scope LABEL to its canonical PAT bitset.
///
/// **Canonical label set** = the persisted `pat.scope` CHECK domain
/// (`read-only` / `read-write` / `admin`, D1 migration
/// `0037_signup_orchestration.sql`). This route, the D1 CHECK, and the
/// persist paths (`worker/src/lib/session_exchange.ts::canonicalizePatScope`,
/// `scripts/admin/mint-dogfood-pat.sh`) must all agree on this set.
///
/// - `read-only`  → [`SCOPE_CACHE_R`]   (cache READ only — a witness /
///   read-only credential; MUST NOT carry write or admin bits).
/// - `read-write` → [`SCOPE_CACHE_RW`]  (cache read+write, no admin).
/// - `admin`      → [`SCOPE_ADMIN_ALL`].
/// - `cas:rw`     → **back-compat ALIAS** of `read-write` (existing callers —
///   the token-exchange path sends `cas:rw`). It maps to the SAME bitset as
///   `read-write`; any D1 persist writes the canonical `read-write` (never
///   `cas:rw`, which would violate the CHECK).
///
/// Returns `None` for an unrecognized label so the route fails CLOSED (400).
fn scope_label_to_bits(label: &str) -> Option<PatScopes> {
    match label {
        "admin" => Some(PatScopes::from_u64(SCOPE_ADMIN_ALL)),
        // `cas:rw` is a back-compat alias of the canonical `read-write`.
        "cas:rw" | "read-write" => Some(PatScopes::from_u64(SCOPE_CACHE_RW)),
        "read-only" => Some(PatScopes::from_u64(SCOPE_CACHE_R)),
        _ => None,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Mint concurrency backstop (red-team #7)
// ──────────────────────────────────────────────────────────────────────────────

/// Default ceiling on concurrent in-flight mints (red-team #7).
///
/// Each `/_internal/pat/mint` call runs one Argon2id hash, which is
/// deliberately CPU- AND memory-hard. An authenticated-but-malicious (or
/// looping/buggy) caller firing thousands of concurrent mints would
/// otherwise exhaust the container's CPU/RAM. Bounding the number of
/// SIMULTANEOUS Argon2id computations caps the worst-case memory + CPU
/// footprint regardless of request rate. Excess concurrent calls are
/// rejected with `429 Too Many Requests` (fail-CLOSED — we shed load
/// rather than thrash). 16 is generous for the real signup-webhook caller
/// (one mint per new tenant) while bounding the Argon2id working set to a
/// small multiple of a single hash.
pub const DEFAULT_MAX_INFLIGHT_MINTS: u32 = 16;

/// Env var overriding [`DEFAULT_MAX_INFLIGHT_MINTS`].
pub const MAX_INFLIGHT_MINTS_ENV: &str = "PAT_MINT_MAX_INFLIGHT";

/// A process-global bounded counter of in-flight mints (red-team #7).
///
/// `try_acquire` atomically reserves a slot iff the in-flight count is
/// below `max`; the returned [`MintSlot`] releases the slot on drop (RAII,
/// so an early-return / panic in the handler cannot leak a permit).
#[derive(Debug)]
pub struct MintInflightLimiter {
    inflight: AtomicU32,
    max: u32,
}

impl MintInflightLimiter {
    /// Construct a limiter with the given concurrent-mint ceiling.
    ///
    /// A `max` of 0 is clamped to 1 so the limiter can never wedge the
    /// surface entirely closed via misconfiguration.
    #[must_use]
    pub fn new(max: u32) -> Self {
        Self {
            inflight: AtomicU32::new(0),
            max: max.max(1),
        }
    }

    /// Build the limiter from [`MAX_INFLIGHT_MINTS_ENV`], falling back to
    /// [`DEFAULT_MAX_INFLIGHT_MINTS`] when unset/empty/non-numeric.
    #[must_use]
    pub fn from_env() -> Self {
        let max = std::env::var(MAX_INFLIGHT_MINTS_ENV)
            .ok()
            .and_then(|raw| raw.trim().parse::<u32>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_MAX_INFLIGHT_MINTS);
        Self::new(max)
    }

    /// Atomically reserve an in-flight slot. Returns `Some(MintSlot)` when
    /// a slot was reserved (caller may proceed) or `None` when the ceiling
    /// is already reached (caller must shed with 429).
    #[must_use]
    pub fn try_acquire(self: &Arc<Self>) -> Option<MintSlot> {
        // CAS loop: increment only while strictly under the ceiling, so we
        // never transiently exceed `max` (no over-admission).
        let mut cur = self.inflight.load(Ordering::Acquire);
        loop {
            if cur >= self.max {
                return None;
            }
            match self.inflight.compare_exchange_weak(
                cur,
                cur + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(MintSlot {
                        limiter: self.clone(),
                    })
                }
                Err(actual) => cur = actual,
            }
        }
    }
}

/// RAII permit for one in-flight mint; releases its slot on drop.
#[derive(Debug)]
pub struct MintSlot {
    limiter: Arc<MintInflightLimiter>,
}

impl Drop for MintSlot {
    fn drop(&mut self) {
        // Release the reserved slot. `fetch_sub` cannot underflow: a slot
        // is only ever released by the unique owner that reserved it.
        self.limiter.inflight.fetch_sub(1, Ordering::AcqRel);
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Mint RATE limit (cluster G)
// ──────────────────────────────────────────────────────────────────────────────

/// Default mints allowed per [`MINT_RATE_WINDOW`] before a 429 is returned
/// (cluster G).
///
/// The concurrency backstop ([`MintInflightLimiter`]) bounds *simultaneous*
/// Argon2id work, but NOT the sustained RATE: an internal-auth holder firing
/// serial mints (each completing before the next starts) stays under any
/// concurrency cap while still driving the container's Argon2id CPU/RAM at
/// 100% indefinitely. This fixed-window rate limit caps the *throughput* of
/// mints, mirroring the Worker-side `session_exchange.ts` `checkMintThrottle`
/// fixed-window pattern but enforced container-side.
///
/// 60 mints/minute is generous for the only legitimate caller (the
/// signup-webhook + session/token-exchange paths mint a handful per
/// real user event) while bounding sustained Argon2id cost to ~1 hash/s.
pub const DEFAULT_MAX_MINTS_PER_WINDOW: u32 = 60;

/// Fixed window length for the mint rate limit (cluster G). One minute,
/// matching the Worker-side `MINT_THROTTLE_WINDOW_MS`.
pub const MINT_RATE_WINDOW: Duration = Duration::from_secs(60);

/// Env var overriding [`DEFAULT_MAX_MINTS_PER_WINDOW`].
pub const MAX_MINTS_PER_WINDOW_ENV: &str = "PAT_MINT_MAX_PER_MINUTE";

/// Process-global fixed-window mint RATE limiter (cluster G).
///
/// Semantics: a single GLOBAL counter (NOT per-caller) over a rolling fixed
/// window. The mint surface has exactly one trusted class of caller (holders
/// of the internal-auth secret), so a global throughput ceiling is the right
/// shape — it bounds the container's total sustained Argon2id work regardless
/// of how the load is distributed across callers, and cannot be evaded by
/// rotating a per-caller key. The counter + window are kept in an in-memory
/// [`Mutex`]; this is correct + sufficient for the single-container
/// deployment (the limiter resets on process restart, exactly like the
/// Worker-side in-memory backstop). When the window's count reaches the cap,
/// further mints are shed with `429` until the window rolls.
#[derive(Debug)]
pub struct MintRateLimiter {
    /// Start of the current window, in ms since the Unix epoch.
    window_start_ms: AtomicU64,
    /// Mints admitted in the current window.
    count: Mutex<u32>,
    /// Max mints admitted per window.
    max_per_window: u32,
    /// Window length in ms.
    window_ms: u64,
    /// Clock — injectable for deterministic tests. Defaults to wall-clock.
    now_ms: fn() -> u64,
}

/// Wall-clock milliseconds since the Unix epoch (saturating).
fn wallclock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

impl MintRateLimiter {
    /// Construct a limiter with the given per-window ceiling and the default
    /// [`MINT_RATE_WINDOW`]. A `max` of 0 is clamped to 1 so a misconfig can
    /// never wedge the surface fully shut.
    #[must_use]
    pub fn new(max_per_window: u32) -> Self {
        Self::with_clock(max_per_window, MINT_RATE_WINDOW, wallclock_ms)
    }

    /// Construct with an explicit window + clock (tests inject a fixed clock).
    #[must_use]
    pub fn with_clock(max_per_window: u32, window: Duration, now_ms: fn() -> u64) -> Self {
        Self {
            window_start_ms: AtomicU64::new(now_ms()),
            count: Mutex::new(0),
            max_per_window: max_per_window.max(1),
            window_ms: u64::try_from(window.as_millis()).unwrap_or(u64::MAX),
            now_ms,
        }
    }

    /// Build the limiter from [`MAX_MINTS_PER_WINDOW_ENV`], falling back to
    /// [`DEFAULT_MAX_MINTS_PER_WINDOW`] when unset/empty/non-numeric.
    #[must_use]
    pub fn from_env() -> Self {
        let max = std::env::var(MAX_MINTS_PER_WINDOW_ENV)
            .ok()
            .and_then(|raw| raw.trim().parse::<u32>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_MAX_MINTS_PER_WINDOW);
        Self::new(max)
    }

    /// Atomically roll the window if it has elapsed, then admit-or-reject one
    /// mint. Returns `true` when admitted (caller may proceed), `false` when
    /// the current window is already at the cap (caller must shed with 429).
    ///
    /// The whole roll+increment is done under the count mutex so concurrent
    /// callers cannot race past the cap (the lock serialises the read-modify-
    /// write, exactly as the Worker-side single-statement UPSERT does).
    #[must_use]
    pub fn try_admit(&self) -> bool {
        let now = (self.now_ms)();
        // Hold the count lock for the whole decision (roll + check + bump).
        let Ok(mut count) = self.count.lock() else {
            // Poisoned mutex (a prior panic while holding it). Fail-CLOSED:
            // refuse the mint rather than admit under an unknown counter.
            return false;
        };
        let start = self.window_start_ms.load(Ordering::Acquire);
        if now.saturating_sub(start) >= self.window_ms {
            // Window elapsed → roll: reset start + count.
            self.window_start_ms.store(now, Ordering::Release);
            *count = 0;
        }
        if *count >= self.max_per_window {
            return false;
        }
        *count += 1;
        true
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// State
// ──────────────────────────────────────────────────────────────────────────────

/// Route state injected at boot time.
#[derive(Clone)]
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
    if s.len() % 2 != 0 {
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
