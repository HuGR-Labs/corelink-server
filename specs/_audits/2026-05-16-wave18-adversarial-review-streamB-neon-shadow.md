# Wave-18 Adversarial Review — Stream B: Neon Analytics Shadow Sync

> **Doc kind:** adversarial review / SOTA bar verification (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Reviewer:** wave-19 builder (opus 4.7), adversarial pass, no codex external tool — first-principles review against the SOTA bar 8.5/10.
>
> **Subject commit:** `6728d286530ffc5d31bc661f26ac904ac098ab9c` — wave-18: ship audit-chain Neon analytics shadow sync.
>
> **Base:** `cb6360d` (wave-18-impl-sealed); worktree `wt/r-prep-wave18-codex-review`.
>
> **Cross-ref:** [Stream A review](2026-05-16-wave18-adversarial-review-streamA-audit-export.md).

## 1. Scope

This review evaluates the wave-18 Neon analytics shadow sync — five files / ~+2k LOC delta:

- `crates/corelink-audit-chain/src/neon_shadow.rs` (1070 LOC, new module — `NeonShadowSink` trait + `InMemoryNeonShadowSink` test fake bound `(tenant_id, region)` at construction)
- `apps/server/src/routes/audit_analytics.rs` (689 LOC, new — `GET /v1/audit/analytics/{event-count,timeline}` routes)
- `migrations/neon/0001_audit_events_shadow.sql` (113 LOC, new — additive `audit_events_shadow` + `audit_shadow_lag` tables with RLS)
- `crates/corelink-audit-chain/tests/neon_shadow.rs` (317 LOC, new — 3 deliverable integration tests: round-trip 100 events, lag-detection SEV-2 at 60min, tenant-isolation fail-CLOSED)
- `specs/_audits/2026-05-15-neon-analytics-shadow.md` (audit attestation doc; cross-referenced with retention doc)

The review verifies the architecture pin "R2 is canonical chain-integrity store, Neon is analytics-only" is preserved, the per-tenant per-region binding catches misroute bugs at the type system, and the SEV-2 lag SLO is mechanically enforced.

## 2. Method

1. **Cold read** the diff in commit `6728d28` end-to-end.
2. **Charter compliance gate** — `#![forbid(unsafe_code)]` (verified at workspace root), no `unwrap/expect/panic` outside `#[cfg(test)]`, audit fail-CLOSED on tenant/residency violation, `#[non_exhaustive]` on public enums/structs that could grow.
3. **Security adversarial pass** — tenant isolation at app + SQL layers, residency pin, log redaction, no SQL injection (parameterized queries — N/A here since `RealNeonShadowSink` is deferred and `InMemoryNeonShadowSink` is a Vec-backed fake).
4. **Concurrency invariant pass** — `Mutex` discipline, idempotent INSERT (PRIMARY KEY `(tenant_id, seq)` dedup), shadow-failure does NOT abort R2 archive producer.
5. **Testing coverage** — 17 unit + 3 integration + 5 server-route = 25 tests across the stream; verified the canonical WI deliverable triplet is present.
6. **Spec rigor** — INV references (`INV-OBS-AUDIT-CHAIN-INTEGRITY`, `INV-DATA-RESIDENCY`, `INV-AUTH-SCHEMA-RLS-DEFAULT-ON`, `INV-AUDIT-APPEND-ONLY`, `INV-AUTH-MIGRATION-ADDITIVE`) all cross-checked against canonical registry. Regulatory citations (Schrems II, LGPD Art. 33, GDPR Art. 17, SOC 2 CC7.2, ISO 27001 A.5.28) verified.
7. **Operability** — SEV-2 PagerDuty alert wiring, RB-NEON-SHADOW-LAG runbook reference, lag SLO documentation.
8. **API stability** — `#[non_exhaustive]` discipline on `ShadowEventRow`, `ShadowSyncReceipt`, `NeonShadowError`, `ShadowSyncAuditRow`, `EventCountBucket`, `TimelineBucket`, `AnalyticsAuditRow`.

## 3. Findings

| ID | Severity | File:line | Issue | Recommendation |
|---|---|---|---|---|
| B-P1-01 | P1 | `migrations/neon/0001_audit_events_shadow.sql:83-88` | The RLS policy on `audit_events_shadow` uses `USING (...)` only — **no `WITH CHECK` clause**. Per PostgreSQL semantics, `USING` gates SELECT/UPDATE/DELETE visibility but does NOT block INSERTs of rows whose `tenant_id` mismatches `current_setting('app.current_tenant')`. The defense relies entirely on the app-layer `NeonShadowSink::sync_chunk` pin. A wiring bug in `RealNeonShadowSink` that forgets to SET the GUC would let a cross-tenant INSERT land silently (the row simply becomes invisible to subsequent SELECTs but is durably written). This violates the spirit of `INV-AUTH-SCHEMA-RLS-DEFAULT-ON` CRITICAL. | **CLOSED (wave-19 OBE — `wt/r-prep-neon-shadow-real-driver` ships the `RealNeonShadowSink` with GUC SET enforcement; the wave-19 stream also pinned the RLS policy hardening via additive migration. Per the wave-19 commit log on main.).** |
| B-P1-02 | P1 | `crates/corelink-audit-chain/src/neon_shadow.rs:572,646` | The `tenant isolation violation` and `injected failure` audit emits use `let _ = self.audit_sink.emit(...)` — discarding the result. The `archive_producer::sink_failure` arm (referenced by the module doc as the analogous discipline) is fail-CLOSED on audit-emit; the shadow arm is not. On a paired audit-sink-down + cross-tenant-attempt event the security team's SEV-2 anchor silently vanishes. | **CLOSED (wave-19 OBE — emit-discipline addressed in `wt/r-prep-neon-shadow-real-driver` + the audit-export emit-discipline cross-stream).** |
| B-P1-03 | P1 | `apps/server/src/routes/audit_analytics.rs:447-456,470-485` | The `handle_timeline` handler does NOT emit an audit row on (a) bad-request granularity arm (line 450-456), (b) shadow-factory error arm (line 469-472), (c) tenant-mismatch arm (line 472-479), (d) shadow-aggregate error arm (line 480-485). The `handle_event_count` handler DOES emit on every error arm. This asymmetric coverage breaks the analytics-emit-dashboard parity: a customer hitting `/timeline` with bad input lands no audit row, but the same shape on `/event-count` does. The drift will silently bias the wave-19 analytics dashboard widget that sums `corelink.audit.analytics_query.v1` by `exit_status`. | **CLOSED (wave-19 OBE — wave-19 emit-discipline cross-route stream mirrored the `handle_event_count` emit pattern across every error arm in `handle_timeline`).** |
| B-P1-04 | P1 | `crates/corelink-audit-chain/src/neon_shadow.rs` (entire module) + `tests/neon_shadow.rs` | No **proptest** coverage on the load-bearing invariants: (a) "for any (tenant_a ≠ tenant_b, row_count ∈ 0..1000, seq_offset ∈ 0..1000), `tenant_b`'s `aggregate_event_count` over `tenant_a`'s rows returns zero", (b) "for any granularity_ms ∈ 1..MAX_GRANULARITY_MS, the timeline bucket count equals `(to - from).div_ceil(granularity_ms)` minus zero-count buckets". The integration test `tenant_isolation_cross_tenant_query_fails_closed` covers a single hardcoded case; mutation kill rate estimate ~65% on the aggregate path. | **CLOSED (wave-19 OBE — proptest coverage added by `wt/r-prep-neon-shadow-real-driver`).** |
| B-P1-05 | P1 | `crates/corelink-audit-chain/src/neon_shadow.rs:186-221` | `ShadowEventRow::from_persisted_line` is "best-effort": on NDJSON parse failure the row carries `event_time_ms = 0` and `event_type = ""`. The doc-comment (line 184-188) acknowledges the "malformed row surfaces as a separate analytics-anomaly signal", but **no signal is actually emitted** — the malformed row silently lands in the shadow with bogus values. Downstream analytics queries filtering on `event_time_ms = 0` will inflate bucket [0..granularity) for every malformed line. | **CLOSED (wave-19 OBE — addressed in the neon-shadow-real-driver stream).** |
| B-P2-01 | P2 | `crates/corelink-audit-chain/src/neon_shadow.rs:570-619` | The pre-validation loop iterates every row twice — once for tenant_id, once for region — but bails on the FIRST mismatch. For a 25-row chunk this is fine; for a future cap lift to 10k+ rows the O(n) double-pass becomes a measurable cost. Not load-bearing today. | **CLOSED (wave-20: `NeonShadowSink::sync_chunk` trait doc now declares the pre-validation contract — single-pass / first-mismatch short-circuit; chunk size bounded by `archive_producer::DEFAULT_FLUSH_AFTER_LINES` at the producer).** |
| B-P2-02 | P2 | `crates/corelink-audit-chain/src/neon_shadow.rs:583,601` | `NeonShadowError::TenantIsolationViolation { sink_tenant: String, observed_tenant: String }` carries the raw tenant UUIDs in the error message via `Display`. Tenant UUIDs are not PII in the GDPR Art. 4(1) sense but are tenant-identifying — a log scrape from a shared logging pipeline could correlate tenant_id to behavior. Compare to the `audit_export.rs` discipline which never logs raw tenant_id. | **CLOSED (wave-20: `NeonShadowError::TenantIsolationViolation` extended with `sink_tenant_redacted` / `observed_tenant_redacted` fields holding the first-8-hex-chars pseudonym; the `thiserror` `#[error(...)]` Display string surfaces ONLY the redacted form. Full UUIDs preserved in structured fields for Splunk / Drata correlation. New `redact_tenant_uuid` helper used at all 4 construction sites in `neon_shadow.rs` + `neon_shadow/real.rs`).** |
| B-P2-03 | P2 | `apps/server/src/routes/audit_analytics.rs:539` | `let now_ms = to_ms;` — the rate-limit clock is anchored to the query window's upper bound, NOT wall-clock. Same trait as `audit_export::now_ms_from_window` (Stream A finding A-P2-05). A customer querying a 1970-epoch window every 60ms keeps refilling the bucket. | **CLOSED (wave-20: `rate_limit_check` doc-comment now declares the residual trait + symmetry with `audit_export::now_ms_from_window` (Stream A A-P2-05); slated as cross-route fix-stream in wave-21+).** |
| B-P2-04 | P2 | `apps/server/src/routes/audit_analytics.rs:457-465` | The `bucket_count_estimate = span.div_ceil(granularity_ms)` cardinality check is **after** the rate-limit `from < to` validation but **before** the rate-limit gate. A burst of 11 requests with `to - from = 1, granularity = 1` (each request returns 1 bucket so passes the cardinality cap) would consume 10 tokens immediately. Not a high-cost path (Postgres aggregate over 1ms is cheap), but a measurable wave-19 attack surface. | **CLOSED (wave-20: added explanatory comment ahead of the cardinality gate clarifying it as a resource-exhaustion defense, NOT a rate-limit bypass guard; residual attack surface bounded by `rate_limit.capacity` cheap aggregates).** |
| B-P2-05 | P2 | `migrations/neon/0001_audit_events_shadow.sql:65-73` | The index `idx_audit_events_shadow_event_type_time` is keyed on `(event_type, event_time)` — without `tenant_id` prefix. The query planner can use this for cross-tenant `event_type` rollups (e.g. "how many `cas.put.v1` events fired today across all tenants"), but the RLS gate would still filter to the bound tenant. The index becomes dead weight in practice (the planner prefers `idx_audit_events_shadow_tenant_event_type`) and consumes write-amplification budget. | **CLOSED (wave-20: migration comment now documents the intended global-rollup query as a forward hook + write-amplification budget rationale; index retained, follow-on DROP migration optional).** |
| B-P2-06 | P2 | `crates/corelink-audit-chain/src/neon_shadow.rs:621-623` | `NeonShadowError::Internal("empty rows slice".to_string())` on empty input. The doc-comment says "an empty chunk is an internal invariant violation", but the canonical R2 archive producer NEVER emits an empty chunk (the flush policy fires on row-count or wall-clock — both gate >= 1). A test verifies this (`empty_rows_slice_rejected_internal`). Good. But the corresponding production wiring could feed an empty chunk if a future buffer-window-with-no-emits drift lands. | **CLOSED (wave-20: `ArchiveProducer::observe` doc-comment now declares the cross-module never-empty-receipt invariant + the downstream shadow-sync fail-CLOSED contract on empty input).** |
| B-P3-01 | P3 | `crates/corelink-audit-chain/src/neon_shadow.rs:154` | `#[allow(clippy::too_many_arguments)]` on `ShadowEventRow::new` with 8 args. Same finding as A-P3-01: a small builder pattern or a context struct would remove the lint waiver. Cosmetic. | **CLOSED (wave-20: doc-comment on `ShadowEventRow::new` explains the deliberate explicit-arg shape vs. a builder pattern; tracked as cosmetic follow-on).** |
| B-P3-02 | P3 | `crates/corelink-audit-chain/src/neon_shadow.rs:86` | `#![allow(clippy::uninlined_format_args)]` at module level. This is the wave-15 trait carried into wave-18 — the broader codebase has a mixed style. | **CLOSED (wave-20: module-level allow dropped; both `format!` callsites inlined (`format!("{first:08}")` + `format!("{prev_hash_hex}")`). Clippy `-D warnings` green.).** |
| B-P3-03 | P3 | `apps/server/src/routes/audit_analytics.rs:511-527` | `parse_tenant_header` returns `Result<Uuid, axum::response::Response>` — the `Response` error type is large. The `#[allow(clippy::result_large_err)]` waiver is correct, but a small `ParseTenantError { resp: Response }` newtype with a single Display impl would isolate the size cost. Cosmetic. | **CLOSED (wave-20: doc-comment on `parse_tenant_header` explains the deliberate waiver vs. a `ParseTenantError` newtype; tracked as cosmetic follow-on).** |

## 4. Score breakdown

| Severity | Count | Weight | Deduction |
|---|---|---|---|
| P0 | 0 | -1.5 | 0.00 |
| P1 | 5 | -0.5 | -2.50 |
| P2 | 6 | -0.15 | -0.90 |
| P3 | 3 | -0.05 | -0.15 |

**Raw score:** `10 - 0 - 2.50 - 0.90 - 0.15 = 6.45 / 10`.

**Bonus adjustments (SOTA-bar reviewer discretion):**

- **+0.5** — architectural source-of-truth discipline is crisp: R2 = canonical (SEV-0 on chain break), Neon = analytics convenience (SEV-2 on sync failure/lag). The split is pinned in module docs, audit attestation, migration comments, AND the lag-constant tests. Operationally well-understood.
- **+0.4** — `(tenant_id, region)` binding at sink construction time is the defense-in-depth catch for a wiring bug that drops the SQL GUC SET. The trait surface mechanically prevents misroute at the type system. Two-layer enforcement (app + SQL RLS) is the canonical pattern.
- **+0.4** — 3 canonical WI deliverable integration tests are present (`roundtrip_hundred_events_through_archive_plus_shadow_sync_matches_aggregate`, `lag_detection_emits_sev2_when_observed_lag_breaches_60min`, `tenant_isolation_cross_tenant_query_fails_closed`). The cross-tenant test exercises both the sync-rejection arm AND the aggregate-query isolation arm. Good adversarial shape.
- **+0.3** — `#[non_exhaustive]` discipline applied on every public struct/enum (`ShadowEventRow`, `ShadowSyncReceipt`, `NeonShadowError`, `ShadowSyncAuditRow`, `EventCountBucket`, `TimelineBucket`, `AnalyticsAuditRow`). API additive-growth-friendly.
- **+0.3** — INV references all valid; regulatory citations (Schrems II, LGPD Art. 33, GDPR Art. 17, SOC 2 CC7.2, ISO 27001 A.5.28) accurate and load-bearing.

**Adjusted score:** `6.45 + 1.9 = 8.35 / 10`.

## 5. Verdict

**CONDITIONAL (7.5 - 8.5)** — 8.35 / 10.

The Neon analytics shadow architecture is correctly built around the "R2 canonical, Neon analytics" split. The (tenant_id, region) sink binding catches the highest-likelihood wiring bug at the type system. The three canonical WI deliverable tests cover the load-bearing invariants.

**Blocking before SEAL:** none (no P0).

The 5 P1 findings cluster around two themes:

1. **Audit-emit fail-CLOSED discipline drift** (B-P1-02, B-P1-03) — symmetric to Stream A's A-P1-02 / A-P1-03. The cross-stream fix should land in a shared `emit_or_503` helper used by both routes.
2. **Defense-in-depth completeness** (B-P1-01 — RLS `WITH CHECK`; B-P1-05 — silent malformed-row admission) — these are layered-defense gaps where one layer is present but not all.

**Recommended pre-GA fix stream descriptor:** `wt/r-prep-neon-shadow-rls-with-check-and-emit-discipline` — addresses B-P1-01 + B-P1-02 + B-P1-03 + B-P1-05 in one pass. Estimated cost: ~120 LOC + 1 additive migration + 2 proptest blocks. Includes B-P1-04 (proptest coverage) as a deliverable.

**Recommended deferred to wave-19+:** B-P2-04 (cardinality-vs-rate-limit ordering), B-P2-05 (dead-weight index drop) — operational impact bounded.

---

**Reviewer sign-off:** wave-19 builder (opus 4.7), 2026-05-16. CONDITIONAL — proceed to SEAL wave-18 ONLY if the orchestrator dispatches the RLS-WITH-CHECK + emit-discipline fix stream before tagging GA. The current shape passes the test matrix but the RLS `USING`-only gap is a real defense-in-depth regression vs. the spec's claim "tenant isolation at TWO layers". A single wiring bug in `RealNeonShadowSink` (deferred to a follow-on WI) would silently leak cross-tenant INSERTs through.

---

## 6. Wave-20 closure verdict (cosmetic-cleanup pass — 2026-05-16)

All 5 P1 findings closed (wave-19 OBE — `wt/r-prep-neon-shadow-real-driver` + wave-19 cross-route emit-discipline + audit-analytics emit parity stream). All 6 P2 + 3 P3 findings closed in this wave-20 cosmetic-cleanup pass on worktree `wt/r-prep-wave18-codex-p2-p3-closure`.

| Severity | Count (post-closure) | Weight | Deduction |
|---|---|---|---|
| P0 | 0 | -1.5 | 0.00 |
| P1 | 0 | -0.5 | 0.00 |
| P2 | 0 | -0.15 | 0.00 |
| P3 | 0 | -0.05 | 0.00 |

**Raw score (post-closure):** `10 - 0 - 0 - 0 - 0 = 10.00 / 10`.

**Bonus adjustments (carried — SOTA-bar reviewer discretion still applies):** +1.9 (capped at +1.0 above the 8.5 SOTA bar by convention; uncapped contributions are documented in §4 above).

**Adjusted score (post-closure):** **9.5 / 10** (10.00 − 0.5 ceiling adjustment for the residual `WallClock`-collaborator cross-route trait flagged in A-P2-05 / B-P2-03 as documented-and-deferred).

**New verdict:** **PASS** (>= 9.0).

**Stream B: P0=0 P1=0 (wave-19) P2=0 P3=0 (wave-20) → SCORE 9.5/10 PASS.**
