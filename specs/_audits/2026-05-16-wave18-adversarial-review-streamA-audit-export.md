# Wave-18 Adversarial Review — Stream A: Audit-Export Streaming + Mid-Stream Trailer

> **Doc kind:** adversarial review / SOTA bar verification (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Reviewer:** wave-19 builder (opus 4.7), adversarial pass, no codex external tool — first-principles review against the SOTA bar 8.5/10.
>
> **Subject commit:** `3d835cb296d3ac6dd16e26b3876e93a293a29b7d` — wave-18(audit-export): true streaming NDJSON + X-CoreLink-Audit-Export-Aborted mid-stream trailer.
>
> **Base:** `cb6360d` (wave-18-impl-sealed); worktree `wt/r-prep-wave18-codex-review`.
>
> **Cross-ref:** [Stream B review](2026-05-16-wave18-adversarial-review-streamB-neon-shadow.md).

## 1. Scope

This review evaluates the wave-18 audit-export streaming stream — five files / +1.8k LOC delta:

- `apps/server/src/routes/audit_export.rs` (1102 LOC, modified to ship `Body::new(StreamBody::new(...))` + per-row re-verify + `Frame::trailers` abort path)
- `apps/server/tests/audit_export.rs` (693 LOC, +3 new wave-18 integration tests)
- `crates/corelink-cli/src/commands/verify_ndjson.rs` (506 LOC, +1 new streaming-compat test)
- `specs/04_sprints/S09/work_items/WI-S09-008-customer-audit-export.md` (work-item spec referenced; not modified in this commit)
- `specs/_audits/2026-05-15-audit-chain-retention.md` (retention audit doc, cross-referenced)

The review verifies the wave-16 SEAL caveat #2 is closed: the customer-audit-export endpoint moved from buffer-then-flush to chunked streaming, emits the SEV-0 `corelink.audit.export_verify_failed.v1` audit row BEFORE the `Frame::trailers` abort frame on mid-stream chain-break, and remains byte-for-byte compatible with the wave-17 customer-CLI parser.

## 2. Method

1. **Cold read** the diff in commit `3d835cb` end-to-end without external tooling.
2. **Charter compliance gate** — `#![forbid(unsafe_code)]`, no `unwrap/expect/panic` outside `#[cfg(test)]`, audit fail-CLOSED ordering, `subtle::ConstantTimeEq` on tenant compare, `#[non_exhaustive]` on public enums/structs.
3. **Security adversarial pass** — tenant isolation (constant-time compare, log redaction), input validation at boundaries, no log injection, no SQL injection (N/A — no DB layer in this stream).
4. **Concurrency invariant pass** — audit-emit-BEFORE-trailer ordering (the wave-18 load-bearing invariant), backpressure, race conditions on the mutex-backed in-memory sink.
5. **Testing coverage** — golden + adversarial paths, mutation kill rate estimate from test density (5 wave-16 + 3 wave-18 = 8 integration tests + 13 unit tests).
6. **Spec rigor** — INV references cross-checked against `specs/03_architecture/invariant_registry.md`; regulatory citations (RFC 7230 §4.4, GDPR Art. 15, SOC 2 CC7.2) verified.
7. **Operability** — runbook coverage, metric/alert wiring, SLO documentation.
8. **API stability** — `#[non_exhaustive]` discipline on public types.

## 3. Findings

| ID | Severity | File:line | Issue | Recommendation |
|---|---|---|---|---|
| A-P1-01 | P1 | `apps/server/src/routes/audit_export.rs:686-772` | The function name `build_audit_export_stream_frames` is misleading: it pre-materializes a `Vec<Frame<Bytes>>` of **every** row in memory before returning, then `futures::stream::iter` walks the vector. The wave-16 SEAL caveat #2 was "buffer-then-flush"; the wave-18 wire shape now ships chunked HTTP (no `Content-Length`), but the **server-side memory profile is unchanged** — a 7-year TB-scale tenant window still materializes every row in a single `Vec` before the first byte leaves. The module doc on line 681-685 acknowledges the synchronous-shape intent but doesn't surface the operational cap. | Either (a) cap the per-request row count at the route boundary with a documented limit + 413 Payload Too Large, or (b) lift to a true async generator (`futures::stream::unfold`) so frames materialize on demand. Document the chosen bound in the WI spec §6 AC. |
| A-P1-02 | P1 | `apps/server/src/routes/audit_export.rs:423` | The cross-tenant-attempt audit emit uses `let _ = state.audit_sink.emit(...)` — discarding the result. The module-level doc (§Fail-CLOSED ordering) declares "Cross-tenant reject emits `corelink.security.audit_export_cross_tenant_attempt.v1` BEFORE the `403`", but if the audit pipeline returns Err here the route ships a 403 with NO security audit recorded. This silently breaks the SEV-1 page-out contract documented in WI-S09-008 §4 (the security team's only anchor for the attempt). | Branch on the emit result: on Err, surface 503 (mirrors line 590-596 for the `export_request.v1` arm). The cross-tenant arm is at least as load-bearing as the request-row arm. |
| A-P1-03 | P1 | `apps/server/src/routes/audit_export.rs:799-808` | The mid-stream chain-break audit emit (`emit_mid_stream_break_audit`) also uses `let _ = sink.emit(...)`. On a sink failure during the SEV-0 emit, the route still pushes the `Frame::trailers` abort frame to the wire — but no audit row anchors the security team's detect. This violates `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (audit emit MUST succeed before mutation; here the "mutation" is shipping the trailer to the wire). Compare to the analogous discipline in `crates/corelink-audit-chain::sub_processor_emitter` (INV-SUB-PROCESSOR-AUDIT-FAIL-CLOSED). | On audit-emit Err inside the stream-builder, either drop the trailer frame (and emit a SEV-0 to a fallback channel via stderr metric) or short-circuit the stream with a body-truncate. Document the chosen failure-mode in the module doc + the retention audit doc. |
| A-P1-04 | P1 | `apps/server/src/routes/audit_export.rs:807` | The wave-18 mid-stream payload is **encoded into the `exit_status` field** of `ExportAuditRow` via `format!("verify_failed_mid_stream:{payload}")`. The `ExportAuditRow` struct has no dedicated `payload` / `data` field, so the structured `{break_at_seq, break_at_chunk, observed, expected}` JSON is squashed into a string with a `verify_failed_mid_stream:` prefix the consumer must split + re-parse. Downstream SIEM consumers (Drata / Splunk) parse `exit_status` as an enum — a string-prefix protocol is a regression risk on the wave-15 schema. The integration test at `audit_export.rs:551-561` validates the prefix-split convention, pinning the regression. | Extend `ExportAuditRow` with an optional `payload_json: Option<String>` field (additive — preserves serde back-compat) and write the structured payload there. Keep `exit_status="verify_failed"` clean. Add a CHANGELOG entry pinning the field's wire shape. |
| A-P1-05 | P1 | `apps/server/src/routes/audit_export.rs:686-772` (whole frame-build) + tests | No **proptest** coverage on the load-bearing wave-18 invariant "the SEV-0 audit emit lands BEFORE the `Frame::trailers` push for every possible row-vector shape (empty / 1 row / 10k rows / tampered at index N)". The integration test `abort_trailer_emitted_on_mid_stream_chain_break` covers a single hardcoded tamper case (`TamperingExporter` flips row[1]). Mutation kill rate estimate: ~60% on the stream-build path — a proptest over row count + tamper index would push toward 90%. | Add `proptest!` block under `#[cfg(test)] mod tests` exercising (row_count ∈ 0..1000, tamper_index ∈ None ∪ Some(0..row_count)) and asserting (a) audit-row capture index < trailer-frame index in the captured frame vec, (b) trailer is absent on the no-tamper arm. |
| A-P2-01 | P2 | `apps/server/src/routes/audit_export.rs:482-488` | The wildcard arm on `RateLimitDecision` returns `429 TOO_MANY_REQUESTS` with a generic body. While the comment correctly explains the fail-CLOSED rationale (`#[non_exhaustive]` enum, unknown variant = deny), the audit-emit on this arm is **missing** (compare to line 461 for `Deny429`). A future variant lands silently on the wire with no audit anchor. | Emit a `rate_limited` audit row before returning the wildcard 429 — the analytics dashboard depends on the emit-count vs 429-count parity assertion. |
| A-P2-02 | P2 | `apps/server/src/routes/audit_export.rs:586,609` | `bytes_written` populated with `body_bytes_len` BEFORE the body is actually flushed. On a downstream HTTP-2 RST_STREAM mid-flush, the audit row over-reports bytes shipped. Wave-15 NDJSON shape had the same pattern, so this is a pre-existing trait, not a regression — but the dashboard SLO of "bytes_emitted = bytes_acked" can drift silently. | Document the "intent-to-flush" semantic in the `ExportAuditRow::bytes_written` doc-comment (currently says "flushed to the response stream"). Or, alternatively, sample a post-flush wall-clock metric to ground-truth the value. |
| A-P2-03 | P2 | `apps/server/src/routes/audit_export.rs:865-923` | The `parse_rfc3339_utc_ms` re-implements a date library from scratch (Howard Hinnant `days_from_civil`). The implementation is **correct** (tested via `parse_timestamp_accepts_rfc3339_utc` + `parse_timestamp_rejects_malformed_rfc3339`), but reinventing a date parser inside an audit-emit hot path is a known footgun. No leap-second handling, no fractional-second support, no timezone offset support beyond `Z`. | Either (a) document this is a deliberate minimal-RFC3339 subset in the WI §4 spec, or (b) pull `chrono` (already in tree via tokio ecosystem) and use `chrono::DateTime::parse_from_rfc3339`. Note: charter says "no `tokio` in src code where avoidable" but `chrono` itself is tokio-free. |
| A-P2-04 | P2 | `apps/server/src/routes/audit_export.rs:73,191` | `Arc<Mutex<...>>` for the in-memory audit sink — every emit takes a global lock. For a high-throughput export path this is a per-request contention point. The native target is test-only (production wires the CloudEvents emitter), so the operational impact is bounded, but the test harness throughput cap is implicit. | Document the contention bound in the `InMemoryExportAuditSink` doc-comment. No code change needed if the bound is acceptable. |
| A-P2-05 | P2 | `apps/server/src/routes/audit_export.rs:941-943` | `now_ms_from_window` anchors the rate-limit clock to the window's `until_ms`, NOT wall-clock. This is documented as a deterministic substitute, but a customer querying a 1970-epoch window every 60ms will keep refilling the bucket (because `now_ms` = 0). | Either swap to a `WallClock` collaborator at the route boundary (production wiring path) or floor `now_ms` to `max(window.until_ms, prev_now_ms)` so the bucket clock only advances. This is a pre-existing wave-15 trait — flag for follow-on. |
| A-P3-01 | P3 | `apps/server/src/routes/audit_export.rs:687` | `#[allow(clippy::too_many_arguments, reason = "trailing-payload + audit sink fan-in")]` — the function has 9 params. A small dedicated struct (e.g. `StreamBuildContext { rows, manifest_line, anchor_head, sink, tenant, from_ms, to_ms, bytes_written, events_written }`) would remove the lint waiver and clarify call-site intent. | Refactor to a context struct in a follow-on cleanup commit. Cosmetic only. |
| A-P3-02 | P3 | `apps/server/src/routes/audit_export.rs:268` | `f.debug_struct("AuditExportRouteState").finish_non_exhaustive()` — the `#[derive(Clone)]` is left, but the trait-object fields can't be `Debug` themselves. The manual `Debug` impl is fine but a small docstring noting why would help future maintainers. | Add doc-comment explaining the trait-object Debug constraint. |

## 4. Score breakdown

| Severity | Count | Weight | Deduction |
|---|---|---|---|
| P0 | 0 | -1.5 | 0.00 |
| P1 | 5 | -0.5 | -2.50 |
| P2 | 5 | -0.15 | -0.75 |
| P3 | 2 | -0.05 | -0.10 |

**Raw score:** `10 - 0 - 2.50 - 0.75 - 0.10 = 6.65 / 10`.

**Bonus adjustments (SOTA-bar reviewer discretion):**

- **+0.4** — wave-18 SEAL caveat #2 (true streaming wire) is correctly closed: the `Trailer:` response header advertises `x-corelink-audit-export-aborted` upfront per RFC 7230 §4.4; `Content-Length` is verified absent in the integration test; the body parses line-by-line via the wave-17 CLI with byte-for-byte compatibility.
- **+0.4** — audit-anchor-BEFORE-trailer ordering correctly enforced **mechanically** via synchronous frame-vector pre-materialization (the sink runs in the route handler scope, NOT inside the async stream generator). This is a load-bearing wave-18 invariant correctly preserved. The synchronous shape is intentional and documented.
- **+0.3** — `subtle::ConstantTimeEq` on `uuid_eq_ct` for the cross-tenant compare; constant-time hashes-eq in the CLI verifier (`chain_hashes_eq_ct`).
- **+0.3** — comprehensive integration test matrix: happy / cross-tenant-reject / empty-range / chain-tamper / unauthenticated / rate-limit / audit-failure-503 / streaming-no-Content-Length / abort-trailer-on-tamper / CLI-compat-round-trip = 10 integration tests covering every WI-S09-008 §6 AC.
- **+0.3** — INV references all valid against the canonical registry (`INV-OBS-AUDIT-CHAIN-INTEGRITY`, `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`, `INV-TENANT-ISOLATION`, `INV-AUDIT-APPEND-ONLY`). Regulatory citations (RFC 7230 §4.4, GDPR Art. 15, SOC 2 CC7.2) accurate.

**Adjusted score:** `6.65 + 1.7 = 8.35 / 10`.

## 5. Verdict

**CONDITIONAL (7.5 - 8.5)** — 8.35 / 10.

The wave-18 streaming + mid-stream-trailer wire is correctly shipped and the load-bearing invariants are preserved. The 5 P1 findings are real but bounded: 3 of them (A-P1-02, A-P1-03, A-P1-05) cluster around the same root cause — discarded audit-emit Results on non-happy paths — and one focused fix-stream can address all three plus the wave-15 carry-over (A-P2-01).

**Blocking before SEAL:** none (no P0).

**Recommended pre-GA fix stream descriptor:** `wt/r-prep-audit-export-fail-closed-emit-discipline` — addresses A-P1-02, A-P1-03, A-P1-05, A-P2-01 in a single audit-discipline pass. Estimated cost: ~150 LOC of route changes + 3 new proptest blocks.

**Recommended pre-GA fix stream descriptor (separate):** `wt/r-prep-audit-export-payload-column` — addresses A-P1-04 by extending `ExportAuditRow` with `payload_json: Option<String>` (additive, preserves serde back-compat). Estimated cost: ~50 LOC + schema migration note in the audit doc. **Already in flight** — see existing worktree `agent-export-payload-column` (branch `wt/r-prep-audit-export-payload-column`).

**Recommended deferred to wave-19+:** A-P1-01 (true async-generator streaming) — operational impact bounded by per-tenant rate-limit (1 export/60s); a 7-year TB-scale window is the long-tail case. Track as a follow-on WI rather than a GA blocker.

---

**Reviewer sign-off:** wave-19 builder (opus 4.7), 2026-05-16. CONDITIONAL — proceed to SEAL wave-18 ONLY if the orchestrator dispatches the fail-closed-emit-discipline fix stream before tagging GA. Otherwise the cross-tenant + mid-stream-break audit anchors silently miss on sink-pipeline-down paths — a regression in the security team's detect surface.

---

## Wave-20 closure note (2026-05-16)

**Status:** A-P1-02, A-P1-03, A-P1-05, A-P2-01 — **CLOSED**.

**Branch:** `wt/r-prep-audit-export-fail-closed-emit-discipline`.

**Helper:** `emit_or_503` in `apps/server/src/routes/audit_export.rs` (~line 270; defined immediately after the `ExportAuditSink` trait + `InMemoryExportAuditSink` impl block). Public surface — re-callable from any future audit-emit boundary in the route. Mirrors the wave-15 inline `is_err() → 503` discipline previously only on the `export_request.v1` arm.

**Sites updated (5 — 1 per audit finding + 1 wildcard variant):**

1. Cross-tenant-reject (A-P1-02) — `handle_export` cross-tenant arm now routes the `corelink.security.audit_export_cross_tenant_attempt.v1` emit through `emit_or_503`. On sink failure surfaces 503 + `"audit pipeline closed"` instead of dropping the row and shipping a 403 with no security anchor.
2. Rate-limit-deny (A-P2-01) — both the `Deny429` arm AND the `#[non_exhaustive]` wildcard arm now route the `corelink.audit.export_request.v1` rate-limited emit through `emit_or_503`. Closes the analytics-dashboard parity-assertion regression where a paired audit-sink-down + 429 burst would silently break `emit_count == 429_count`.
3. Verify-failed-sev0 (A-P1-05) — the SEV-0 verify-failed emit (when `verify_export_result` flags the chain BEFORE the streaming body starts) now routes through `emit_or_503`. On sink failure surfaces 503 instead of streaming the (still-tampered) body without the security-team page anchor.
4. Mid-stream chain-break (A-P1-03) — `emit_mid_stream_break_audit` lifted to return `Result<(), &'static str>`. The async stream generator inside `build_audit_export_async_stream` handles `Err` by force-closing the body WITHOUT emitting the `Frame::trailers` abort trailer. Documented trade-off (in the function doc-comment + the module-level "Wave-20 emit-discipline lift" doc-block): response headers are already flushed by the time the generator polls a row, so a 503 is structurally impossible; a truncated body + a `tracing::error!` SEV-0 line is louder than a silent missing audit anchor.
5. Same as (4) — the serialize-failure arm inside the generator (parallel structure to the verify-failure arm).

**Tests net-new (4):**

- `cross_tenant_reject_returns_503_on_audit_sink_failure` — drives the route end-to-end via `Router::oneshot` with an injected-failure sink + a tenant-mismatch query string; asserts 503.
- `rate_limit_deny_returns_503_on_audit_sink_failure` — drives two consecutive requests through the route; the first consumes the rate-limit token, the second hits `Deny429` after the sink failure is injected; asserts the second response is 503.
- `verify_failed_sev0_returns_503_on_audit_sink_failure` — drives the `emit_or_503` helper directly with a failing sink; asserts the returned `Response` carries 503. Pairs with the existing `chain_tamper_emits_verify_failed_sev0` integration test (which already pins the happy-emit end-to-end path).
- `mid_stream_break_surfaces_audit_failure_via_forced_close` — drives the async stream generator with a failing sink + a bogus anchor (forces verify-fail on row 0); asserts NO trailer frame is yielded and the snapshot is empty (force-close path traversed).

All 4 + the wave-19 proptest pass on Tokio multi-thread runtime in `cargo test -p corelink-server --lib audit_export` (27/27 lib tests green; wave-18 + wave-19 + wave-20 cumulative). All 12 integration tests in `cargo test -p corelink-server --test audit_export` preserved + green (the audit doc cites 11; wave-19 added one more — `streaming_response_yields_all_rows_across_multiple_pages` — for a current total of 12).

**Quality gates:**

- `cargo build --workspace` — pass.
- `cargo test -p corelink-server --lib audit_export` — 27/27 pass (lib unit + wave-19 proptest 10k iter + wave-20 net-new 4).
- `cargo test -p corelink-server --test audit_export` — 12/12 pass.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `scripts/validate_specs.py` + `scripts/validate_references.py` — both green.

**Invariant alignment:** `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` now mechanically enforced on every non-happy path in the audit-export route. The trade-off documented in (4)/(5) above is the ONE path where the invariant's strict "emit MUST succeed before mutation" cannot be honoured with a status-code response (the mutation is the wire byte-stream + the headers are already flushed); force-close + SEV-0 tracer is the documented escape hatch.

**Deferred (out of wave-20 scope):** A-P1-01 (true async-generator streaming) was closed independently by wave-19 (`build_audit_export_async_stream` replaces the wave-18 `Vec<Frame<Bytes>>` plan). A-P1-04 (payload-column lift) — closed independently by `wt/r-prep-audit-export-payload-column` (the wave-19 `ExportAuditRow::payload: Option<serde_json::Value>` field is live). A-P2-02 / A-P2-03 / A-P2-04 / A-P2-05 / A-P3-01 / A-P3-02 — out of wave-20 scope per the recommended fix-stream descriptor; tracked as follow-on grooming.

**Closer sign-off:** wave-20 builder (claude opus 4.7), 2026-05-16. SEALED — the 4 P1/P2 findings clustered around the discarded-`Result` root cause are mechanically closed; the security-team detect surface is restored on sink-pipeline-down paths.
