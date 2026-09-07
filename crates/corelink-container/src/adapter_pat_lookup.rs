//! PAT row decoding, D1 lookup, and freshness-preserving single-flight.
//!
//! B126-H2 symbol map: PatRow, PatRowLookup, D1HttpClient lookup impls, and
//! SingleFlightPatLookup live here; adapter_pat.rs reexports the public API.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::future::{BoxFuture, FutureExt, Shared};

use crate::storage::d1_http::{D1HttpClient, D1Row};

/// Failure surface of [`PatVerifier::verify`].
///
/// Adapter route shells map this onto their adapter's local
/// `TenantResolveError` (`InvalidPat` ⇒ 401, `Backend` ⇒ 503).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VerifyError {
    /// PAT not found, expired, forged, wrong secret, or lacking a cache
    /// scope. Uniform by design (no oracle). Surfaces as HTTP 401.
    ///
    /// NEVER returned from a load shed — see the module header on
    /// `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`.
    #[error("invalid PAT")]
    InvalidPat,
    /// Verifier backend (D1, corrupt row) failed, **or** the Argon2id
    /// permit pool shed this request under saturation (message `pat
    /// verifier overloaded`, identical on the row-FOUND and row-NOT-FOUND
    /// arms — `INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`). Surfaces as HTTP 503;
    /// the adapter surfaces attach `Retry-After: 1` because a shed is
    /// transient and converges as soon as the first in-flight verify
    /// populates the memo.
    #[error("verifier backend: {0}")]
    Backend(String),
}

/// A single `pat` row, reduced to the fields PAT verification needs.
///
/// `token_id` is the non-secret lookup key; the secret material is the
/// caller-supplied plaintext, verified against `pat_hash`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PatRow {
    /// Tenant UUID (text) that owns the PAT.
    pub tenant_id: String,
    /// Argon2id PHC hash string from `pat.pat_hash`.
    pub pat_hash: String,
    /// The PAT's D1 `scope` string (e.g. `cas:rw`, `admin`, or `""` for
    /// legacy rows). Empty / absent ⇒ fail-CLOSED at the scope gate.
    pub scope: String,
    /// D1 `pat.find_only` (migration 0093, ADR-0071): `true` = a FIND-ONLY
    /// least-privilege PAT whose base `scope` is the CHECK-safe `read-only`, but
    /// which grants ONLY cache find-missing. The adapter planes (npm/pip/brew/
    /// cargo/OCI) have NO find-missing operation, and — unlike the native cache
    /// planes — the OCI adapter authorizes from THIS D1 `scope` directly (not the
    /// Worker's `x-corelink-scope` header), so a find-only PAT MUST be rejected
    /// here (else its `read-only` base would grant e.g. `docker pull`). Legacy /
    /// non-find rows are `false`.
    pub find_only: bool,
    /// D1 `pat.runner_job_ac_key IS NOT NULL` (migration 0086): `true` = a
    /// NARROWED runner-job PAT, minted per CI job by the runner fabric.
    ///
    /// The migration's stated contract is that such a credential "must not be
    /// able to EVICT the tenant's cache", and the native plane enforces exactly
    /// that (`routes/cas.rs` denies CAS DELETE; `routes/ac.rs` pins AC writes to
    /// the job's key). The adapter plane could not enforce it at all until this
    /// field existed, because it authorizes from the D1 row directly and never
    /// sees the Worker's `x-corelink-runner-job` header — the SAME structural
    /// blindness ADR-0071 closed for `find_only`.
    ///
    /// Unlike `find_only` this is NOT a reason to reject the PAT: a runner job
    /// legitimately reads and writes the cache, and sccache-over-CoreLink is the
    /// runner fabric's own dogfood path. It narrows ONE operation
    /// (`routes/cargo.rs`'s WebDAV DELETE of a build artifact) and nothing else.
    /// The AC-key value itself is deliberately not carried: no adapter surface
    /// has an Action Cache to pin it against.
    pub runner_job: bool,
}

