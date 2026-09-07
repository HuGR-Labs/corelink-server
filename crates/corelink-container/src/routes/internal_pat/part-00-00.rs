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
