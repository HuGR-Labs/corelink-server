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
//! | `ostore`  | the moat storage lookup's CAS/R2 blob work and, on the native CAS/AC plane, the R2/S3 object GET/PUT/DELETE/LIST calls `R2CasHandler`/`R2AcHandler` make through the sync `block_in_place` bridge (`storage/r2_s3.rs`) — **and**, on the native CAS/AC `list()` read path specifically, the CONCURRENT audit write that now runs alongside the R2 `ListObjectsV2` call — and, on the Bazel `findMissingBlobs` path, the whole joined window of the batched audit write plus the concurrent R2 `HeadObject` probes (see "Concurrent native-plane list seam" below) |
//! | `oaccounting` | D1 storage-byte accounting and adapter URL-map writes/reads. It is intentionally separate from `ostore`; the two scopes never overlap. |
//! | `oargon`  | Argon2id verification (`adapter_pat.rs`): the secret-match memo check plus, on a miss, the coalesced verify flight — AND, on the SAME name, the row-not-found coalesced dummy Argon2id burn that pads timing for a missing/expired/revoked `token_id` (see the security note below) |
//! | `opermit` | the semaphore acquires bounded by `ARGON2_PERMIT_WAIT`, in both the dummy-burn arm and the real verify arm |
//! | `ortier`  | `ensure_tier_applied`'s D1 tier-label resolution (`routes/ratelimit_layer.rs` → `oci_cap.rs`) |
//! | `oaudit`  | the blocking durable-audit D1 write on the request path (`D1AuditOutboxSink::write_blocking`, `storage/d1_audit_sink.rs`) — every `AuditSink::emit`/`append` the native CAS/AC handlers make before/after a mutation or read routes through this one blocking D1-over-HTTP `INSERT`. **Exceptions:** the native CAS/AC `list()` read path's `ListAttempted` audit write, and the Bazel `findMissingBlobs` path's batched `ReadAttempted` write, do NOT appear here when they run concurrently with the R2 calls — see below. |
//! | `oratelimit` | the synchronous per-tenant/OCI token-bucket admission decision; it covers no awaited handler work, so it cannot overlap the named D1/R2 phases |
//! | `ohandler` | request-framework work outside a more specific phase: routing, HMAC, body handling, response assembly |
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
//! prevent. The reconciliation is unaffected — the named phases still sum
//! EXACTLY to the time the container held the request, because `ohandler` accounts for the
//! remaining framework work and the phases partition, not label, the work.
//! During the mixed-rollout window the wire also carries an identical `oother`
//! compatibility alias; new Workers normalize that alias and old Workers still
//! recognize it instead of charging the framework window to `ohop`.
//! This makes the rollout order safe in either direction: deploy the dual
//! producer before or after the parser, keep the alias while old Workers drain,
//! then remove it only in a later cleanup once the old allowlist is gone.
//!
//! `ohandler` is computed from the layer's whole-request clock after every
//! specific phase has been accounted for. It is an explicit request-framework
//! phase, not an anonymous residue: the Worker allowlist consumes it and the
//! canonical phases always partition the container window; the legacy alias is
//! normalized away by new Workers.
//!
//! ## Concurrent native-plane list seam — the ONE place two phases now overlap
//!
//! Distinct phases on a production path are entered and exited one at a time;
//! their regions are sequential, so summing their accumulated microseconds is
//! a true partition of the request and can never double-count a millisecond.
//! When a same-phase region crosses a task boundary, the shared ledger
//! coalesces nested and overlapping scopes into one wall-clock window.
//! `R2CasHandler::list` / `R2AcHandler::list` (`storage/r2_s3.rs`) are the ONE
//! exception: the mandatory `ListAttempted` durable-audit write and the R2
//! `ListObjectsV2` call now run CONCURRENTLY, `tokio::join!`ed under a single
//! `block_in_place` + `block_on`, because they measurably don't need to be
//! serial (the audit write and the R2 call don't read each other's result —
//! see `r2_s3.rs`'s `list()` doc for the fail-CLOSED argument).
//!
//! If BOTH sides entered their own `PhaseScope` (`Phase::Audit` and
//! `Phase::Store`) for that overlapping wall-clock window, their accumulated
//! microseconds would NOT partition the request any more — the SAME
//! milliseconds would be counted under two names, `attributed_ms` could
//! exceed `total_ms`, and `ohandler`'s `(total_ms - attributed_ms).max(0)` guard
//! would silently swallow the overcount into a floor of zero rather than
//! reporting it. That would make the header LIE by omission — `Σ(phases)`
//! would no longer be a request partition even though nothing overflowed.
//!
//! The concurrent seam avoids this by attributing the WHOLE joined window
//! ONCE, to `Phase::Store`, and never entering `Phase::Audit` for it:
//! `D1AuditOutboxSink::append_async` (the audit half of the join) is a bare
//! future with NO `PhaseScope` of its own — only the serial
//! `write_blocking` bridge (used by every other audit call, including
//! `list()`'s own fallback path when no async-capable sink is wired) still
//! enters `Phase::Audit`. So: on the concurrent `list()` path, `ostore`
//! reports the FULL joined window (audit + R2, whichever finishes last) and
//! `oaudit` reports nothing for that specific write — `oaudit` is
//! unaffected everywhere else (every mutation path — write/update/delete —
//! stays fully serial and keeps entering `Phase::Audit` exactly as before;
//! see `r2_s3.rs` for why those paths were NOT made concurrent). The
//! named-phase sum therefore still partitions the request exactly, by
//! construction, not by the `max(0)` guard papering over an overcount.
//!
//! ### The `findMissingBlobs` batch seam obeys the SAME rule
//!
//! `R2CasHandler::exists_batch` (`storage/r2_s3.rs`, the Bazel REAPI
//! `findMissingBlobs` path) is the second — and, at time of writing, last —
//! place two kinds of work overlap. It joins ONE batched `ReadAttempted`
//! audit write (`D1AuditOutboxSink::append_batch_async`, N rows in one
//! statement) with up to `MAX_CONCURRENT_EXISTS_PROBES` in-flight R2
//! `HeadObject` probes.
//!
//! It is attributed by exactly the rule above, applied twice over:
//!
//! * **Across the two kinds of work** — ONE `Phase::Store` scope wraps the
//!   whole joined window and `Phase::Audit` is never entered for it
//!   (`append_batch_async`, like `append_async`, opens no scope of its own),
//!   so the audit and the storage halves cannot both bill the same
//!   milliseconds.
//! * **Across the concurrent probes themselves** — the per-probe helper
//!   (`probe_existence_unaudited`) opens NO `PhaseScope` either. N probes
//!   overlapping in one window, each entering `Phase::Store`, would bill that
//!   window N times over and could make `ostore` alone exceed `total_ms`; the
//!   single outer scope bills the wall-clock window once.
//!
//! So on this path too, `ostore` reports the FULL joined window (batched
//! audit + all probes, whichever finishes last) and `oaudit` reports nothing
//! for that specific write. The rule to carry forward when adding any future
//! concurrent seam: **the joined window gets exactly one `PhaseScope`, opened
//! by whoever owns the join — never one per concurrent branch.**
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
//!   `ohandler`. The phase is named for what it measures, not for what one might
//!   wish it measured.
//! * `oaudit` instruments [`crate::storage::d1_audit_sink::D1AuditOutboxSink`]'s
//!   `write_blocking` — the ONE blocking-D1 choke point every CAS/AC audit
//!   `emit`/`append` call routes through, native plane only (the moat
//!   surfaces' cache-hit audit trail is a separate, already-async path and is
//!   not in `oaudit`). EXCEPT the native `list()` read path's concurrent
//!   audit write, which is charged to `ostore` instead — see "Concurrent
//!   native-plane list seam" above.
//! * The layer stops timing when the handler returns its `Response`. For a
//!   buffered body (every route on the measured `/cargo` path) that is the whole
//!   cost; for a streamed body the streaming itself is outside `ohandler` — and it
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
//! `Σ(parts) ≤ total` holds by the monotonicity of `floor` and `ohandler` can
//! never be negative.
//!
//! # Overhead
//!
//! Per request: one `Arc` allocation, one task-local scope, two `Instant::now()`
//! calls in the layer plus one short mutex-protected window transition per
//! phase, three relaxed atomic adds, and one header format (~50 bytes). The
//! lock is held only while changing a phase's active-window counter; it never
//! spans I/O. This is the necessary cost of making nested and genuinely
//! concurrent scopes report the union of wall windows rather than either
//! double-counting or dropping a sibling's time.
//!
//! # Recording is best-effort by construction
//!
//! [`timed`] records into a task-local ledger and is a plain pass-through when
//! no ledger is in scope (unit tests, background tasks, any non-HTTP caller).
//! Instrumentation must never be able to change a result, so there is no error
//! path here at all.

