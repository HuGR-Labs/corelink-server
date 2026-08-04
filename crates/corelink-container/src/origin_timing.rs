//! Container-side decomposition of the Worker's `origin` Server-Timing phase.
//!
//! # What this measures and why it exists
//!
//! The Worker publishes a `Server-Timing` split of the authed hot path
//! (`worker/src/index.ts`): `auth` (PAT verify at the edge), `wdb` (the
//! Worker-side D1 reads, itself split into `qtier` / `qbatch` / `qresid`), and
//! `origin` — the DO/container subrequest. Live prod on 2026-08-04 (warm,
//! memo-hit steady state, n=30, one reused connection, authenticated `/cargo`
//! 404 miss) read `auth` 0/0/7 ms, `wdb` 152/156/161 ms and `origin`
//! 281/300/329 ms of a 443/457/547 ms `total`: **`origin` is two thirds of the
//! request and was one opaque block.** Everything inside it — the DO dispatch
//! and placement, the DO→container wire, the container's own per-request D1
//! `pat` row read, the `$`-ceiling quota accrue, the storage lookup — summed
//! into that one number, so no measurement could say which of them to attack.
//!
//! This module is the container half of the split. It records the phases the
//! container can see and reports them **on the subresponse's own
//! `Server-Timing` header**, which the Worker parses and merges under the
//! `origin` group (see `originSubPhases` in `worker/src/index.ts`).
//!
//! # The phases
//!
//! | name     | covers                                                             |
//! |----------|--------------------------------------------------------------------|
//! | `opat`   | the container's per-request D1 `pat` row read (kept by #1022 for immediate revocation) |
//! | `oquota` | the per-tenant monthly `$`-ceiling check/accrue (ADR-0068) — a D1 round trip |
//! | `ostore` | the moat storage lookup: the `(namespace,key)→content_hash` map read plus the CAS/R2 blob fetch |
//! | `oother` | **residue** — every other millisecond the container spent: routing, the rate-limit layer, HMAC, Argon2id (or its memo hit), body handling, response assembly |
//!
//! `oother` is computed by subtraction from the layer's own whole-request clock
//! and is emitted ALWAYS, so the four **sum exactly** to the time the container
//! held the request. The Worker then computes `ohop = origin − Σ(these)` — the
//! DO hop — which is why an unattributed millisecond can never vanish: it lands
//! in a named phase on one side of the boundary or the other.
//!
//! ## Coverage caveats — stated, not faked
//!
//! * `ostore` instruments [`crate::adapter_cache::MoatCache`], which backs the
//!   cargo/brew/npm/pip cache surfaces. The **native** CAS/AC plane does not go
//!   through the moat, so on those routes storage time lands in `oother`. The
//!   phase is named for what it measures, not for what one might wish it
//!   measured.
//! * The layer stops timing when the handler returns its `Response`. For a
//!   buffered body (every route on the measured `/cargo` path) that is the whole
//!   cost; for a streamed body the streaming itself is outside `oother` — and it
//!   is outside the Worker's `origin` too, since `stub.fetch()` also resolves on
//!   headers. The two clocks stay comparable.
//! * The DO's own prologue (tenant-id lifecycle bind, `ensureContainerRunning`,
//!   the `getAlarm()` re-arm read) is on the Worker's side of this boundary and
//!   therefore lands in `ohop` together with the dispatch RPC and the wire.
//!   Separating those two needs the DO to rewrite the subresponse's headers,
//!   which is a hot-path behaviour change and deliberately NOT taken here.
//!
//! # Clock discipline
//!
//! The container measures with [`Instant`] — a real monotonic clock, unlike the
//! Worker's `Date.now()`, which advances only across I/O. **The two sides of the
//! `origin` split therefore do not have the same resolution**: a Worker phase
//! reading `dur=0` means "no I/O", whereas a container phase reading `dur=0`
//! means "under a millisecond of actual wall time". Sub-phase micros are
//! accumulated and truncated to whole milliseconds exactly once, at emission, so
//! `Σ(parts) ≤ total` holds by the monotonicity of `floor` and `oother` can
//! never be negative.
//!
//! # Overhead
//!
//! Per request: one `Arc` allocation, one task-local scope, two `Instant::now()`
//! calls in the layer plus two per instrumented phase, three relaxed atomic
//! adds, and one header format (~50 bytes). Tens of nanoseconds each — call it a
//! low single-digit microsecond against a phase measured in hundreds of
//! milliseconds, i.e. under 0.001 %. Nothing here does I/O, allocates per phase,
//! or takes a lock.
//!
//! # Recording is best-effort by construction
//!
//! [`timed`] records into a task-local ledger and is a plain pass-through when
//! no ledger is in scope (unit tests, background tasks, any non-HTTP caller).
//! Instrumentation must never be able to change a result, so there is no error
//! path here at all.