/// Fetch a `pat` row by its non-secret `token_id`, already expiry-filtered.
///
/// Abstracted as a trait so the security-critical verification pipeline in
/// [`PatVerifier`] can be unit-tested hermetically with real crypto and a
/// fake row source — the production impl talks to D1 over HTTP, which a
/// unit test cannot reach.
#[async_trait]
pub trait PatRowLookup: Send + Sync {
    /// Return the row for `token_id`, or `None` when no live (non-expired,
    /// non-revoked) row exists. `Err` is reserved for backend faults.
    async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String>;
}

/// The D1 `pat` lookup SQL for the container verifier.
///
/// Mirrors the Worker hot-path (migration `0054_pat_token_id`): an `O(1)`
/// covering-index lookup on `token_id`, with the same SQL-side expiry
/// filter (`expires_ms = 0` ⇒ no-expiry token) and the same soft-revocation
/// filter (migration `0063_pat_customer_keys`: `revoked_at_ms IS NULL` ⇒
/// active; a revoked row is indistinguishable from an absent one).
pub(super) const PAT_LOOKUP_SQL: &str =
    "SELECT tenant_id, pat_hash, scope, find_only, runner_job_ac_key FROM pat \
     WHERE token_id = ?1 \
       AND (expires_ms = 0 OR expires_ms > unixepoch('now', 'subsec') * 1000) \
       AND revoked_at_ms IS NULL \
     LIMIT 1";

/// The **co-read**: [`PAT_LOOKUP_SQL`] and the url-map lookup the storage layer
/// is about to need, in ONE statement ⇒ ONE D1 round trip.
///
/// # Why
///
/// Prod (`1ee76248-r1`, n=65) measured `opat` 72 ms and `ostore` 75 ms as the
/// median halves of a 267 ms `origin` — two container→D1-primary round trips of
/// one RTT each, issued back to back. Merging them is the same move PR #1047
/// made on the Worker side (`qmeter` + `qstor` → one `db.batch`, `wdb` 284 →
/// 156 ms): the cost is the ocean crossing, not the query.
///
/// # Shape
///
/// A compound `UNION ALL` of two independently-`LIMIT 1`-ed arms (SQLite
/// requires the subquery wrapper: a bare `LIMIT` in a compound arm binds to the
/// whole compound). Each arm is an `O(1)` index probe — `pat.token_id`
/// (migration `0054_pat_token_id`) and `adapter_cache_map(namespace, url_hash)`
/// — so the co-read is two point lookups in one trip, never a scan.
///
/// The arms are discriminated by the literal `kind` column, NOT by row order:
/// a compound `SELECT` has no ordering guarantee without `ORDER BY`, and a
/// mis-assigned arm here would hand a `content_hash` to the PAT verifier.
///
/// The pat arm is [`PAT_LOOKUP_SQL`] verbatim (same filters, same `LIMIT 1`,
/// same columns, only aliased), so a co-read decides expiry and soft-revocation
/// identically to the serial read — `INV-PAT-REVOKE-PROPAGATION` is untouched.
pub(super) const PAT_URL_MAP_COREAD_SQL: &str = "SELECT * FROM (\
       SELECT 'p' AS kind, tenant_id AS c1, pat_hash AS c2, scope AS c3, find_only AS c4, \
              runner_job_ac_key AS c5 \
       FROM pat \
       WHERE token_id = ?1 \
         AND (expires_ms = 0 OR expires_ms > unixepoch('now', 'subsec') * 1000) \
         AND revoked_at_ms IS NULL \
       LIMIT 1) \
     UNION ALL \
     SELECT * FROM (\
       SELECT 'm' AS kind, content_hash AS c1, NULL AS c2, NULL AS c3, NULL AS c4, \
              NULL AS c5 \
       FROM adapter_cache_map \
       WHERE namespace = ?2 AND url_hash = ?3 \
       LIMIT 1)";