use std::future::Future;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
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
/// attack `origin`; everything else is deliberately pooled into the `ohandler`
/// residue rather than split into names nobody would act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The per-request D1 `pat` row read (`opat`).
    Pat,
    /// The per-tenant `$`-ceiling quota check/accrue (`oquota`).
    Quota,
    /// CAS/R2 durable storage work (`ostore`). Adapter URL-map and byte
    /// accounting D1 calls use [`Phase::Accounting`] so the two costs can be
    /// measured independently without overlapping the request partition.
    Store,
    /// D1 storage-byte accounting and adapter URL-map operations
    /// (`oaccounting`). This phase is deliberately bounded around the actual
    /// D1 windows; it never wraps the inner R2 handler.
    Accounting,
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
    /// `ensure_tier_applied`'s tier resolution (`ortier`).
    Tier,
    /// The blocking durable-audit D1 write on the request path (`oaudit`):
    /// `D1AuditOutboxSink::write_blocking` (`storage/d1_audit_sink.rs`), the
    /// single choke point every CAS/AC `AuditSink::emit`/`append` call routes
    /// through before/after a native-plane read or mutation.
    Audit,
    /// The synchronous per-tenant/OCI token-bucket admission decision
    /// (`oratelimit`). This is deliberately scoped around only the
    /// `try_acquire` call(s); handler execution remains outside this phase, so
    /// it cannot overlap `opat`/`oquota`/`ostore` or any other awaited phase.
    RateLimit,
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
/// in the write gate and the adapter may verify again). Sequential windows
/// **add**, while nested or concurrent windows are coalesced into their union;
/// this keeps the sum a partition of the request across synchronous and
/// blocking-task boundaries.
#[derive(Debug)]
pub struct PhaseLedger {
    pat_us: AtomicI64,
    quota_us: AtomicI64,
    store_us: AtomicI64,
    argon_us: AtomicI64,
    permit_us: AtomicI64,
    tier_us: AtomicI64,
    audit_us: AtomicI64,
    accounting_us: AtomicI64,
    rate_limit_us: AtomicI64,
    windows: [Mutex<PhaseWindow>; 9],
    #[cfg(test)]
    completed_windows: [AtomicUsize; 9],
    #[cfg(test)]
    recordings: [AtomicUsize; 9],
}