use std::future::Future;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::extract::Request;
use axum::http::header::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

/// The response header the container reports its `origin` sub-phases on.
///
/// `http` ships no constant for it (it is not one of the classic headers), so
/// it is named once here rather than spelled as a literal at each use site.
const SERVER_TIMING: HeaderName = HeaderName::from_static("server-timing");

/// A container-internal phase of the Worker's `origin` Server-Timing block.
///
/// The variants are the phases that would change a decision about WHERE to
/// attack `origin`; everything else is deliberately pooled into the `oother`
/// residue rather than split into names nobody would act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The per-request D1 `pat` row read (`opat`).
    Pat,
    /// The per-tenant `$`-ceiling quota check/accrue (`oquota`).
    Quota,
    /// The moat storage lookup — url-map read + CAS/R2 blob (`ostore`).
    Store,
}

/// Sentinel for "this phase did not run at all", mirroring the Worker's `-1`
/// convention for the `wdb` sub-phases.
///
/// It is what makes `dur=0` mean "ran, and cost less than a millisecond"
/// instead of being indistinguishable from a phase that never executed — the
/// exact ambiguity that forced an inference from the `auth` phase's 3-of-30
/// emission rate in probe run 30916725902.
const DID_NOT_RUN: i64 = -1;

/// Per-request accumulator for the instrumented phases, in microseconds.
///
/// A phase may be entered more than once in a request (a PUT verifies the PAT
/// in the write gate and the adapter may verify again); occurrences **add**,
/// because the question the phase answers is "how many milliseconds did this
/// request spend in D1 `pat` reads", not "how long did the last one take". The
/// instrumented regions are sequential and non-overlapping, so the sum stays a
/// partition of the request rather than double-counting it.
#[derive(Debug)]
pub struct PhaseLedger {
    pat_us: AtomicI64,
    quota_us: AtomicI64,
    store_us: AtomicI64,
}

