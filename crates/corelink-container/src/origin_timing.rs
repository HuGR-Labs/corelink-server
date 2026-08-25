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
//! | `opat`    | the container's per-request D1 `pat` row read (kept by #1022 for immediate revocation) — and, on the cargo read path, the url-map row it CO-READS in the same round trip |
//! | `oquota`  | the per-tenant monthly `$`-ceiling check/accrue (ADR-0068) — a D1 round trip |
//! | `ostore`  | the moat storage lookup (`(namespace,key)→content_hash` map read plus the CAS/R2 blob fetch) **and**, on the native CAS/AC plane, the R2/S3 object GET/PUT/DELETE/LIST calls `R2CasHandler`/`R2AcHandler` make through the sync `block_in_place` bridge (`storage/r2_s3.rs`) |
//! | `oargon`  | Argon2id verification (`adapter_pat.rs`): the secret-match memo check plus, on a miss, the coalesced verify flight — AND, on the SAME name, the row-not-found coalesced dummy Argon2id burn that pads timing for a missing/expired/revoked `token_id` (see the security note below) |
//! | `opermit` | the semaphore acquires bounded by `ARGON2_PERMIT_WAIT`, in both the dummy-burn arm and the real verify arm |
//! | `ortier`  | `ensure_tier_applied`'s D1 tier-label resolution (`routes/ratelimit_layer.rs` → `oci_cap.rs`) |
//! | `oaudit`  | the blocking durable-audit D1 write on the request path (`D1AuditOutboxSink::write_blocking`, `storage/d1_audit_sink.rs`) — every `AuditSink::emit`/`append` the native CAS/AC handlers make before/after a mutation or read routes through this one blocking D1-over-HTTP `INSERT` |
//! | `oother`  | **residue** — every other millisecond the container spent: routing, HMAC, body handling, response assembly |
//!
//! ## Security: `oargon` must not become a token-enumeration oracle
//!
//! `adapter_pat.rs` deliberately burns a DUMMY Argon2id hash on the
//! token-not-found arm so that a missing/expired/revoked `token_id` is
//! timing-indistinguishable from a valid one. Both the found arm (memo check +
//! verify flight) and the not-found arm (dummy burn) report under the exact
//! same `oargon` name, with the exact same emission rule (present iff that
//! arm's `timed`/[`PhaseScope`] region actually ran). A caller therefore cannot
//! tell the two arms apart by which phases are present or absent in
//! `Server-Timing` — the split adds a NEW way to read the phases, not a new
//! way to distinguish the two arms.
//!
//! ## The co-read moved a millisecond, it did not lose one
//!
//! `opat` and `ostore` were the two halves the first measurement found: 72 and
//! 75 ms of a 267 ms `origin`, one D1 round trip each. They are now ONE round
//! trip (`crate::d1_coread`), and it is charged to **`opat`** — the phase whose
//! statement leads it — so on the cargo read path `ostore` falls to ~0 on a
//! miss and to the CAS/R2 fetch alone on a hit. The names are deliberately
//! unchanged: renaming a phase would strand the ms in `ohop` on any Worker that
//! has not yet been redeployed (`originSubPhases` ignores names outside its
//! allowlist), which is exactly the silent mis-attribution this split exists to
//! prevent. The reconciliation is unaffected — the four still sum EXACTLY to
//! the time the container held the request, because `oother` is a residue and
//! the phases partition, not label, the work.
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
//!   through the moat, but its R2/S3 object calls (`storage/r2_s3.rs`) are ALSO
//!   wrapped into `ostore` via [`PhaseScope`] — the two report under the same
//!   name because both answer the same question ("how long did this request
//!   spend touching durable blob storage"), not because they share a code
//!   path. What is still NOT covered by `ostore` on the native plane is the
//!   BYOK key-resolution / encrypt / decrypt work (`resolve_byok`,
//!   `encrypt_body`, `decrypt_body`) that wraps those calls — that remains in
//!   `oother`. The phase is named for what it measures, not for what one might
//!   wish it measured.
//! * `oaudit` instruments [`crate::storage::d1_audit_sink::D1AuditOutboxSink`]'s
//!   `write_blocking` — the ONE blocking-D1 choke point every CAS/AC audit
//!   `emit`/`append` call routes through, native plane only (the moat
//!   surfaces' cache-hit audit trail is a separate, already-async path and is
//!   not in `oaudit`).
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
    /// Argon2id verification (`oargon`) — the memo check plus, on a miss, the
    /// verify flight (`adapter_pat.rs`'s `verify_capability`), AND the
    /// row-not-found coalesced dummy Argon2id burn that pads timing for a
    /// missing/expired/revoked `token_id`. Both regions report under this SAME
    /// name so the found and not-found arms cannot be told apart by which
    /// phases are present on the wire — see the module-level security note on
    /// [`timed`] and [`PhaseScope`].
    Argon,
    /// The semaphore acquires bounded by `ARGON2_PERMIT_WAIT` (`oquota`'s
    /// Argon2id-permit sibling): the global permit wait in both the dummy-burn
    /// arm and the real verify arm accumulate into this one phase (`opermit`).
    Permit,
    /// `ensure_tier_applied`'s tier resolution — the one D1 round trip in the
    /// `oother` residue that no other phase counted (`ortier`).
    Tier,
    /// The blocking durable-audit D1 write on the request path (`oaudit`):
    /// `D1AuditOutboxSink::write_blocking` (`storage/d1_audit_sink.rs`), the
    /// single choke point every CAS/AC `AuditSink::emit`/`append` call routes
    /// through before/after a native-plane read or mutation.
    Audit,
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
    argon_us: AtomicI64,
    permit_us: AtomicI64,
    tier_us: AtomicI64,
    audit_us: AtomicI64,
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
            argon_us: AtomicI64::new(DID_NOT_RUN),
            permit_us: AtomicI64::new(DID_NOT_RUN),
            tier_us: AtomicI64::new(DID_NOT_RUN),
            audit_us: AtomicI64::new(DID_NOT_RUN),
        }
    }

    fn slot(&self, phase: Phase) -> &AtomicI64 {
        match phase {
            Phase::Pat => &self.pat_us,
            Phase::Quota => &self.quota_us,
            Phase::Store => &self.store_us,
            Phase::Argon => &self.argon_us,
            Phase::Permit => &self.permit_us,
            Phase::Tier => &self.tier_us,
            Phase::Audit => &self.audit_us,
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
        self.server_timing_value_with(total_us, detail_phases_enabled())
    }

    /// [`Self::server_timing_value`] with the detail gate passed in explicitly.
    ///
    /// The public method reads the process-wide flag; this one takes it as an
    /// argument so both sides of the gate are unit-testable without mutating
    /// process environment (which is global and would race across test threads).
    #[must_use]
    pub(crate) fn server_timing_value_with(&self, total_us: i64, detail: bool) -> String {
        let total_ms = total_us.max(0) / 1_000;
        let mut parts: Vec<String> = Vec::with_capacity(8);
        let mut attributed_ms: i64 = 0;
        for (name, phase) in [
            ("opat", Phase::Pat),
            ("oquota", Phase::Quota),
            ("ostore", Phase::Store),
            ("oargon", Phase::Argon),
            ("opermit", Phase::Permit),
            ("ortier", Phase::Tier),
            ("oaudit", Phase::Audit),
        ] {
            // The four detail phases are gated (see `detail_phases_enabled`);
            // when off their time is left to fall into `oother`, exactly as
            // before this split existed.
            if !detail
                && matches!(
                    phase,
                    Phase::Argon | Phase::Permit | Phase::Tier | Phase::Audit
                )
            {
                continue;
            }
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

/// Whether the four detail phases (`oargon`, `opermit`, `ortier`, `oaudit`) are
/// published on the wire.
///
/// **Off by default, and that default is load-bearing.** `oargon`/`opermit`
/// report the state of a per-process cache on the credential path, so their
/// PRESENCE — not their duration — is observable signal:
///
///   - `opermit` is recorded inside the Argon2id flight. On the row-NOT-FOUND arm
///     the coalesced dummy burn always runs, so the phase appears; on the valid-token
///     arm with a WARM `secret_match_memo` the flight is skipped, so it does not.
///     Publishing it therefore tells a caller whether that exact secret was recently
///     verified by the process that served them.
///   - The dummy burn exists precisely so that a missing / expired / revoked
///     `token_id` is timing-indistinguishable from a valid one. A header that
///     partitions that time by name erodes the padding it is there to provide.
///
/// `ortier` and `oaudit` carry no such credential-oracle risk on their own —
/// they are gated behind the SAME flag as a matter of a single, conservative
/// opt-in switch for every phase this split has added since the original
/// three (`opat`/`oquota`/`ostore`), rather than growing a second flag per
/// addition. An operator may enable the whole group once they have read this
/// doc; there is currently no reason to ship `oaudit` alone.
///
/// When off, the four phases are simply not emitted and their time falls into
/// `oother` — the residue is unchanged in meaning and the header is byte-identical
/// to what shipped before the split. The ledger still RECORDS them unconditionally
/// (an `Instant` is free), so turning the flag on needs no rebuild of the timing
/// code, only a redeploy of this gate.
///
/// Armed with `CORELINK_ORIGIN_TIMING_DETAIL=on`, deliberately, by an operator who
/// has read the above — never as a default.
fn detail_phases_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("CORELINK_ORIGIN_TIMING_DETAIL").is_ok_and(|v| v == "on"))
}

tokio::task_local! {
    /// The in-scope ledger for the request currently being served.
    ///
    /// Task-local rather than a request extension because the instrumented code
    /// (the PAT verifier, the quota guard, the moat) is reached through trait
    /// ports that have no access to the `Request` — threading a parameter
    /// through them would change production signatures for the sake of a clock.
    /// Most instrumented regions are polled on the request's own task (the PAT
    /// row-lookup single-flight uses a `Shared` future, not a spawn), so the
    /// ambient task-local is visible where [`timed`] and [`PhaseScope::enter`]
    /// need it. A region that DOES move onto a `tokio::spawn`ed task (the
    /// Argon2id `FlightGroup` flights in `adapter_pat.rs` spawn their `lead`
    /// future so the run survives every awaiter disconnecting) cannot see this
    /// task-local at all — that region must instead capture a handle with
    /// [`current_ledger`] on the ORIGINATING task, before spawning, and record
    /// through it via [`PhaseScope::with_handle`]; only a region that captures
    /// no handle at all (no `current_ledger()` call reachable) records nothing
    /// and falls into `oother`.
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

/// Capture a handle to the CURRENT request's ledger, or `None` if no ledger is
/// in scope on this task.
///
/// [`timed`] and [`PhaseScope::enter`] both read the ambient task-local, which
/// only works when the instrumented region is polled on the SAME task the
/// scope was opened on. A region reached inside a `tokio::spawn`ed future runs
/// on a different task with no task-local visible at all — the lesson already
/// paid for elsewhere in this crate (`adapter_pat.rs`'s co-read cell): publish
/// by a CAPTURED HANDLE, never by the ambient task-local, once the executing
/// task can differ from the one that opened the scope. A caller that spawns
/// therefore calls this BEFORE spawning, on the original task, and passes the
/// result into the spawned closure for [`PhaseScope::with_handle`] to consume.
#[must_use]
pub fn current_ledger() -> Option<Arc<PhaseLedger>> {
    LEDGER.try_with(Arc::clone).ok()
}

/// RAII timer for a region that has its own internal early returns (`?` or
/// `return`) and so cannot be wrapped as a single awaited expression the way
/// [`timed`] wraps a future.
///
/// Starts the clock on construction and records elapsed wall time into
/// `phase` on [`Drop`] — which Rust runs on EVERY exit path out of the scope
/// the guard was declared in, including an early `return` from the enclosing
/// function, so timing a multi-statement region this way adds no branch and
/// reorders nothing already there.
#[derive(Debug)]
pub struct PhaseScope {
    ledger: Option<Arc<PhaseLedger>>,
    phase: Phase,
    start: Instant,
}

impl PhaseScope {
    /// Time a region reached on the SAME task that opened the ledger's scope —
    /// captures the ambient task-local, like [`timed`], but for a region that
    /// is not a single future.
    #[must_use]
    pub fn enter(phase: Phase) -> Self {
        Self {
            ledger: current_ledger(),
            phase,
            start: Instant::now(),
        }
    }

    /// Time a region reached on a DIFFERENT task than the one that opened the
    /// ledger's scope (e.g. inside a `tokio::spawn`ed future). The caller must
    /// capture `ledger` with [`current_ledger`] on the ORIGINATING task,
    /// before spawning — see that function's doc for why.
    #[must_use]
    pub fn with_handle(ledger: Option<Arc<PhaseLedger>>, phase: Phase) -> Self {
        Self {
            ledger,
            phase,
            start: Instant::now(),
        }
    }
}

impl Drop for PhaseScope {
    fn drop(&mut self) {
        if let Some(ledger) = &self.ledger {
            let micros = i64::try_from(self.start.elapsed().as_micros()).unwrap_or(i64::MAX);
            ledger.add(self.phase, micros);
        }
    }
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
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use super::{
        current_ledger, origin_timing_layer, timed, Phase, PhaseLedger, PhaseScope, LEDGER,
    };

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
        let parsed = parse(&ledger.server_timing_value_with(5_000, true));
        assert_eq!(
            parsed.get("opat"),
            Some(&0),
            "a phase that RAN must be reported even at dur=0, or `fast` and \
             `skipped` become the same observation on the wire"
        );
        assert!(!parsed.contains_key("oquota"));
        assert!(!parsed.contains_key("ostore"));
        assert!(!parsed.contains_key("oargon"));
        assert!(!parsed.contains_key("opermit"));
        assert!(!parsed.contains_key("ortier"));
        assert!(!parsed.contains_key("oaudit"));
    }

    #[test]
    fn emitted_phases_sum_exactly_to_the_container_total() {
        let ledger = PhaseLedger::new();
        ledger.add(Phase::Pat, 3_400);
        ledger.add(Phase::Quota, 118_900);
        ledger.add(Phase::Store, 900);
        let parsed = parse(&ledger.server_timing_value_with(140_250, true));
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
    fn all_seven_named_phases_plus_oother_sum_exactly_to_the_container_total() {
        // W2/W3: opat + oquota + ostore + oargon + opermit + ortier + oaudit +
        // oother must reconcile exactly against the container's own
        // whole-request clock, the same way the original three did.
        let ledger = PhaseLedger::new();
        ledger.add(Phase::Pat, 3_400);
        ledger.add(Phase::Quota, 20_100);
        ledger.add(Phase::Store, 900);
        ledger.add(Phase::Argon, 61_700);
        ledger.add(Phase::Permit, 12_300);
        ledger.add(Phase::Tier, 4_600);
        ledger.add(Phase::Audit, 8_500);
        let parsed = parse(&ledger.server_timing_value_with(140_250, true));
        let sum: i64 = parsed.values().sum();
        assert_eq!(
            sum, 140,
            "opat+oquota+ostore+oargon+opermit+ortier+oaudit+oother must equal \
             the 140ms the container held the request; they summed to {sum}. \
             Split: {parsed:?}"
        );
        assert_eq!(parsed["opat"], 3);
        assert_eq!(parsed["oquota"], 20);
        assert_eq!(parsed["ostore"], 0);
        assert_eq!(parsed["oargon"], 61);
        assert_eq!(parsed["opermit"], 12);
        assert_eq!(parsed["ortier"], 4);
        assert_eq!(parsed["oaudit"], 8);
        // 140 - (3 + 20 + 0 + 61 + 12 + 4 + 8) = 32, the truncation residue.
        assert_eq!(parsed["oother"], 32);
    }

    #[test]
    fn a_phase_that_never_ran_among_the_new_three_is_omitted_and_one_that_ran_is_emitted() {
        let ledger = PhaseLedger::new();
        ledger.add(Phase::Argon, 0);
        let parsed = parse(&ledger.server_timing_value_with(5_000, true));
        assert_eq!(
            parsed.get("oargon"),
            Some(&0),
            "oargon must be reported even at dur=0, exactly like the existing \
             three phases — see the not-found/found parity requirement"
        );
        assert!(!parsed.contains_key("opermit"));
        assert!(!parsed.contains_key("ortier"));
        assert!(!parsed.contains_key("oaudit"));
        assert!(!parsed.contains_key("opat"));
        assert!(!parsed.contains_key("oquota"));
        assert!(!parsed.contains_key("ostore"));
    }

    #[test]
    fn a_phase_that_never_ran_among_audit_is_omitted_and_one_that_ran_is_emitted() {
        let ledger = PhaseLedger::new();
        ledger.add(Phase::Audit, 0);
        let parsed = parse(&ledger.server_timing_value_with(5_000, true));
        assert_eq!(
            parsed.get("oaudit"),
            Some(&0),
            "oaudit must be reported even at dur=0, exactly like the other \
             detail phases"
        );
        assert!(!parsed.contains_key("oargon"));
        assert!(!parsed.contains_key("opermit"));
        assert!(!parsed.contains_key("ortier"));
        assert!(!parsed.contains_key("opat"));
        assert!(!parsed.contains_key("oquota"));
        assert!(!parsed.contains_key("ostore"));
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
        let parsed = parse(&ledger.server_timing_value_with(10_000, true));
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

    #[tokio::test]
    async fn phase_scope_records_on_early_return_from_inside_it() {
        // The `oargon`/`opermit` wraps in `adapter_pat.rs` cover regions with
        // their own internal early returns. `PhaseScope` must record on THAT
        // exit path too, not only when the scoped block runs to completion.
        async fn region_with_early_return() -> u8 {
            let _scope = PhaseScope::enter(Phase::Argon);
            if true {
                return 7; // exits the function; the guard must still drop here
            }
            #[allow(unreachable_code)]
            0
        }

        let ledger = Arc::new(PhaseLedger::new());
        let v = LEDGER
            .scope(Arc::clone(&ledger), region_with_early_return())
            .await;
        assert_eq!(v, 7);
        assert!(
            ledger.micros(Phase::Argon).is_some(),
            "PhaseScope must record even when the scoped region exits via an \
             early `return` — Drop runs on every exit path, branch or not"
        );
    }

    #[tokio::test]
    async fn phase_scope_with_handle_records_from_a_spawned_task() {
        // `opermit`'s acquire runs inside `FlightGroup::run`'s `tokio::spawn`ed
        // `lead` future, where the ambient task-local is not visible. Proves
        // the captured-handle path (`current_ledger` + `with_handle`) records
        // correctly from a genuinely different task, not just a nested future
        // on the same task.
        let ledger = Arc::new(PhaseLedger::new());
        let handle = LEDGER
            .scope(Arc::clone(&ledger), async { current_ledger() })
            .await;
        assert!(
            handle.is_some(),
            "current_ledger must see the scope it is called inside"
        );

        let spawned = tokio::spawn(async move {
            let _scope = PhaseScope::with_handle(handle, Phase::Permit);
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        });
        spawned.await.expect("spawned task must not panic");

        assert!(
            ledger.micros(Phase::Permit).is_some(),
            "a captured-handle PhaseScope must record from the spawned task \
             it runs on, not just from the task that opened the ledger scope"
        );
    }

    #[tokio::test]
    async fn current_ledger_is_none_with_no_scope_in_place() {
        // Mirrors `timed_is_a_pass_through_when_no_ledger_is_in_scope`: a
        // caller with no ledger in scope must get `None`, never panic.
        assert!(current_ledger().is_none());
    }

    #[tokio::test]
    async fn both_recording_mechanisms_reach_the_same_ledger() {
        // ⚠️ READ THIS BEFORE TRUSTING THIS TEST AS A SECURITY PROOF — IT IS NOT ONE.
        //
        // `found_arm` and `not_found_arm` below are two hand-written stand-ins
        // that are DELIBERATELY identical. A test whose two sides are identical
        // by construction can only prove that identical code behaves
        // identically, which is a tautology. It would pass unchanged even if the
        // real row-FOUND and row-NOT-FOUND arms in `adapter_pat.rs` were
        // asymmetric — the exact defect it looks like it is guarding against.
        //
        // What it DOES prove, and the only reason it is worth keeping: the two
        // recording MECHANISMS agree. `PhaseScope::enter` reads the ambient
        // task-local, while `PhaseScope::with_handle` carries a handle captured
        // on the originating task across a `tokio::spawn` (which the real
        // Argon2id flights need, because `FlightGroup::run` spawns its lead
        // future onto a task that cannot see this task-local). Both must land in
        // the same ledger and yield the same phase-name set. That is a real
        // property, and it is the one asserted here.
        //
        // Arm symmetry in `adapter_pat.rs` itself is NOT enforced by this test.
        // It is enforced by (1) code review of the two call sites, and (2) the
        // `CORELINK_ORIGIN_TIMING_DETAIL` gate, which is off in production and
        // makes both arms trivially indistinguishable on the wire because
        // neither emits these phases at all. Test (2)'s coverage is
        // `detail_phases_are_absent_when_the_gate_is_off`.
        async fn found_arm() {
            let _argon = PhaseScope::enter(Phase::Argon);
            let handle = current_ledger();
            tokio::spawn(async move {
                let _permit = PhaseScope::with_handle(handle, Phase::Permit);
            })
            .await
            .unwrap();
        }
        async fn not_found_arm() {
            let _argon = PhaseScope::enter(Phase::Argon);
            let handle = current_ledger();
            tokio::spawn(async move {
                let _permit = PhaseScope::with_handle(handle, Phase::Permit);
            })
            .await
            .unwrap();
        }

        let found_ledger = Arc::new(PhaseLedger::new());
        LEDGER.scope(Arc::clone(&found_ledger), found_arm()).await;
        let not_found_ledger = Arc::new(PhaseLedger::new());
        LEDGER
            .scope(Arc::clone(&not_found_ledger), not_found_arm())
            .await;

        let found_names: std::collections::HashSet<String> =
            parse(&found_ledger.server_timing_value_with(1_000, true))
                .into_keys()
                .collect();
        let not_found_names: std::collections::HashSet<String> =
            parse(&not_found_ledger.server_timing_value_with(1_000, true))
                .into_keys()
                .collect();
        assert_eq!(
            found_names, not_found_names,
            "the found and not-found PAT arms must produce the identical SET \
             of phase names — a caller must not be able to distinguish them \
             by which phases are present on the wire"
        );
        assert!(found_names.contains("oargon"));
        assert!(found_names.contains("opermit"));
    }
    /// The gate is OFF by default and that default must keep the header exactly
    /// as it was before the PAT-detail split existed: no `oargon`, no `opermit`,
    /// no `ortier`, no `oaudit`, and their time left inside the `oother` residue.
    ///
    /// This is a security property, not a formatting preference — see
    /// `detail_phases_enabled`. `opermit`'s presence reports whether the
    /// Argon2id flight ran, which reports the state of a per-process cache on
    /// the credential path.
    #[test]
    fn detail_phases_are_absent_when_the_gate_is_off() {
        let ledger = PhaseLedger::new();
        ledger.add(Phase::Pat, 10_000);
        ledger.add(Phase::Argon, 90_000);
        ledger.add(Phase::Permit, 5_000);
        ledger.add(Phase::Tier, 20_000);
        ledger.add(Phase::Audit, 15_000);

        let off = parse(&ledger.server_timing_value_with(200_000, false));
        assert!(
            !off.contains_key("oargon"),
            "oargon must not ship by default"
        );
        assert!(
            !off.contains_key("opermit"),
            "opermit must not ship by default"
        );
        assert!(
            !off.contains_key("ortier"),
            "ortier must not ship by default"
        );
        assert!(
            !off.contains_key("oaudit"),
            "oaudit must not ship by default"
        );
        assert_eq!(
            off.get("opat"),
            Some(&10),
            "the pre-existing phases are untouched"
        );
        // 200 total - 10 opat = 190; the four gated phases stay in the residue.
        assert_eq!(
            off.get("oother"),
            Some(&190),
            "gated time falls into oother"
        );

        // With the gate on, the same ledger partitions the very same total.
        let on = parse(&ledger.server_timing_value_with(200_000, true));
        assert_eq!(on.get("oargon"), Some(&90));
        assert_eq!(on.get("opermit"), Some(&5));
        assert_eq!(on.get("ortier"), Some(&20));
        assert_eq!(on.get("oaudit"), Some(&15));
        assert_eq!(on.get("oother"), Some(&60), "190 - 90 - 5 - 20 - 15 = 60");
    }
}