/// Build a [`PatRow`] from the four `pat` column values, whatever they were
/// named by the statement that fetched them.
///
/// Shared verbatim by the serial read and the co-read so the two can never
/// diverge on a NULL/absent column: the error strings, the legacy-`scope`
/// fail-CLOSED default and the `find_only` decoding are defined ONCE.
pub(super) fn pat_row_from_columns(
    tenant_id: Option<&serde_json::Value>,
    pat_hash: Option<&serde_json::Value>,
    scope: Option<&serde_json::Value>,
    find_only: Option<&serde_json::Value>,
    runner_job_ac_key: Option<&serde_json::Value>,
) -> Result<PatRow, String> {
    let tenant_id = tenant_id
        .and_then(|v| v.as_str())
        .ok_or("D1 pat: missing `tenant_id` column")?
        .to_owned();
    let pat_hash = pat_hash
        .and_then(|v| v.as_str())
        .ok_or("D1 pat: missing `pat_hash` column")?
        .to_owned();
    // `scope` may be NULL on legacy rows; map that to "" (fail-CLOSED
    // at the scope gate) rather than a backend error.
    let scope = scope.and_then(|v| v.as_str()).unwrap_or("").to_owned();
    // `find_only` (0093): NULL/0 = normal PAT; 1 = find-missing-only. Absent on
    // a legacy row ⇒ false (a normal PAT). The D1 HTTP API returns integers as
    // JSON numbers.
    let find_only = find_only.and_then(serde_json::Value::as_i64) == Some(1);
    // `runner_job_ac_key` (0086): NULL = a normal PAT; any non-NULL value (the
    // launch value is the literal `"*"`) = a NARROWED runner-job PAT. Only
    // PRESENCE is decoded — the key value pins an AC key, and no adapter surface
    // has an Action Cache. Absent on a legacy row / on the url-map arm of the
    // co-read ⇒ `false`, i.e. a normal PAT, which is the pre-0086 behaviour.
    let runner_job = runner_job_ac_key.is_some_and(|v| !v.is_null());
    Ok(PatRow {
        tenant_id,
        pat_hash,
        scope,
        find_only,
        runner_job,
    })
}

impl D1HttpClient {
    /// The serial `pat` read — [`PAT_LOOKUP_SQL`] alone. Unchanged behaviour;
    /// still the path for every route that publishes no co-read hint, and the
    /// fallback when a co-read fails.
    async fn pat_lookup_only(&self, token_id: &str) -> Result<Option<PatRow>, String> {
        let rows = self
            .query(
                PAT_LOOKUP_SQL,
                &[serde_json::Value::String(token_id.to_owned())],
            )
            .await?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        pat_row_from_columns(
            row.get("tenant_id"),
            row.get("pat_hash"),
            row.get("scope"),
            row.get("find_only"),
            row.get("runner_job_ac_key"),
        )
        .map(Some)
    }