impl Default for PhaseLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl PhaseLedger {
    /// A ledger with every phase marked as not-yet-run.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pat_us: AtomicI64::new(DID_NOT_RUN),
            quota_us: AtomicI64::new(DID_NOT_RUN),
            store_us: AtomicI64::new(DID_NOT_RUN),
        }
    }

    fn slot(&self, phase: Phase) -> &AtomicI64 {
        match phase {
            Phase::Pat => &self.pat_us,
            Phase::Quota => &self.quota_us,
            Phase::Store => &self.store_us,
        }
    }

    /// Add `micros` to `phase`, promoting it out of the not-run sentinel.
    fn add(&self, phase: Phase, micros: i64) {
        let _ = self
            .slot(phase)
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
                Some(if cur == DID_NOT_RUN {
                    micros
                } else {
                    cur.saturating_add(micros)
                })
            });
    }

    /// Read a phase's accumulated microseconds, or `None` if it never ran.
    #[must_use]
    pub fn micros(&self, phase: Phase) -> Option<i64> {
        match self.slot(phase).load(Ordering::Relaxed) {
            DID_NOT_RUN => None,
            v => Some(v),
        }
    }

    /// Render the `Server-Timing` value for a request the container held for
    /// `total_us` microseconds.
    ///
    /// A phase that ran is emitted even at `dur=0`; a phase that did not run is
    /// omitted entirely. `oother` is the residue and is ALWAYS emitted, so the
    /// emitted phases sum EXACTLY to `floor(total_us / 1000)` — the property the
    /// Worker relies on to derive `ohop` without losing a millisecond.
    #[must_use]
    pub fn server_timing_value(&self, total_us: i64) -> String {
        let total_ms = total_us.max(0) / 1_000;
        let mut parts: Vec<String> = Vec::with_capacity(4);
        let mut attributed_ms: i64 = 0;
        for (name, phase) in [
            ("opat", Phase::Pat),
            ("oquota", Phase::Quota),
            ("ostore", Phase::Store),
        ] {
            if let Some(us) = self.micros(phase) {
                let ms = us.max(0) / 1_000;
                attributed_ms = attributed_ms.saturating_add(ms);
                parts.push(format!("{name};dur={ms}"));
            }
        }
        // `floor(a) + floor(b) <= floor(a + b) <= floor(total)`, so this cannot
        // go negative — the `max(0)` is a belt-and-braces guard against a future
        // phase that overlaps another rather than partitioning the request.
        let other_ms = (total_ms - attributed_ms).max(0);
        parts.push(format!("oother;dur={other_ms}"));
        parts.join(", ")
    }
}

tokio::task_local! {
    /// The in-scope ledger for the request currently being served.
    ///
    /// Task-local rather than a request extension because the instrumented code
    /// (the PAT verifier, the quota guard, the moat) is reached through trait
    /// ports that have no access to the `Request` — threading a parameter
    /// through them would change production signatures for the sake of a clock.
    /// Every instrumented region is polled on the request's own task (the PAT
    /// single-flight uses a `Shared` future, not a spawn), so the ledger is
    /// visible where it is needed; a region that ever moves onto a spawned task
    /// simply records nothing and its cost falls into `oother`.
    static LEDGER: Arc<PhaseLedger>;
}

/// Time `fut` and attribute its wall duration to `phase`.
///
/// Pass-through (zero recording, no allocation) when no ledger is in scope.
pub async fn timed<F: Future>(phase: Phase, fut: F) -> F::Output {
    let start = Instant::now();
    let out = fut.await;
    let micros = i64::try_from(start.elapsed().as_micros()).unwrap_or(i64::MAX);
    LEDGER.try_with(|l| l.add(phase, micros)).ok();
    out
}

