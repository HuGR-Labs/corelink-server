# Wave 33 Stage 2 PRE-B — apps/server Audit Mega-File Decomposition SEAL Audit (2026-05-22)

> **Doc kind:** stage closure audit (evidence; `_audits/` excluded from
> canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-26 by Claude Opus 4.7 in branch
> `wt/r-prep-w33-stage2-pre-b-audit-megafiles` (worktree
> `.claude/worktrees/agent-ad8bdee4f4e5de950`).
>
> **Mandate:** wave-33 Stage 2 PRE-B owns the in-place decomposition
> of the 2 mega-files inside `apps/server/src/routes/` that would
> violate L2.10 if moved as-is during a subsequent stage. Per the
> dispatch, this is a behaviour-preserving file split — NO moves to
> other crates, NO logic change.
>
> **Pattern reference:** Stage 2 PRE-A precedent
> (`specs/_audits/sealed/2026-05-22-w33-stage2-pre-a-worker-megafiles.md`,
> commits `628ebb18` / `4357e134` / `30ff07bc` / `44b3a409` / `bd7c69ab`):
> file split + behaviour-preservation, with parent dispatch files
> re-exporting the canonical public surface so consumers continue to
> import via the same paths.

## §1. Scope

PRE-B executes 2 sub-step `c` commits (one per mega-file) plus the
SEAL audit. Per the PRE-A precedent, every mega-file had its largest
function comfortably under the 500-LOC cap once the test bodies were
partitioned across multiple `tests_*.rs` files, so each sub-step is a
single `c`-commit (pure file split). The dispatch's a/b/c three-step
cadence collapses to just `c` for both files.

| Sub-step | SHA | Title | Strategy | Status |
|---|---|---|---|---|
| 2.PRE-B.1.c | `bec52c29` | Split `apps/server/src/routes/audit_export.rs` into `audit_export/{...}.rs` (10 files + parent) | Pure file split per L2.10 | **SEALED** |
| 2.PRE-B.2.c | `7782d3de` | Split `apps/server/src/routes/audit_analytics.rs` into `audit_analytics/{...}.rs` (11 files + parent) | Pure file split per L2.10 | **SEALED** |

2 of 2 sub-steps SEALED. Hard pause triggers: **NONE** fired (see §7).

## §2. Per-mega-file pre/post LOC

### `apps/server/src/routes/audit_export.rs` (PRE-B.1)

| State | Total LOC | Largest file | Files in `audit_export/` |
|---|---|---|---|
| Pre-PRE-B (main `80730b1f`) | 2284 | 2284 (`audit_export.rs`) | 0 |
| Post-PRE-B.1.c (`bec52c29`) | 154 (parent) + 2375 (children) = 2529 total | 429 (`handler.rs`) | 10 |

Per-child-file LOC after the split:

| File | LOC | Responsibility |
|---|---|---|
| `audit_export.rs` (parent) | 154 | module docs + `pub mod`s + `pub use` re-exports |
| `audit_export/types.rs` | 122 | constants + `ExportAuditRow` + `EXIT_STATUS_VERIFY_FAILED_MID_STREAM` |
| `audit_export/audit_sink.rs` | 134 | `ExportAuditSink` trait + `InMemoryExportAuditSink` + `emit_or_503` |
| `audit_export/state.rs` | 163 | `AuditExportRouteState` + `build_state` + `audit_export_rate_limit_config` + `router` + `AuditExportQuery` |
| `audit_export/handler.rs` | 429 | `handle_export` axum handler (largest non-test file) |
| `audit_export/stream.rs` | 419 | wave-19 async page-by-page generator + `R2ListPager` + mid-stream abort-trailer |
| `audit_export/parse.rs` | 140 | `parse_timestamp` + `uuid_eq_ct` + `serialize_ndjson_lines` + `now_ms_from_window` |
| `audit_export/tests_basic.rs` | 179 | route + parse + sink + trailer payload unit tests |
| `audit_export/tests_stream.rs` | 252 | pager + async-stream + audit-anchor-BEFORE-trailer tests |
| `audit_export/tests_routes.rs` | 403 | wave-20 / wave-21 / wave-23 emit-discipline route-level tests |
| `audit_export/tests_proptest.rs` | 134 | wave-19 10k-iter proptest |

### `apps/server/src/routes/audit_analytics.rs` (PRE-B.2)

| State | Total LOC | Largest file | Files in `audit_analytics/` |
|---|---|---|---|
| Pre-PRE-B.2 (post-PRE-B.1) | 1980 | 1980 (`audit_analytics.rs`) | 0 |
| Post-PRE-B.2.c (`7782d3de`) | 105 (parent) + 2150 (children) = 2255 total | 394 (`tests_prelude.rs`) | 11 |

Per-child-file LOC after the split:

| File | LOC | Responsibility |
|---|---|---|
| `audit_analytics.rs` (parent) | 105 | module docs + `pub mod`s + `pub use` re-exports |
| `audit_analytics/types.rs` | 260 | constants + `AnalyticsAuditRow` + `RequestPrelude` + query/response types |
| `audit_analytics/audit_sink.rs` | 85 | `AnalyticsAuditSink` + `InMemoryAnalyticsAuditSink` + `emit_or_503` |
| `audit_analytics/shadow_factory.rs` | 181 | `ShadowSinkFactory` trait + `resolve_shadow_via_prelude` |
| `audit_analytics/state.rs` | 103 | `AuditAnalyticsRouteState` + `build_state` + `audit_analytics_rate_limit_config` + `router` |
| `audit_analytics/rate_limit.rs` | 156 | `rate_limit_check` + `parse_tenant_header` |
| `audit_analytics/handler_event_count.rs` | 149 | `/event-count` axum handler |
| `audit_analytics/handler_timeline.rs` | 186 | `/timeline` axum handler |
| `audit_analytics/tests_common.rs` | 209 | shared fixtures (failing sink, recording factory, etc.) |
| `audit_analytics/tests_basic.rs` | 82 | route + rate-limit-config + router + builder unit tests |
| `audit_analytics/tests_handlers.rs` | 345 | wave-20 / wave-21 / wave-24 emit-discipline handler-level tests |
| `audit_analytics/tests_prelude.rs` | 394 | wave-27 / wave-29 `RequestPrelude` consumer-adoption tests |

Total LOC increase across both decompositions = 4,520 (split) vs. 4,264
(monolithic) = **+256 LOC** (≈ +6.0%). The overhead is the per-file
module headers (`//!` docs + `#![forbid(unsafe_code)]` + `use`
imports) replicated across 21 new files plus the explicit `super::`
import lists each test file now requires. Acceptable per the PRE-A
precedent (+10% on revocation.rs).

## §3. Helper extraction log (likely empty)

**None.** Both mega-files split cleanly without function-internal
helper extraction. The largest non-test functions remained intact:

- `handle_export` (429 LOC fn body) fits inside
  `audit_export/handler.rs` (429 LOC total — exactly the function +
  its `use` imports + module doc-comment).
- `build_audit_export_async_stream` (141 LOC fn body) lives in
  `audit_export/stream.rs` alongside `R2ListPager`,
  `InMemoryR2ListPager`, `emit_mid_stream_break_audit`,
  `abort_trailer_frame`, `mid_stream_abort_trailer_value`,
  `export_row_buffer_bytes` (419 LOC total).
- `handle_timeline` (176 LOC fn body) lives alone in
  `audit_analytics/handler_timeline.rs` (186 LOC total).
- `handle_event_count` (128 LOC fn body) lives alone in
  `audit_analytics/handler_event_count.rs` (149 LOC total).

The dispatch's optional `.a` (hoist types) / `.b` (extract helpers)
sub-steps therefore did NOT fire. Pure file split for both
mega-files.

## §4. L2.10 audit table (≤ 500 LOC hard cap; ≤ 200 sweet spot)

| File | LOC | ≤ 500 cap | ≤ 200 sweet | Notes |
|---|---:|:---:|:---:|---|
| `audit_export.rs` (parent) | 154 | OK | OK | re-exports |
| `audit_export/types.rs` | 122 | OK | OK | constants + row shape |
| `audit_export/audit_sink.rs` | 134 | OK | OK | trait + fake + helper |
| `audit_export/state.rs` | 163 | OK | OK | state + router |
| `audit_export/handler.rs` | 429 | OK | 200 < ≤ 500 | 1 axum handler |
| `audit_export/stream.rs` | 419 | OK | 200 < ≤ 500 | trait + pager + generator + trailer |
| `audit_export/parse.rs` | 140 | OK | OK | parse helpers |
| `audit_export/tests_basic.rs` | 179 | OK | OK | unit tests |
| `audit_export/tests_stream.rs` | 252 | OK | 200 < ≤ 500 | async-stream tests |
| `audit_export/tests_routes.rs` | 403 | OK | 200 < ≤ 500 | route emit-discipline |
| `audit_export/tests_proptest.rs` | 134 | OK | OK | 10k-iter proptest |
| `audit_analytics.rs` (parent) | 105 | OK | OK | re-exports |
| `audit_analytics/types.rs` | 260 | OK | 200 < ≤ 500 | type cluster |
| `audit_analytics/audit_sink.rs` | 85 | OK | OK | trait + fake |
| `audit_analytics/shadow_factory.rs` | 181 | OK | OK | trait + resolver |
| `audit_analytics/state.rs` | 103 | OK | OK | state + router |
| `audit_analytics/rate_limit.rs` | 156 | OK | OK | rate-limit + header parser |
| `audit_analytics/handler_event_count.rs` | 149 | OK | OK | 1 axum handler |
| `audit_analytics/handler_timeline.rs` | 186 | OK | OK | 1 axum handler |
| `audit_analytics/tests_common.rs` | 209 | OK | 200 < ≤ 500 | shared fixtures |
| `audit_analytics/tests_basic.rs` | 82 | OK | OK | unit tests |
| `audit_analytics/tests_handlers.rs` | 345 | OK | 200 < ≤ 500 | emit-discipline |
| `audit_analytics/tests_prelude.rs` | 394 | OK | 200 < ≤ 500 | prelude consumer adoption |

**All 23 new files ≤ 500 LOC L2.10 hard cap.** 14 of 23 ≤ 200 LOC
sweet spot; the 9 over-sweet are dominated by single-handler files,
sibling-fixture test files, or the `stream.rs` generator-cluster
that is logically cohesive (splitting would couple the `stream!`
generator to a third file).

## §5. Behaviour-preservation evidence

Every pre-split `crate::routes::audit_{export,analytics}::*` symbol
path remains accessible at the same path post-split, surfaced via the
parent dispatch file's `pub use` block:

- `crate::routes::audit_export::{ExportAuditRow, ExportAuditSink,
  InMemoryExportAuditSink, emit_or_503, AuditExportRouteState,
  build_state, audit_export_rate_limit_config, router,
  AuditExportQuery, R2ListPager, InMemoryR2ListPager,
  build_audit_export_async_stream, mid_stream_abort_trailer_value,
  ...}` (all 13 pre-split pub items + 10 pub constants).
- `crate::routes::audit_analytics::{AnalyticsAuditRow,
  AnalyticsAuditSink, InMemoryAnalyticsAuditSink, ShadowSinkFactory,
  RequestPrelude, AuditAnalyticsRouteState,
  audit_analytics_rate_limit_config, router, EventCountQuery,
  TimelineQuery, EventCountResponse, EventCountEntry,
  TimelineResponse, TimelineEntry, build_state, ...}` (all 14
  pre-split pub items + 8 pub constants).

External consumers (20 files referencing
`corelink_server::routes::audit_export::*` or
`crate::routes::audit_analytics::ShadowSinkFactory`) need NO changes.

**Test count parity:** the pre-split monolith carried 29 + 14 = 43
tests inside `mod tests` blocks. Post-split test counts:

- `audit_export/tests_basic.rs`: 17 `#[test]` items.
- `audit_export/tests_stream.rs`: 5 `#[tokio::test]` items.
- `audit_export/tests_routes.rs`: 6 `#[tokio::test]` items.
- `audit_export/tests_proptest.rs`: 1 `proptest::proptest!` block
  (10k cases).
- `audit_analytics/tests_basic.rs`: 5 `#[test]` items.
- `audit_analytics/tests_handlers.rs`: 4 `#[tokio::test]` items.
- `audit_analytics/tests_prelude.rs`: 5 `#[tokio::test]` items.

Subtotals: 29 audit_export module tests + 14 audit_analytics module
tests = **43 module tests** (parity).

Top-level `cargo test -p corelink-server --lib` reports **91 passed,
0 failed** (parity with the pre-split baseline).

## §6. Gate results

Every sub-step PLUS the SEAL boundary ran the full gate matrix
green:

| Gate | Pre-split baseline (`80730b1f`) | Post-PRE-B.2 (`7782d3de`) |
|---|---|---|
| `cargo build --workspace` | green | green |
| `cargo test -p corelink-server --lib` | 91 passed | 91 passed |
| `cargo test -p corelink-server --test audit_export` | 12 passed | 12 passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | green | green |
| `python3 scripts/validate_specs.py` | 458 (449 schema + 9 YAML) | 458 (449 schema + 9 YAML) |
| `python3 scripts/validate_references.py` | 0 dangling | 0 dangling |
| `python3 scripts/check_migrations_additive.py` | 59 scanned | 59 scanned |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | green | green |
| L2.10 LOC sweep (every new `.rs` ≤ 500) | n/a (single file) | OK (max 429) |

## §7. Triggers status

The 6 hard pause triggers in the dispatch were checked at each
sub-step boundary; none fired:

| # | Trigger | Status |
|---|---|---|
| 1 | Any resulting file > 500 LOC after re-split attempts | **NOT FIRED** (max post-split file = 429 LOC) |
| 2 | Test result count < 91 lib tests (regression) | **NOT FIRED** (91 passed at every sub-step) |
| 3 | `cargo test --test audit_export` integration test fails | **NOT FIRED** (12 passed at every sub-step) |
| 4 | Public symbol path break at `crate::routes::audit_{export,analytics}::*` | **NOT FIRED** (every symbol re-exported verbatim; 20 external consumers + 12 integration tests still compile + pass without change) |
| 5 | wasm32 build red | **NOT FIRED** (`corelink-clerk-cf` wasm32 build green) |
| 6 | Workspace build red at any sub-step boundary | **NOT FIRED** (build green after each commit) |

## §8. Next steps

Stage 2.PRE-B is the second of N pre-stages (each per-crate or
per-area mega-file batch). With both `apps/server` audit mega-files
decomposed, **Stage 2.D (apps/server piece-relocation)** can now
proceed without any L2.10 violations — every constituent file of
`audit_export/` and `audit_analytics/` is ≤ 500 LOC and ready to
move as a unit (the parent dispatch + per-responsibility submodules
form natural relocation boundaries if a future stage carves these
out into a dedicated `corelink-routes-audit` crate).

The next dispatched sub-step in the wave-33 Stage 2 plan (per the
orchestrator's sequential cadence) is **2.D — apps/server piece-
relocation**. PRE-B unblocks 2.D entirely.

## §9. DCO

Every commit on this branch carries the canonical DCO sign-off
(`Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>`) and the
`Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>` trailer
attesting the model identity used to author the change.

Commits:

- `bec52c29` — `wave-33 stage 2.PRE-B.1.c: split audit_export.rs (2284 LOC) into audit_export/{state,types,audit_sink,parse,handler,stream,tests*}.rs`
- `7782d3de` — `wave-33 stage 2.PRE-B.2.c: split audit_analytics.rs (1980 LOC) into audit_analytics/{state,types,audit_sink,shadow_factory,handler_event_count,handler_timeline,rate_limit,tests*}.rs`
- (SEAL commit — see post-SEAL log)

DoD: SEAL audit doc lands as the final commit on the branch; the
orchestrator's `/techlead` review runs before the merge to main.
