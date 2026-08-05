//! The request-scoped **co-read cell**: how the container's per-request D1
//! `pat` row read carries the url-map row the storage lookup is about to need,
//! so the two cost ONE round trip instead of two.
//!
//! # The measurement this exists to fix
//!
//! Live prod (`1ee76248-r1`, n=65 across two independent batches, authenticated
//! `/cargo` 404 miss) decomposed the `origin` Server-Timing block as
//! `ohop` 114/116/119, **`opat` 62/72/81**, **`ostore` 66/75/84**, `oquota`
//! 0/0/0, `oother` 0/1/1 ms of a 247/267/312 ms `origin` — the sub-phases
//! summing EXACTLY on 65/65. `opat + ostore` = 147 ms = **55 % of `origin`**,
//! and both are container→D1-primary round trips of ~72–75 ms: **one RTT
//! apiece** (SAM→ENAM; see the `worker-pat-verify-cache-and-d1-locality` note).
//! On a MISS `ostore` is the url-map `SELECT` **alone** —
//! [`crate::adapter_cache::MoatCache::get_untimed`] returns early when the map
//! misses, so no CAS/R2 fetch happens — so the 75 ms is a bare round trip, not
//! storage work.
//!
//! Same shape, same fix as PR #1047 on the Worker side (`qmeter` + `qstor` →
//! one `db.batch`, `wdb` 284 → 156 ms): the win is not in the query, it is in
//! the **number of times we cross the ocean**.
//!
//! # Why a task-local cell and not a parameter
//!
//! The two reads live on opposite sides of two trait ports that deliberately do
//! not know about each other — [`crate::adapter_pat::PatRowLookup`] (auth) and
//! [`crate::adapter_cache::UrlMapStore`] (storage) — and the code between them
//! is the `corelink_adapter_host::cargo` adapter, which has neither. Threading
//! a co-read parameter through would put a storage key in the signature of the
//! auth port. Instead the route publishes a *hint* into a task-local cell, the
//! D1 `pat` read picks it up and satisfies it in the same statement, and the
//! moat consumes the answer. Exactly the pattern [`crate::origin_timing`]
//! already uses for the phase ledger, for the same reason.
//!
//! # ⚠️ The safety rule: hint ≠ authority
//!
//! The hint's namespace comes from the Worker-set `x-corelink-tenant-id`
//! header. That header is server-trusted (the Worker strips any client copy and
//! sets its own D1-resolved value) but it is **NOT** what the storage lookup is
//! keyed by: the moat is keyed by the tenant the container itself derived from
//! the PAT (Option B — the container never trusts the Worker's tenant for
//! storage). So the cell is written under the hint and can only ever be read
//! back under a key that matches it EXACTLY ([`take`]):
//!
//! * the PAT is verified first, exactly as before — the co-read changes WHEN
//!   the url-map bytes arrive, never WHEN the authorization decision is made;
//! * a prefetched row whose namespace is not the PAT-derived tenant is
//!   **discarded unread** and the moat issues its own correctly-keyed read.
//!
//! A wrong hint therefore costs a wasted index probe, never a wrong answer:
//! there is no path on which a url-map row fetched under a hinted namespace is
//! *acted on* for a different tenant.
//!
//! # And it is not a cache
//!
//! The cell lives for one request and is consumed once ([`take`] is one-shot).
//! Nothing is retained across requests, no authorization decision is stored,
//! and the `pat` row is still read from D1 on every single request — which is
//! the whole point of #1022 and what keeps revocation immediate.

use std::sync::{Arc, Mutex};

/// The per-request co-read cell.
///
/// `hint` is set at scope creation by the route; `map` is filled in by the D1
/// `pat` read when it co-reads, and drained by the moat.
#[derive(Debug)]
pub struct CoReadCell {
    /// `(namespace, url_hash)` the co-read should fetch — the route's hint.
    hint: (String, String),
    /// The co-read result once published: `Some(None)` = "queried, no mapping
    /// row" (the 404 miss), `Some(Some(hash))` = the mapped content hash.
    /// `None` = nothing published (no co-read ran, or it failed).
    map: Mutex<Option<Option<String>>>,
}

impl CoReadCell {
    /// A cell primed with the `(namespace, url_hash)` the request will look up.
    #[must_use]
    pub fn new(namespace: impl Into<String>, url_hash: impl Into<String>) -> Self {
        Self {
            hint: (namespace.into(), url_hash.into()),
            map: Mutex::new(None),
        }
    }
}

tokio::task_local! {
    /// The in-scope co-read cell for the request currently being served.
    static CELL: Arc<CoReadCell>;
}

/// Run `fut` with a co-read cell in scope for `(namespace, url_hash)`.
///
/// Everything downstream — the gate, the adapter, the PAT verifier, the moat —
/// sees the same cell.
pub async fn scope<F: std::future::Future>(
    namespace: impl Into<String>,
    url_hash: impl Into<String>,
    fut: F,
) -> F::Output {
    CELL.scope(Arc::new(CoReadCell::new(namespace, url_hash)), fut)
        .await
}