#[derive(Debug, Default)]
struct PhaseWindow {
    active: usize,
    started: Option<Instant>,
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
            accounting_us: AtomicI64::new(DID_NOT_RUN),
            rate_limit_us: AtomicI64::new(DID_NOT_RUN),
            windows: std::array::from_fn(|_| Mutex::new(PhaseWindow::default())),
            #[cfg(test)]
            completed_windows: std::array::from_fn(|_| AtomicUsize::new(0)),
            #[cfg(test)]
            recordings: std::array::from_fn(|_| AtomicUsize::new(0)),
        }
    }

    fn slot(&self, phase: Phase) -> &AtomicI64 {
        match phase {
            Phase::Pat => &self.pat_us,
            Phase::Quota => &self.quota_us,
            Phase::Store => &self.store_us,
            Phase::Accounting => &self.accounting_us,
            Phase::Argon => &self.argon_us,
            Phase::Permit => &self.permit_us,
            Phase::Tier => &self.tier_us,
            Phase::Audit => &self.audit_us,
            Phase::RateLimit => &self.rate_limit_us,
        }
    }

    fn window_slot(&self, phase: Phase) -> &Mutex<PhaseWindow> {
        match phase {
            Phase::Pat => &self.windows[0],
            Phase::Quota => &self.windows[1],
            Phase::Store => &self.windows[2],
            Phase::Accounting => &self.windows[3],
            Phase::Argon => &self.windows[4],
            Phase::Permit => &self.windows[5],
            Phase::Tier => &self.windows[6],
            Phase::Audit => &self.windows[7],
            Phase::RateLimit => &self.windows[8],
        }
    }

    #[cfg(test)]
    fn completed_window_counter(&self, phase: Phase) -> &AtomicUsize {
        match phase {
            Phase::Pat => &self.completed_windows[0],
            Phase::Quota => &self.completed_windows[1],
            Phase::Store => &self.completed_windows[2],
            Phase::Accounting => &self.completed_windows[3],
            Phase::Argon => &self.completed_windows[4],
            Phase::Permit => &self.completed_windows[5],
            Phase::Tier => &self.completed_windows[6],
            Phase::Audit => &self.completed_windows[7],
            Phase::RateLimit => &self.completed_windows[8],
        }
    }

    #[cfg(test)]
    fn recording_counter(&self, phase: Phase) -> &AtomicUsize {
        match phase {
            Phase::Pat => &self.recordings[0],
            Phase::Quota => &self.recordings[1],
            Phase::Store => &self.recordings[2],
            Phase::Accounting => &self.recordings[3],
            Phase::Argon => &self.recordings[4],
            Phase::Permit => &self.recordings[5],
            Phase::Tier => &self.recordings[6],
            Phase::Audit => &self.recordings[7],
            Phase::RateLimit => &self.recordings[8],
        }
    }

    /// Enter a phase's active wall-clock window.
    ///
    /// A phase is a partition of request wall time, not an arbitrary label.
    /// Consequently an inner scope of the same phase must not add its already
    /// covered window a second time. The active count is shared by all tasks
    /// that carry this request ledger, so this remains true across a
    /// `spawn_blocking` boundary and also handles genuinely overlapping
    /// sibling scopes without dropping either one's union.
    fn begin(&self, phase: Phase) {
        let mut window = self
            .window_slot(phase)
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if window.active == 0 {
            window.started = Some(Instant::now());
        }
        window.active += 1;
    }

    /// Leave a phase scope and close the union window when its final scope
    /// exits. A poisoned lock is recovered because timing is best-effort and
    /// must never turn an application response into an instrumentation error.
    fn end(&self, phase: Phase) {
        let elapsed = {
            let mut window = self
                .window_slot(phase)
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if window.active == 0 {
                return;
            }
            window.active -= 1;
            if window.active == 0 {
                window.started.take().map(|started| started.elapsed())
            } else {
                None
            }
        };
        if let Some(elapsed) = elapsed {
            let micros = i64::try_from(elapsed.as_micros()).unwrap_or(i64::MAX);
            self.add(phase, micros);
            #[cfg(test)]
            self.completed_window_counter(phase)
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    #[cfg(test)]
    fn completed_windows_for_test(&self, phase: Phase) -> usize {
        self.completed_window_counter(phase).load(Ordering::Relaxed)
    }

    #[cfg(test)]
    fn recordings_for_test(&self, phase: Phase) -> usize {
        self.recording_counter(phase).load(Ordering::Relaxed)
    }

    #[cfg(test)]
    fn active_depth_for_test(&self, phase: Phase) -> usize {
        self.window_slot(phase)
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active
    }

    /// Add `micros` to `phase`, promoting it out of the not-run sentinel.
    fn add(&self, phase: Phase, micros: i64) {
        #[cfg(test)]
        self.recording_counter(phase)
            .fetch_add(1, Ordering::Relaxed);
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
    /// omitted entirely. `ohandler` accounts for request-framework work, so the
    /// canonical emitted phases sum EXACTLY to `floor(total_us / 1000)`; the
    /// compatibility alias is intentionally excluded from that arithmetic.
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
        let mut parts: Vec<String> = Vec::with_capacity(11);
        let mut attributed_ms: i64 = 0;
        for (name, phase) in [
            ("opat", Phase::Pat),
            ("oquota", Phase::Quota),
            ("ostore", Phase::Store),
            ("oaccounting", Phase::Accounting),
            ("oargon", Phase::Argon),
            ("opermit", Phase::Permit),
            ("ortier", Phase::Tier),
            ("oaudit", Phase::Audit),
            ("oratelimit", Phase::RateLimit),
        ] {
            // Only the CREDENTIAL-PATH pair is gated (see
            // `detail_phases_enabled`). `ortier`/`oaudit` publish
            // unconditionally: they carry no credential oracle and are already
            // explicit phases. Keeping this accounting named avoids silently
            // charging framework work to the DO hop.
            if !detail && matches!(phase, Phase::Argon | Phase::Permit) {
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
        let handler_ms = (total_ms - attributed_ms).max(0);
        parts.push(format!("ohandler;dur={handler_ms}"));
        // Keep the historical name for one safe mixed-rollout window. Old
        // Workers only allowlist `oother` and would otherwise drop the new
        // phase, inflating `ohop`. New Workers normalize and deduplicate this
        // exact-value alias, so it cannot double-count the partition.
        parts.push(format!("oother;dur={handler_ms};desc=\"legacy-alias\""));
        parts.join(", ")
    }
}

/// Whether the two CREDENTIAL-PATH detail phases (`oargon`, `opermit`) are
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
/// `ortier` and `oaudit` carry no such credential-oracle risk on their own, and
/// **they are no longer behind this flag.** They were, as a single conservative
/// opt-in for everything the split added — with the note that there was "currently
/// no reason to ship `oaudit` alone". B-109 is that reason.
///
/// The 2026-08-30 production profile measured the then-unnamed framework work at
/// 139 ms on a warm PUT
/// against 1 ms on the GET: real container work, unnamed, because the residue is
/// computed by subtraction and these two were being subtracted into it. The only
/// way to enumerate it was to arm this flag in production — which would publish
/// the credential oracle above to every caller, for as long as the diagnostic
/// window lasted. **A performance diagnostic must not widen a security window.**
/// Splitting the gate enumerates the two neutral phases permanently and keeps the
/// two that are not neutral shut.
///
/// What remains gated is exactly what the oracle argument covers: `oargon` and
/// `opermit`, both on the credential path, both reporting per-process cache state
/// by their PRESENCE. When off they are not emitted and their time falls into
/// `ohandler`, whose meaning is explicit. The ledger still RECORDS them
/// unconditionally (an `Instant` is free), so arming the flag needs no rebuild of
/// the timing code, only a redeploy of this gate.
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
    /// and falls into `ohandler`.
    static LEDGER: Arc<PhaseLedger>;
}

// `spawn_blocking` closures are synchronous and therefore cannot carry the
// Tokio task-local above.  Keep a thread-local bridge for the duration of the
// closure so code below the sync CAS traits can still open *its own* bounded
// phase (R2 versus D1 accounting).  This is deliberately scoped and restored
// by `PhaseScope`; a worker thread never retains a request ledger after the
// closure returns.
thread_local! {
    static BLOCKING_LEDGER: std::cell::RefCell<Option<Arc<PhaseLedger>>> =
        const { std::cell::RefCell::new(None) };
}

/// Time `fut` and attribute its wall duration to `phase`.
///
/// Pass-through (zero recording, no allocation) when no ledger is in scope.
pub async fn timed<F: Future>(phase: Phase, fut: F) -> F::Output {
    // Use the same RAII guard as multi-statement regions. Besides keeping the
    // pass-through behaviour when no ledger is installed, this prevents a
    // nested `timed`/`PhaseScope` pair from double-counting one wall window.
    let _scope = PhaseScope::enter(phase);
    fut.await
}

/// Run `fut` with `ledger` installed as the ambient task-local, for tests in
/// OTHER modules of this crate that need to observe what a real code path
/// records (e.g. `adapter_cache`'s proof that `MoatCache::put` double-counts
/// `Phase::Store` when the CAS handler re-enters it).
///
/// Test-only: production installs the ledger exactly once, in
/// [`origin_timing_layer`], and nothing else may scope one.
#[cfg(test)]
pub(crate) async fn scope_for_test<F: Future>(ledger: Arc<PhaseLedger>, fut: F) -> F::Output {
    LEDGER.scope(ledger, fut).await
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
    LEDGER
        .try_with(Arc::clone)
        .ok()
        .or_else(|| BLOCKING_LEDGER.with(|slot| slot.borrow().as_ref().map(Arc::clone)))
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
    previous_blocking_ledger: Option<Option<Arc<PhaseLedger>>>,
}

impl PhaseScope {
    /// Time a region reached on the SAME task that opened the ledger's scope —
    /// captures the ambient task-local, like [`timed`], but for a region that
    /// is not a single future.
    #[must_use]
    pub fn enter(phase: Phase) -> Self {
        let ledger = current_ledger();
        if let Some(ledger) = &ledger {
            ledger.begin(phase);
        }
        Self {
            ledger,
            phase,
            previous_blocking_ledger: None,
        }
    }

    /// Time a region reached on a DIFFERENT task than the one that opened the
    /// ledger's scope (e.g. inside a `tokio::spawn`ed future). The caller must
    /// capture `ledger` with [`current_ledger`] on the ORIGINATING task,
    /// before spawning — see that function's doc for why.
    #[must_use]
    pub fn with_handle(ledger: Option<Arc<PhaseLedger>>, phase: Phase) -> Self {
        if let Some(ledger) = &ledger {
            ledger.begin(phase);
        }
        Self {
            ledger,
            phase,
            // This constructor is also used by spawned async futures. Do not
            // install a thread-local across an await: Tokio may run another
            // request on the same worker thread. Synchronous closures use
            // `with_ledger` below instead.
            previous_blocking_ledger: None,
        }
    }

    /// Install a captured ledger on a synchronous task without opening a
    /// phase.  Use this at a `spawn_blocking` boundary when the sync callee
    /// owns the phase boundaries (for example, the accounting decorator owns
    /// `oaccounting` while the R2 handler owns `ostore`).
    #[must_use]
    pub fn with_ledger(ledger: Option<Arc<PhaseLedger>>) -> Self {
        let previous_blocking_ledger = BLOCKING_LEDGER.with(|slot| slot.replace(ledger.clone()));
        Self {
            ledger: None,
            phase: Phase::Store,
            previous_blocking_ledger: Some(previous_blocking_ledger),
        }
    }
}

impl Drop for PhaseScope {
    fn drop(&mut self) {
        if let Some(ledger) = &self.ledger {
            ledger.end(self.phase);
        }
        if let Some(previous) = self.previous_blocking_ledger.take() {
            BLOCKING_LEDGER.with(|slot| {
                // `take` above removes the outer bookkeeping `Option`; the
                // value left here is already the thread-local's
                // `Option<Arc<PhaseLedger>>`.  Restoring it directly keeps a
                // previously installed ledger (including `None`) intact.
                slot.replace(previous);
            });
        }
    }
}

/// Axum middleware that scopes a [`PhaseLedger`] over the request and stamps
/// the resulting `origin` sub-phases onto the response's `Server-Timing`.
///
/// Wired as the OUTERMOST data-plane layer (added last in
/// [`crate::routes::build_with_factory`]) so its own clock spans everything the
/// container does — including the rate-limit layer and the OTel export layer —
/// which is what makes `ohandler` a complete framework phase rather than a
/// partial one.
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

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// Split by CONCERN, not by size. Each file states the property it pins:
//
//   `tests_emission`  — what the ledger emits, and the partition arithmetic that
//                       keeps `sum(phases) + ohandler` equal to the container total.
//   `tests_gate`      — which detail phases publish. A security property: the
//                       PRESENCE of `opermit` reports whether the Argon2id flight ran.
//   `tests_recording` — the two recording mechanisms (ambient task-local vs a
//                       handle carried across a spawn) and where each one works.
//   `tests_layer`     — the axum middleware end to end.
//   `tests_support`   — the one shared helper, here rather than in a sibling so
//                       no test file looks load-bearing for the others.

#[cfg(test)]
mod tests_support;

#[cfg(test)]
mod tests_emission;

#[cfg(test)]
mod tests_gate;

#[cfg(test)]
mod tests_layer;

#[cfg(test)]
mod tests_recording;

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests_b279_bridge {
    use std::sync::Arc;

    use super::{current_ledger, PhaseLedger, PhaseScope};

    #[test]
    fn blocking_bridge_restores_the_previous_option() {
        let outer = Arc::new(PhaseLedger::new());
        let inner = Arc::new(PhaseLedger::new());
        std::thread::spawn({
            let outer = Arc::clone(&outer);
            let inner = Arc::clone(&inner);
            move || {
                let outer_bridge = PhaseScope::with_ledger(Some(Arc::clone(&outer)));
                assert!(
                    current_ledger().is_some_and(|ledger| Arc::ptr_eq(&ledger, &outer)),
                    "outer bridge must install its captured ledger"
                );
                {
                    let inner_bridge = PhaseScope::with_ledger(Some(Arc::clone(&inner)));
                    assert!(
                        current_ledger().is_some_and(|ledger| Arc::ptr_eq(&ledger, &inner)),
                        "nested bridge must shadow the outer ledger"
                    );
                    drop(inner_bridge);
                }
                assert!(
                    current_ledger().is_some_and(|ledger| Arc::ptr_eq(&ledger, &outer)),
                    "dropping nested bridge must restore the previous ledger"
                );
                drop(outer_bridge);
                assert!(
                    current_ledger().is_none(),
                    "dropping outer bridge must restore the previous None"
                );
            }
        })
        .join()
        .expect("bridge restoration thread must not panic");
    }
}