/// Axum middleware that scopes a [`PhaseLedger`] over the request and stamps
/// the resulting `origin` sub-phases onto the response's `Server-Timing`.
///
/// Wired as the OUTERMOST data-plane layer (added last in
/// [`crate::routes::build_with_factory`]) so its own clock spans everything the
/// container does — including the rate-limit layer and the OTel export layer —
/// which is what makes `oother` a true residue rather than a partial one.
///
/// The header is `insert`ed, replacing (never appending to) any value a handler
/// set, so the container speaks with exactly one voice about its own timing.
pub async fn origin_timing_layer(req: Request, next: Next) -> Response {
    let ledger = Arc::new(PhaseLedger::new());
    let start = Instant::now();
    let mut response = LEDGER.scope(Arc::clone(&ledger), next.run(req)).await;
    let total_us = i64::try_from(start.elapsed().as_micros()).unwrap_or(i64::MAX);
    if let Ok(value) = HeaderValue::from_str(&ledger.server_timing_value(total_us)) {
        response.headers_mut().insert(SERVER_TIMING, value);
    }
    response
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use std::collections::HashMap;

    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use super::{origin_timing_layer, timed, Phase, PhaseLedger};

    /// Parse a `Server-Timing` value into `{ name: dur_ms }`.
    fn parse(value: &str) -> HashMap<String, i64> {
        let mut out = HashMap::new();
        for part in value.split(',') {
            let mut it = part.trim().split(";dur=");
            if let (Some(name), Some(dur)) = (it.next(), it.next()) {
                if let Ok(v) = dur.trim().parse::<i64>() {
                    out.insert(name.trim().to_owned(), v);
                }
            }
        }
        out
    }

    #[test]
    fn a_phase_that_never_ran_is_omitted_and_one_that_ran_is_emitted_at_zero() {
        let ledger = PhaseLedger::new();
        ledger.add(Phase::Pat, 0);
        let parsed = parse(&ledger.server_timing_value(5_000));
        assert_eq!(
            parsed.get("opat"),
            Some(&0),
            "a phase that RAN must be reported even at dur=0, or `fast` and \
             `skipped` become the same observation on the wire"
        );
        assert!(!parsed.contains_key("oquota"));
        assert!(!parsed.contains_key("ostore"));
    }

    #[test]
    fn emitted_phases_sum_exactly_to_the_container_total() {
        let ledger = PhaseLedger::new();
        ledger.add(Phase::Pat, 3_400);
        ledger.add(Phase::Quota, 118_900);
        ledger.add(Phase::Store, 900);
        let parsed = parse(&ledger.server_timing_value(140_250));
        let sum: i64 = parsed.values().sum();
        assert_eq!(
            sum, 140,
            "the container sub-phases must account for the whole 140ms the \
             container held the request; they summed to {sum}. Split: {parsed:?}"
        );
        assert_eq!(parsed["opat"], 3);
        assert_eq!(parsed["oquota"], 118);
        assert_eq!(parsed["ostore"], 0);
        // 140 - (3 + 118 + 0): the truncation residue lands in `oother`, which
        // is exactly where an unattributed millisecond is supposed to go.
        assert_eq!(parsed["oother"], 19);
    }

    #[test]
    fn repeated_entries_into_one_phase_add_rather_than_overwrite() {
        let ledger = PhaseLedger::new();
        ledger.add(Phase::Pat, 1_500);
        ledger.add(Phase::Pat, 2_500);
        assert_eq!(ledger.micros(Phase::Pat), Some(4_000));
    }

    #[test]
    fn residue_is_clamped_rather_than_reported_negative() {
        let ledger = PhaseLedger::new();
        ledger.add(Phase::Quota, 50_000);
        let parsed = parse(&ledger.server_timing_value(10_000));
        assert_eq!(parsed["oother"], 0);
    }

    #[tokio::test]
    async fn timed_is_a_pass_through_when_no_ledger_is_in_scope() {
        // Every unit test and background task in the crate calls instrumented
        // code with no layer above it. Recording must never be load-bearing.
        let v = timed(Phase::Pat, async { 42_u8 }).await;
        assert_eq!(v, 42);
    }

    #[tokio::test]
    async fn the_layer_attributes_a_delay_to_the_phase_it_was_charged_to() {
        let app = Router::new()
            .route(
                "/x",
                get(|| async {
                    timed(
                        Phase::Quota,
                        tokio::time::sleep(std::time::Duration::from_millis(60)),
                    )
                    .await;
                    StatusCode::OK
                }),
            )
            .layer(axum::middleware::from_fn(origin_timing_layer));

        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/x")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let header = resp
            .headers()
            .get(super::SERVER_TIMING)
            .expect("the layer must stamp Server-Timing on every response")
            .to_str()
            .unwrap()
            .to_owned();
        let parsed = parse(&header);
        assert!(
            parsed["oquota"] >= 50,
            "oquota did not absorb the 60ms charged to it — this clock is wired \
             to the wrong await. Header: {header}"
        );
        assert!(!parsed.contains_key("opat"));
        assert!(!parsed.contains_key("ostore"));
        // And it still reconciles: nothing escapes into an unnamed gap.
        assert!(parsed["oother"] >= 0);
    }
}