/// The `(namespace, url_hash)` the in-scope request wants co-read, if any.
///
/// Returns `None` when no cell is in scope (every route except the cargo read
/// verbs, every unit test, every background task) — which is what makes the
/// co-read strictly opt-in and the D1 `pat` read's default behaviour unchanged.
#[must_use]
pub fn hint() -> Option<(String, String)> {
    CELL.try_with(|c| c.hint.clone()).ok()
}

/// Publish a co-read url-map answer into the in-scope cell.
///
/// `value` is the `content_hash` the co-read found, or `None` for "no mapping
/// row" — a published `None` is a real, load-bearing answer (the cache miss),
/// NOT "nothing was read". A no-op when no cell is in scope.
///
/// Only ever called by the D1 `pat` read that actually ran the co-read
/// statement, and only for the cell's own [`hint`], so the published key is the
/// cell's key by construction.
pub fn publish(value: Option<String>) {
    CELL.try_with(|c| {
        if let Ok(mut slot) = c.map.lock() {
            *slot = Some(value);
        }
    })
    .ok();
}

/// Consume the co-read answer for `(namespace, url_hash)`.
///
/// * `None` — no usable prefetch: no cell in scope, nothing published, the
///   caller's key does not match the hint, or it was already consumed. The
///   caller MUST fall back to its own read.
/// * `Some(v)` — the prefetched answer (`v = None` ⇒ no mapping row).
///
/// **One-shot**, and gated on an EXACT key match. Both properties are
/// load-bearing:
///
/// * the key match is the auth-before-act guard — the caller's `namespace` is
///   the PAT-derived tenant, so a hint that named a different tenant can never
///   be served (see the module header);
/// * one-shot keeps the round-trip count honest — a second url-map read in the
///   same request is a second real read, exactly as it is today, rather than a
///   silent in-request cache.
#[must_use]
pub fn take(namespace: &str, url_hash: &str) -> Option<Option<String>> {
    CELL.try_with(|c| {
        if c.hint.0 != namespace || c.hint.1 != url_hash {
            return None;
        }
        c.map.lock().ok().and_then(|mut slot| slot.take())
    })
    .ok()
    .flatten()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: panics surface as failures by design"
)]
mod tests {
    use super::{hint, publish, scope, take};

    #[tokio::test]
    async fn outside_a_scope_everything_is_inert() {
        // Every non-cargo route and every unit test runs here. Nothing may
        // observe a hint, publishing must not panic, and `take` must report
        // "no prefetch" so the caller does its own read.
        assert_eq!(hint(), None);
        publish(Some("would-be-a-hash".to_owned()));
        assert_eq!(take("t", "k"), None);
    }

    #[tokio::test]
    async fn a_published_answer_is_returned_once_for_the_matching_key() {
        scope("tenant-a", "key-1", async {
            assert_eq!(hint(), Some(("tenant-a".to_owned(), "key-1".to_owned())));
            publish(Some("content-hash-1".to_owned()));
            assert_eq!(
                take("tenant-a", "key-1"),
                Some(Some("content-hash-1".to_owned()))
            );
            // One-shot: a SECOND url-map read in the same request is a real
            // read, not a silent in-request cache.
            assert_eq!(take("tenant-a", "key-1"), None);
        })
        .await;
    }

    #[tokio::test]
    async fn a_published_miss_is_an_answer_not_an_absence() {
        // The 404 path is the whole point: "queried, no mapping row" must be
        // servable from the co-read, or the miss still pays a round trip.
        scope("tenant-a", "key-1", async {
            publish(None);
            assert_eq!(take("tenant-a", "key-1"), Some(None));
        })
        .await;
    }

    #[tokio::test]
    async fn a_row_fetched_under_a_different_namespace_is_never_served() {
        // ⚠️ The auth-before-act guard. The hint's namespace is the Worker's
        // `x-corelink-tenant-id`; the caller's namespace is the tenant the
        // container derived from the PAT itself. If they disagree, the
        // prefetched row MUST be discarded unread and the caller must issue its
        // own correctly-keyed read — never serve tenant A's row to tenant B.
        scope("tenant-a", "key-1", async {
            publish(Some("content-hash-a".to_owned()));
            assert_eq!(take("tenant-b", "key-1"), None);
            // …and the key must match too (a stale/renamed key is not an answer).
            assert_eq!(take("tenant-a", "key-2"), None);
            // The real key still works — the guard rejects, it does not poison.
            assert_eq!(
                take("tenant-a", "key-1"),
                Some(Some("content-hash-a".to_owned()))
            );
        })
        .await;
    }

    #[tokio::test]
    async fn an_unpublished_cell_reports_no_prefetch() {
        // The co-read failed (or never ran): the caller must fall back to its
        // own read rather than treat "nothing published" as a cache miss.
        scope("tenant-a", "key-1", async {
            assert_eq!(take("tenant-a", "key-1"), None);
        })
        .await;
    }
}