    /// Run [`PAT_URL_MAP_COREAD_SQL`], publish the url-map arm into `cell`, and
    /// return the `pat` arm.
    ///
    /// The url-map answer is published — including a MISS, which is the
    /// interesting case (the measured 404 path) — but it is only ever *served*
    /// to a caller whose namespace matches the hint the co-read was keyed by,
    /// i.e. the tenant this very `pat` row is about to establish. Auth still
    /// decides first; the co-read only changes when the bytes arrived.
    ///
    /// # `cell` is a parameter, and that is load-bearing
    ///
    /// `cell` is the very [`crate::d1_coread::CoReadCell`] whose hint supplied
    /// `namespace`/`url_hash`, captured by the caller BEFORE the `.await`. It is
    /// not re-derived from the task-local here, because this future is not
    /// guaranteed to be polled by the task that started it: production wraps
    /// this lookup in [`SingleFlightPatLookup`], which `.await`s a `Shared`
    /// future it does NOT spawn — and a `Shared` future is driven only by
    /// whoever polls it (the same property [`FlightGroup::run`] spawns to escape;
    /// see its "The work is SPAWNED, and that is load-bearing" note). So a second
    /// request for the same `token_id` — same PAT, DIFFERENT object, i.e. the
    /// ordinary cargo hot path — can be the one in scope when these rows land.
    /// Publishing into the ambient cell there would file this request's
    /// `content_hash` under that request's `url_hash`, and nothing downstream
    /// would notice: the moat's integrity check re-hashes bytes against the
    /// mapped hash, tying bytes↔hash, never key↔hash. Holding the handle makes
    /// the destination a property of the fetch instead of the scheduler.
    async fn pat_lookup_coread(
        &self,
        cell: &crate::d1_coread::CoReadCell,
        token_id: &str,
        namespace: &str,
        url_hash: &str,
    ) -> Result<Option<PatRow>, String> {
        let rows = self
            .query(
                PAT_URL_MAP_COREAD_SQL,
                &[
                    serde_json::Value::String(token_id.to_owned()),
                    serde_json::Value::String(namespace.to_owned()),
                    serde_json::Value::String(url_hash.to_owned()),
                ],
            )
            .await?;

        let mut pat_row: Option<&D1Row> = None;
        let mut map_hash: Option<String> = None;
        for row in &rows {
            match row.get("kind").and_then(|v| v.as_str()) {
                Some("p") => pat_row = Some(row),
                Some("m") => {
                    map_hash = row.get("c1").and_then(|v| v.as_str()).map(str::to_owned);
                }
                // A row we cannot attribute to an arm. Refuse to guess: fail the
                // co-read so the caller falls back to the serial pair rather
                // than hand an unidentified column to the PAT verifier.
                _ => return Err("D1 co-read: row with unknown `kind`".to_owned()),
            }
        }

        // Publish the url-map arm — `None` here is a real answer ("no mapping
        // row"), not an absence, and it is exactly the 404 the probe measures.
        // Into the CAPTURED cell: see this method's doc for why the ambient one
        // may belong to a different request by now.
        cell.publish(map_hash);

        match pat_row {
            None => Ok(None),
            Some(row) => pat_row_from_columns(
                row.get("c1"),
                row.get("c2"),
                row.get("c3"),
                row.get("c4"),
                row.get("c5"),
            )
            .map(Some),
        }
    }
}

/// Production [`PatRowLookup`] over the CF D1 HTTP API.
#[async_trait]
impl PatRowLookup for D1HttpClient {
    /// Reads the `pat` row — and, when the route in scope published a co-read
    /// hint ([`crate::d1_coread::hint`]), carries that route's url-map lookup in
    /// the SAME round trip.
    ///
    /// # Failure semantics are unchanged, by construction
    ///
    /// A co-read that errors is NEVER load-bearing: it is logged and the serial
    /// [`PAT_LOOKUP_SQL`] read runs, so the returned `Result` is decided by the
    /// exact same statement as before, and (nothing having been published) the
    /// url-map read the moat makes is the exact same one too. The price is one
    /// wasted round trip on a failing request — paid only when D1 is already
    /// erroring, never on the hot path.
    async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
        // Capture the cell HANDLE together with the key, here, before any
        // `.await` — the co-read's answer must land in the cell whose hint keyed
        // it even if a coalescing joiner ends up driving the poll (see
        // [`Self::pat_lookup_coread`]).
        if let Some((cell, namespace, url_hash)) = crate::d1_coread::hint() {
            match self
                .pat_lookup_coread(&cell, token_id, &namespace, &url_hash)
                .await
            {
                Ok(row) => return Ok(row),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "D1 pat co-read failed; falling back to the serial pat read"
                    );
                }
            }
        }
        self.pat_lookup_only(token_id).await
    }
}

/// The shared, cloneable in-flight lookup future used by [`SingleFlightPatLookup`].
/// `Arc<..>` so every joiner clones one heap result; `Shared` so one poll drives
/// the single inner D1 read for all joiners.
type SharedLookup = Shared<BoxFuture<'static, Arc<Result<Option<PatRow>, String>>>>;

