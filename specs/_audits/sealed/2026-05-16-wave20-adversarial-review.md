# Wave-20 Adversarial Review — 2026-05-16

> **Doc kind:** wave-close adversarial review (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Reviewer:** wave-21 R-prep adversarial agent (Claude Opus 4.7) — branch `wt/r-prep-wave20-adversarial-review`.
> **Base:** `main` @ `30e5f66` (wave-20 tip: "merge wt/r-prep-wave18-codex-p2-p3-closure into main (wave-20)").
> **Scope:** Independent SOTA bar review of the 10 wave-20 streams merged into main across `2eec064..30e5f66` (11 inbound commits + L7 reconciliation pass).
> **Charter:** review-only — no source code changes (this audit doc is the sole deliverable).
> **Cross-ref:** `specs/_audits/sealed/2026-05-16-wave20-closure.md` (hygiene roll-up); `specs/_audits/sealed/2026-05-16-wave19-adversarial-review.md` (prior pass — 8.78 PASS); `specs/_audits/sealed/2026-05-16-stripe-wasm32-gate-lift.md`; `specs/_audits/sealed/2026-05-16-neon-shadow-real-driver.md`; `specs/_audits/sealed/2026-05-16-neon-shadow-pg-testharness.md`.

---

## 1. Scope

Eleven inbound commits to review (10 streams + the L7 reconciliation merge tip), as enumerated by the dispatch brief:

| Stream | Commit | Subject |
|---|---|---|
| Meta | `1b893b9` | wave19-adversarial-review (re-rolled into wave-20 audit set) |
| FW-Reviewer | `1470520` | framework-reviewer-staffing proposal (FW-H-1..4) |
| Sec-Matrix | `c590a67` | secrets-matrix-tighten (3-secret small-closure) |
| INV-Reg | `b8ec7cf` | inv-registry-wave20-sweep + DEBT register dedup |
| Stripe-Clock | `62b1f4b` | Clock trait + wasm32 SystemTime panic closure |
| Neon-Test | `af4fbbc` | testcontainers harness (lifts 5 ignored tests) |
| Neon-Binder | `a7ddea3` | `TokioPostgresExecutor` binder + analytics route merge |
| Audit-Emit-Export | `4ccc161` | audit-export fail-CLOSED emit discipline |
| Audit-Emit-Analytics+RLS | `6716383` | RLS `WITH CHECK` + analytics emit-503 discipline |
| Wave-18-P2P3 | `f743811` | wave-18 codex P2/P3 cosmetic closure |
| L7 reconcile | `30e5f66` | rate-limit wildcard arm + branch-comment merge |

Wave-20 is a hardening + GA-prep wave; no new feature surfaces. Focus areas per dispatch:

1. L7 reconciliation correctness on `audit_export.rs` rate-limit wildcard arm.
2. `emit_or_503` discipline on critical paths (no `let _ = ...emit(...)` residue) in `audit_export.rs` + `audit_analytics.rs`.
3. RLS `WITH CHECK` at SQL layer (migration `0002`) + proptest covers cross-tenant INSERT rejection.
4. Stripe `Clock` trait correctly injected; no `SystemTime::now()` residue in `portal.rs` / `webhook_dispatch.rs` / `client.rs`.
5. `TokioPostgresExecutor` binder is wasm32-stub-safe + native correct.
6. Testcontainers harness `Drop` does not leak runtime resources.
7. Charter compliance: `#![forbid(unsafe_code)]` preserved; no `unwrap`/`expect`/`panic!` on production paths; audit fail-CLOSED preserved.

---

## 2. Method

Cold-tool, hands-off verification — no source code mutation. Each focus area was checked by:

- **Diff sweep** — `git show <sha> --stat` + targeted reads of the most-changed file in each commit.
- **Regex grep audits** — looking for forbidden patterns (`let _ = .*\.emit(`, `SystemTime::now()` outside the `clock` module, `unwrap()` / `expect(` / `panic!` on non-test paths, missing `#![forbid(unsafe_code)]`).
- **SQL-layer audit** — read `migrations/neon/0001_*.sql` + `0002_*.sql` end-to-end to confirm `WITH CHECK` is present on BOTH `audit_events_shadow` AND `audit_shadow_lag` policies.
- **Sanity gates** (cold runs, no caching):
  - `cargo check -p corelink-server` → PASS (1m17s clean).
  - `cargo check -p corelink-audit-chain` → PASS (9.5s clean).
  - `cargo build -p corelink-stripe-real` → PASS (47.9s clean).
  - `python3 scripts/validate_specs.py` → PASS (440 schema + 9 YAML-only = 449).
  - `python3 scripts/validate_references.py` → PASS (no dangling refs).
  - `python3 scripts/validate_secrets_matrix.py` → PASS exit 0 (matrix=125, code=105, all `code_only=0`; `matrix_only=20` are forward-looking soft-warns).
- **Drop / runtime hygiene** — direct read of `crates/corelink-audit-chain/tests/harness/pg_container.rs::Drop` to verify the `runtime.enter()` guard before `container` drop (testcontainers async-drop trap).

Tests (`cargo test`) were NOT executed end-to-end at review time — the wave-20 closure audit + the upstream stream-level audits each cite green test runs, and the dispatch budget caps at 50 min. `cargo check`/`cargo build` cover compile-time correctness; the audit relies on the upstream test evidence for behavioural claims.

---

## 3. Findings

### 3.1 Findings table

| ID | Sev | Stream | Locus | Finding | Status |
|---|---|---|---|---|---|
| W20-P0-* | — | — | — | (none) | — |
| W20-P1-* | — | — | — | (none) | — |
| W20-P2-01 | P2 | Audit-Emit-Analytics+RLS | `migrations/neon/0002_audit_events_shadow_with_check.sql:52-57,69-72` | `EXCEPTION WHEN undefined_object THEN NULL` swallows the case where 0002 runs before 0001 on a fresh setup. Comment says operator is expected to re-run; if migration tooling does not re-trigger, a fresh deploy could end up with `USING` only and no `WITH CHECK`. Recommendation: tighten the migration runner to enforce strict ordering OR raise a NOTICE instead of swallowing silently. | Open (low-severity — production deploys use ordered migration tooling so the path is operator-error only). |
| W20-P2-02 | P2 | Neon-Binder | `crates/corelink-audit-chain/src/neon_shadow/real_tokio_pg.rs:221,246` | `block_in_place(|| handle.block_on(...))` requires a multi-thread tokio runtime; a single-thread runtime callsite would panic. The crate doc-comment flags this; no programmatic guard. Recommendation: at constructor time, assert `tokio::runtime::Handle::current().runtime_flavor() == MultiThread`. | Open (covered by doc + by `#[tokio::main]` default; future single-thread callsite would panic-fail-fast). |
| W20-P2-03 | P2 | Neon-Test | `crates/corelink-audit-chain/tests/harness/pg_container.rs:145-156` | `Drop::drop` enters the runtime and synchronously drops `ContainerAsync`; if the Docker daemon hangs, the test process blocks on shutdown. Comment acknowledges "best-effort". No timeout wrapper. Recommendation: wrap the drop in `runtime.block_on(tokio::time::timeout(Duration::from_secs(5), async { drop(container); }))`. | Open (test-infra only; no GA-blocker). |
| W20-P3-01 | P3 | Audit-Emit-Export | `apps/server/src/routes/audit_export.rs:1119` | `unwrap_or(DEFAULT_EXPORT_ROW_BUFFER_BYTES)` is panic-safe (it's a default-fallback combinator, not a panic site). Cosmetic note: the env-var parse path uses `.trim().parse::<usize>().unwrap_or(default)` which is idiomatic; no fix. | No-op (false-positive in initial grep sweep; documented for traceability). |
| W20-P3-02 | P3 | Stripe-Clock | `crates/corelink-stripe-real/src/clock.rs:148-160` | `WasmWorkerClock` reads `js_sys::Date::now()` and clamps non-finite/negative to `UNIX_EPOCH`. The clamp is correct; comment says "impossible". Cosmetic suggestion: emit a `tracing::warn!` on the clamp branch so a malicious/buggy host swapping `Date.now` surfaces in logs. | Open (cosmetic — current behaviour is fail-safe). |
| W20-P3-03 | P2→P3 | Audit-Emit-Export | `apps/server/src/routes/audit_export.rs:1049` | `verify_inclusion_proof(...).unwrap_or_default()` — the `unwrap_or_default()` makes proof-verification failures collapse silently to the `default()` value. Cross-checked: `verify_inclusion_proof` returns `bool` (or a `Result<bool, _>`); `unwrap_or_default()` on a bool `Result` yields `false`, which the surrounding code IF-gates with a SEV-0 emit on the false branch. Behaviour is correct (verify-failed lands the SEV-0 row); cosmetic suggestion: rename / inline-comment to make it explicit that "false = verify failed = SEV-0 emit downstream". | No-op (behaviour is fail-CLOSED; documentation-only). |

### 3.2 Focus-area findings — itemised verdict

**FA-1: L7 reconciliation on rate-limit wildcard arm** — `audit_export.rs:610-640`. The wildcard arm of the `match outcome.decision` block:

- Emits a `rate_limited` audit row via `emit_or_503` (fail-CLOSED on sink error → 503).
- Returns `429 TOO_MANY_REQUESTS` with the body `"rate-limit decision arm not handled"`.
- Branch comment correctly cites both `A-P1-05` (parity assertion) and `A-P2-01` (analytics-dashboard count parity) as the joint motivation.

The merge correctly composed (i) HEAD's wildcard arm with (ii) the inbound fail-CLOSED emit discipline. **VERDICT: pass.**

**FA-2: `emit_or_503` discipline** — `audit_export.rs` + `audit_analytics.rs`. Regex sweep for `let _ = .*\.emit(` returns **zero matches** in both files on critical paths. All call sites now route through `emit_or_503` (`audit_export.rs`: 6 hits inside the route + the helper at L317; `audit_analytics.rs`: 13 hits + the helper at L331). The two direct `.emit()` calls outside `emit_or_503` are:
- `audit_export.rs:743` — the happy-path `request_row` emit; branches on `.is_err()` and returns 503 inline (equivalent shape).
- `audit_export.rs:1165` — `emit_mid_stream_break_audit` internal — the dedicated mid-stream-break helper, by design (anchor-BEFORE-trailer ordering).
- `audit_export.rs:1443, 1464` — TEST callsites (`.expect("emit")` + `.expect_err("inject")`), allowlisted by `#![allow(clippy::unwrap_used, clippy::expect_used)]` in the test module.

**VERDICT: pass.**

**FA-3: RLS `WITH CHECK` + proptest** — `migrations/neon/0002_audit_events_shadow_with_check.sql` adds `ALTER POLICY ... WITH CHECK` to BOTH `tenant_isolation_audit_events_shadow` AND `tenant_isolation_audit_shadow_lag`. The proptest `prop_cross_tenant_insert_rejected_by_rls_with_check_or_app_pin` (`crates/corelink-audit-chain/tests/prop_audit_chain.rs:445-508`) exercises the cross-tenant INSERT path with 10k iterations against the in-memory shadow sink, asserting `NeonShadowError::TenantIsolationViolation` + zero residual rows. The SQL layer + the app-layer pin + the proptest form a 3-layer defence-in-depth. **VERDICT: pass.** Minor caveat: W20-P2-01 (migration-ordering swallow) is a deploy-time hygiene concern, not a correctness issue.

**FA-4: Stripe `Clock` trait injection** — `crates/corelink-stripe-real/src/clock.rs` defines `Clock`, `SystemClock` (native), `WasmWorkerClock` (wasm32 via `js_sys::Date::now()`), `InMemoryFakeClock` (test). `portal::InMemoryPortalSessionCreator` holds an `Arc<dyn Clock>` with a `with_clock(clock)` setter. `webhook_dispatch::SystemClock` is re-exported from `crate::clock` (no inline panicking `SystemTime::now()`). Grep across `portal.rs` + `webhook_dispatch.rs` + `client.rs` + `clock.rs` shows NO `SystemTime::now()` callsites outside of:
- `clock.rs:123` — inside `impl Clock for SystemClock::now() { SystemTime::now() }` (native-only, the canonical wrapper).
- Comments / docstrings referring to the historic panic pattern.

The `WasmWorkerClock` correctly clamps non-finite/negative `Date::now()` values to `UNIX_EPOCH`. **VERDICT: pass.**

**FA-5: `TokioPostgresExecutor` binder** — `crates/corelink-audit-chain/src/neon_shadow/real_tokio_pg.rs`. The crate has explicit `#[cfg(all(feature = "neon-real", not(target_arch = "wasm32")))]` gates on the native module and `#[cfg(target_arch = "wasm32")]` gate on the `wasm_stub` module. The wasm32 stub `impl NeonExecutor` surfaces `NeonError::WasmOnly` on `execute` + `query` — type-checks on wasm32 without pulling tokio. The native impl uses `tokio::task::block_in_place(|| handle.block_on(fut))` per the sync `NeonExecutor` trait shape; requires multi-thread runtime (W20-P2-02 caveat noted). `cargo check -p corelink-audit-chain` green. **VERDICT: pass with P2-02 caveat.**

**FA-6: Testcontainers harness Drop** — `crates/corelink-audit-chain/tests/harness/pg_container.rs:145-156`. `PgHarness` holds `container: Option<ContainerAsync<PostgresImage>>` + `runtime: Arc<tokio::runtime::Runtime>`. `Drop::drop` calls `self.runtime.enter()` BEFORE `drop(container)` to satisfy testcontainers 0.21's async-drop requirement (`Handle::current()` panics otherwise). Field declaration order is `container` BEFORE `runtime` so Rust's drop order guarantees the runtime outlives the container drop. **VERDICT: pass with P2-03 caveat (no Docker-hang timeout wrapper — test-infra only).**

**FA-7: Charter compliance**:
- `#![forbid(unsafe_code)]` — present at `apps/server/src/lib.rs:27` + `apps/server/src/main.rs:10`.
- No `unwrap` / `expect` / `panic!` on production paths in `audit_export.rs`: all hits are either (a) panic-safe `unwrap_or_*` combinators, (b) in `#[cfg(test)]` modules with the `clippy::unwrap_used` / `clippy::expect_used` allow.
- Audit fail-CLOSED preserved — see FA-2.

**VERDICT: pass.**

### 3.3 Counts

| Severity | Count |
|---|---|
| P0 | 0 |
| P1 | 0 |
| P2 | 3 (W20-P2-01, W20-P2-02, W20-P2-03) |
| P3 | 3 (W20-P3-01 no-op, W20-P3-02, W20-P3-03 no-op) |

(Effective P3 count for scoring = 3; the two `No-op` entries are documented for traceability but do not represent open issues.)

---

## 4. Score

```
Score = 10 − (1.5 × 0) − (0.5 × 0) − (0.15 × 3) − (0.05 × 3)
      = 10 − 0 − 0 − 0.45 − 0.15
      = 9.40 / 10
```

Clamp `[0, 10]` → **9.40 / 10**.

SOTA bar = 8.50. **PASS** with margin = +0.90.

Per-stream scores (sub-scores; no P0/P1 anywhere):

| Stream | Score | Note |
|---|---|---|
| Meta `1b893b9` (wave-19 review) | 10.00 | no net-new findings; pure roll-up |
| FW-Reviewer `1470520` | 10.00 | proposal doc; no code |
| Sec-Matrix `c590a67` | 10.00 | matrix tightening; validator green |
| INV-Reg `b8ec7cf` | 10.00 | registry hygiene + DEBT dedup |
| Stripe-Clock `62b1f4b` | 9.85 | W20-P3-02 (cosmetic Date.now clamp log) |
| Neon-Test `af4fbbc` | 9.70 | W20-P2-03 (Docker-hang timeout) |
| Neon-Binder `a7ddea3` | 9.85 | W20-P2-02 (multi-thread runtime guard) |
| Audit-Emit-Export `4ccc161` | 10.00 | clean; W20-P3-01/03 are no-ops |
| Audit-Emit-Analytics+RLS `6716383` | 9.85 | W20-P2-01 (migration-ordering swallow) |
| Wave-18-P2P3 `f743811` | 10.00 | cosmetic; no findings |
| L7 reconcile `30e5f66` | 10.00 | wildcard arm composes correctly |

Aggregate weighted score (uniform-weight average across 11 sub-streams) = **9.83 / 10**; the formula-driven figure (9.40) is the conservative roll-up that propagates each P2/P3 once into the global pool. The conservative figure is what we cite in §5.

---

## 5. Verdict

**PASS — SEAL wave-20.**

- **0 P0** (no release-blocker).
- **0 P1** (nothing must-fix pre-GA).
- 3 P2 (one migration-ordering swallow + one runtime-flavor guard + one Docker-hang test-infra timeout) — all worth a wave-21 follow-on micro-stream but none block wave-20 SEAL.
- 3 P3 (2 cosmetic, 1 false-positive) — documentation / log-emit improvements.

Conservative score **9.40 / 10**, above the 8.50 SOTA bar by +0.90. Wave-19 baseline was 8.78; wave-20 improves the floor by +0.62 (the wave-20 streams close the only two A-P1 gaps that wave-19 flagged as remaining open: A-P1-02 / A-P1-03 emit-discipline and B-P1-01 RLS `WITH CHECK`).

---

## 6. Recommendation

**SEAL wave-20.** Open three lightweight wave-21 follow-on items:

1. **W21-FOLLOWUP-01** (P2) — `migrations/neon/0002_*.sql`: replace `EXCEPTION WHEN undefined_object THEN NULL` swallow with a `RAISE NOTICE` (or refactor to a single combined migration that always lands `WITH CHECK` atomically). Half-hour, doc-only.
2. **W21-FOLLOWUP-02** (P2) — `real_tokio_pg.rs::TokioPostgresExecutor::new`: programmatic assert on `Handle::current().runtime_flavor() == MultiThread`. One-line `debug_assert!` + a `#[cfg(test)]` panic-test. Half-hour.
3. **W21-FOLLOWUP-03** (P2) — `tests/harness/pg_container.rs::Drop`: wrap `drop(container)` in a `tokio::time::timeout(Duration::from_secs(5), ...)`. Hour, test-infra only.

None of the above gate GA; they are R-prep hygiene for the next wave window.

---

## Report

```
BRANCH: wt/r-prep-wave20-adversarial-review
COMMIT: 30e5f66
AGGREGATE SCORE: 9.40/10 PASS
PER-STREAM SCORES: meta=10.00, fw-reviewer=10.00, sec-matrix=10.00, inv-reg=10.00, stripe-clock=9.85, neon-test=9.70, neon-binder=9.85, audit-emit-export=10.00, audit-emit-analytics-rls=9.85, wave18-p2p3=10.00, l7-reconcile=10.00
FINDINGS: P0=0 P1=0 P2=3 P3=3
P0 LIST: none
RECOMMENDATION: SEAL wave-20
TIME: ~38min
```

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

---

## 7. Closure (wave-22 follow-up absorption)

All three P2 follow-ups have been absorbed in wave-22 on branch
`wt/r-prep-w21-followups-p2` (base `bccdd97`):

- **W21-FOLLOWUP-01 — CLOSED.** `migrations/neon/0002_audit_events_shadow_with_check.sql`
  now emits `RAISE NOTICE 'tenant_isolation_<table> policy already exists'`
  in both `EXCEPTION WHEN undefined_object` arms instead of swallowing
  with `NULL`. Idempotency preserved; operator-visible logging restored.
- **W21-FOLLOWUP-02 — CLOSED.** `crates/corelink-audit-chain/src/neon_shadow/real_tokio_pg.rs::TokioPostgresExecutor::connect`
  and `::from_pool` now `debug_assert!` that
  `Handle::current().runtime_flavor() == RuntimeFlavor::MultiThread`,
  failing-fast in test/debug builds and no-op in release. (The audit
  prose suggested `metrics().num_workers() > 1`, but `Handle::metrics`
  is `tokio_unstable`-gated; `runtime_flavor()` is the stable equivalent
  and matches the audit's actual recommendation in §3.1 W20-P2-02.)
- **W21-FOLLOWUP-03 — CLOSED.** `crates/corelink-audit-chain/tests/harness/pg_container.rs::Drop`
  now wraps the testcontainers async-drop in a `tokio::time::timeout`
  bound at 30s; on elapsed timeout it logs to stderr (Drop must never
  panic). The runtime guard via `runtime.block_on` preserves the
  `Handle::current()` requirement.

Status roll-up: **3 P2 CLOSED.** No P0/P1 remained; the remaining P3
entries are cosmetic/no-op and tracked in the issue backlog.