/// A **no-cache single-flight** wrapper over an inner [`PatRowLookup`].
///
/// The cold-hydrate PAT read-herd (2026-07-08: ~57 D1 `pat` reads during a
/// 668 MB pull) is a burst of the SAME runner PAT arriving in parallel — every
/// op re-reads the same `token_id` row. This coalesces a burst of concurrent
/// lookups for one `token_id` into ONE inner D1 read: the first caller leads the
/// read, the rest join its [`Shared`] future.
///
/// # It is NOT a cache — revocation stays immediate
///
/// The shared flight is dropped the instant it resolves, and a late joiner that
/// finds an already-RESOLVED flight (`peek().is_some()`) REFUSES to reuse it and
/// leads a fresh read instead. So there is no window where a resolved result is
/// served to a request that started after it: every returned row is FRESH, and
/// the SQL-side `revoked_at_ms IS NULL` / expiry filters run on every real read
/// — `INV-PAT-REVOKE-PROPAGATION` is preserved (a revoked token surfaces as
/// `None` on the very next non-in-flight lookup). Coalescing is keyed on the
/// **non-secret** `token_id`; the per-request Argon2id verify in
/// [`PatVerifier::verify_capability`] is UNTOUCHED (it still runs once per
/// request and is the sole possession check), so this changes no auth decision,
/// adds no timing oracle (the D1 hop is dwarfed by Argon2id), and leaves the
/// dummy-burn / OOM-permit machinery exactly as is.
#[non_exhaustive]
pub struct SingleFlightPatLookup {
    inner: Arc<dyn PatRowLookup>,
    /// `token_id -> in-flight shared read`. Sync mutex; the guard is NEVER held
    /// across an `.await` (we clone the `Shared` out, then drop the guard).
    inflight: Mutex<HashMap<String, SharedLookup>>,
}

impl std::fmt::Debug for SingleFlightPatLookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleFlightPatLookup")
            .field("inner", &"Arc<dyn PatRowLookup>")
            .finish()
    }
}

impl SingleFlightPatLookup {
    /// Wrap an inner lookup with per-`token_id` single-flight coalescing.
    #[must_use]
    pub fn new(inner: Arc<dyn PatRowLookup>) -> Self {
        Self {
            inner,
            inflight: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl PatRowLookup for SingleFlightPatLookup {
    async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
        // Join an UNRESOLVED in-flight read for this token_id, or lead a fresh
        // one. A poisoned lock falls back to a direct read (fail-safe: correct,
        // just uncoalesced).
        let (shared, is_leader) = {
            let Ok(mut map) = self.inflight.lock() else {
                return self.inner.lookup(token_id).await;
            };
            match map.get(token_id) {
                // Only JOIN a flight that has NOT resolved — never reuse a
                // completed read (freshness / revocation immediacy).
                Some(existing) if existing.peek().is_none() => (existing.clone(), false),
                _ => {
                    let inner = Arc::clone(&self.inner);
                    let tid = token_id.to_owned();
                    let fut: SharedLookup = async move { Arc::new(inner.lookup(&tid).await) }
                        .boxed()
                        .shared();
                    // `insert` REPLACES any resolved-stale entry for this key.
                    let _ = map.insert(token_id.to_owned(), fut.clone());
                    (fut, true)
                }
            }
        };

        let result = shared.await;

        // The leader evicts the now-resolved flight so the NEXT lookup is fresh.
        // Guard: remove ONLY if the current entry is resolved (a fresh leader may
        // have already replaced it with a new unresolved flight — leave that).
        if is_leader {
            if let Ok(mut map) = self.inflight.lock() {
                if let Some(cur) = map.get(token_id) {
                    if cur.peek().is_some() {
                        let _ = map.remove(token_id);
                    }
                }
            }
        }

        (*result).clone()
    }
}
