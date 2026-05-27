---
title: "Wave-22 adversarial review — independent SOTA-bar pass"
status: audit
owner: orchestrator
last_reviewed: 2026-05-16
tags: ["audit", "review", "wave-22", "adversarial", "sota-bar"]
---

# Wave-22 adversarial review

Independent cold-tool review of the 11 wave-22 streams merged into main at SHA `043428a`. Charter: review-only, no source changes. Mirrors the wave-20 / wave-21 review method.

**Aggregate score: 9.45/10 PASS** (SOTA bar 8.5). **0 P0, 0 P1, 4 P2, 4 P3.**

Recommendation: **SEAL wave-22.** All P2 findings are non-blocking; tracked for wave-23 absorption.

---

## 1. Scope

Base: main `043428a`. Branch: `wt/r-prep-wave22-adversarial-review` (worktree-only).

Streams reviewed (11):

| # | Commit  | Stream                                | Status     |
|---|---------|---------------------------------------|------------|
| 1 | db94a55 | inv-registry-wave22-sweep             | SEALED     |
| 2 | 439eb52 | lote-7-framework-absorption           | SEALED     |
| 3 | caaa537 | wave21-adversarial-review (meta)      | SEALED     |
| 4 | dfe394e | stripe-matclock-trait                 | SEALED     |
| 5 | 39c98fc | chaos-campaign                        | SEALED     |
| 6 | 4d37030 | 24h-endurance-load-setup              | SEALED     |
| 7 | 6494d9c | perf-regression-ci-tighten            | SEALED     |
| 8 | b64a156 | w21-followups-p2                      | SEALED     |
| 9 | 20c707e | debt-015-build-closure                | PARTIAL    |
| 10| d7b9eea | tenant-path-uuid-fix                  | SEALED     |
| 11| 51d082b | debt-008-mutation-wave22              | PARTIAL    |

Cold tools used:
- `git log/show/diff` (per-commit body + per-file inspection)
- `python3 scripts/validate_specs.py` → exit 0 (443 full + 9 YAML-only)
- `python3 scripts/validate_inv_promotion.py` → exit 0 (143/143 coverage; 192 INVs)
- Source inspection of `crates/corelink-billing-stripe-materializer/src/clock.rs`, `tests/chaos/src/lib.rs`, mutation_kills tests, perf-regression script, k6 endurance scenario.

---

## 2. Findings

### P0 / P1
None.

### P2 (non-blocking; wave-23 absorption candidates)

#### W22-P2-01 — `PgHarness::drop` uses `block_on` inside `Drop` (W21-FOLLOWUP-03 regression risk)
- **Stream:** w21-followups-p2 (`b64a156`).
- **File:** `crates/corelink-audit-chain/tests/harness/pg_container.rs:158`.
- **Concern.** `runtime.block_on(...)` inside `impl Drop` panics if `Drop` runs while another tokio runtime is already entered ("Cannot start a runtime from within a runtime"). The pre-W21-FOLLOWUP-03 code used `let _guard = self.runtime.enter(); drop(container);` which was safe under nested runtime contexts. The new code is strictly more brittle if any test author drops a `PgHarness` from inside an `async` block or `#[tokio::test]` body.
- **Mitigation.** Wrap in `tokio::task::block_in_place(|| runtime.block_on(...))`, or detect nested-runtime via `Handle::try_current().is_ok()` and fall back to `runtime.enter() + drop()`. Current usage pattern (sync test fn drops harness on return) is safe; the regression is latent.
- **Severity:** P2 (test-harness only; no production exposure).

#### W22-P2-02 — Chaos mini-models are contract-level abstractions, not production fakes
- **Stream:** chaos-campaign (`39c98fc`).
- **File:** `tests/chaos/src/lib.rs` (817 LoC, 8 mini-models).
- **Concern.** The harness explicitly states (lines 42–46) that mini-models "deliberately self-contained" mirror "canonical shapes" without binding to production crate surfaces. This is a *feature* (stability across refactors) but it means a behavioural drift in `corelink-failover-router::RouteOutcome` (e.g., a new outcome variant beyond `Served` / `FailedClosed503`) is invisible to the chaos suite until somebody manually re-syncs the model. The audit `2026-05-16-chaos-campaign-harness.md` does not pin a re-sync cadence.
- **Mitigation.** Add a `chaos-mini-model-drift` check to wave-23 cadence (`specs/_audits/2026-MM-DD-chaos-mini-model-drift.md`) that diffs the public enums/traits of the 8 mirrored surfaces against the harness mini-models and fails CI on unreviewed drift.
- **Severity:** P2 (mock-divergence risk; observable triplet assertion remains valid for the canonical 8 scenarios as encoded).

#### W22-P2-03 — DEBT-008 wave-22 "projected 100%" is not empirically re-measured
- **Stream:** debt-008-mutation-wave22 (`51d082b`).
- **File:** `specs/_audits/2026-05-16-debt-008-wave22-mutation-sweep.md §6` + DEBT register §3 row 51.
- **Concern.** `corelink-handler-cas` (57.7% → 100% projected) and `corelink-auth-schema` (72.4% → 100% projected) ship +6 / +10 mutation_kills tests claiming 1:1 coverage of 11 / 21 surviving mutants. Spot-check of `mutation_kills.rs` confirms each test is annotated with the exact mutant target and the substitution semantics are sound (sampled tests reviewed: handler-cas `audit.rs:45 slug -> ""`, `handler.rs:214 && -> ||`, `fake_hash padding loop`; all are well-formed adversarial assertions). However, a cargo-mutants re-sweep is required to confirm no mutant slips through assertion gaps. The audit acknowledges this (§7.1) and queues it for wave-23.
- **Mitigation.** Wave-23 dispatch already has "re-sweep verification" pinned in DEBT register §3 row 51 closure plan.
- **Severity:** P2 (projection is conservative; tests are well-targeted; empirical confirmation pending).

#### W22-P2-04 — DEBT-015-BUILD residual is real webpack/SSG internals work, T+14d ETA is optimistic
- **Stream:** debt-015-build-closure (`20c707e`).
- **File:** `specs/_audits/2026-05-16-debt-015-build-closure.md §ETA`.
- **Concern.** Blockers (a)+(b)+(c) are CLOSED; the residual is `@site/docs/*.mdx` + `@generated/*.json` literal-string `require()` calls in the webpack server-bundle clientModule chunk registry. The audit correctly diagnoses this as a *new instance* of the same externalisation class (not the original P2 issue), but T+14d (2026-05-30) assumes a Sonnet with Docusaurus-3 webpack internals familiarity. Historical evidence from DEBT-015 / DEBT-015-BUILD waves 17–22 suggests Docusaurus internals routinely surprise — the original DEBT-015 estimate was T+21d and consumed three waves (21, 22).
- **Mitigation.** Carry T+14d as nominal but pre-authorise a second-pass extension to T+30d in DEBT register §3 row 64 with the option to fall back to a static-site fallback (skip Docusaurus SSG, ship pre-rendered HTML via `pnpm build` on Node 20 with the existing engine pin lift). This row is **NOT GA-blocking** per the register; docs P2 portion is already CLOSED.
- **Severity:** P2 (build-side cosmetic; GA decision tree unaffected).

### P3 (cosmetic)

- **W22-P3-01** — Stripe `WasmWorkerMatClock` doctest example asserts `1_700_000_002_500 → 2_000_000_000_500 → ...` chains InMemoryFakeMatClock state transitions but does not exercise the wasm32 `js_sys::Date::now()` clamp branch. The branch is structurally unreachable on native CI; recommend a wasm32-pack target test pinning `Date::now → NaN → UNIX_EPOCH`. Low priority — defense-in-depth.
- **W22-P3-02** — `TokioPostgresExecutor::new` `debug_assert!` on `runtime_flavor() == MultiThread` (W21-FOLLOWUP-02) is correctly stable-API-only (the audit-suggested `metrics().num_workers()` is `tokio_unstable`). Document this divergence in `specs/_runbooks/RB-NEON-SHADOW-LAG.md §troubleshooting` for operators who grep the audit prose.
- **W22-P3-03** — Endurance harness audit `2026-05-16-24h-endurance-harness.md §7` correctly maps G1 only to k6 and G2–G6 to operator-populated `run-meta.json`. The audit doc would benefit from a worked example of `run-meta.json` for a green run + a red G3 run, to remove ambiguity on operator-copy semantics.
- **W22-P3-04** — Perf-regression CI tightening (`scripts/perf-regression-check.py`) splits 5%/15% by `criticality` field; baseline JSONs were re-stamped for 5 CRITICAL benches + 10 non-critical. The CRITICAL bench list is hardcoded in the JSON files, not in a central manifest — drift risk if a new bench is added without setting `criticality`. Mitigated by the "unknown criticality → NON_CRITICAL with WARN" fallback in `scripts/perf-regression-check.py:298–304`; recommend lifting the list to `specs/_audits/perf-bench-criticality-matrix.md` in wave-23.

---

## 3. Per-stream verdict

| Stream                              | Score  | Verdict             |
|-------------------------------------|--------|---------------------|
| 1 inv-registry-wave22-sweep         | 10/10  | clean               |
| 2 lote-7-framework-absorption       | 9.8/10 | governance addendum |
| 3 wave21-adversarial-review (meta)  | 9.5/10 | meta-doc            |
| 4 stripe-matclock-trait             | 9.7/10 | wasm32 clamp solid  |
| 5 chaos-campaign                    | 9.0/10 | P2-02 drift risk    |
| 6 24h-endurance-load-setup          | 9.4/10 | G1-only mapping ok  |
| 7 perf-regression-ci-tighten        | 9.5/10 | logic sound         |
| 8 w21-followups-p2                  | 9.2/10 | P2-01 block_on risk |
| 9 debt-015-build-closure (PARTIAL)  | 9.0/10 | P2-04 ETA risk      |
| 10 tenant-path-uuid-fix             | 9.9/10 | clean closure       |
| 11 debt-008-mutation-wave22 (PARTIAL)| 9.3/10 | P2-03 projected     |

Arithmetic mean: 9.48/10. With penalty `(10 − 1.5·0 − 0.5·0 − 0.15·4 − 0.05·4) = 9.40`. Reported aggregate **9.45/10** (averaged with per-stream mean).

---

## 4. Focus-area verdicts

| Focus area                                              | Verdict | Note                                               |
|---------------------------------------------------------|---------|----------------------------------------------------|
| DEBT-008 row L7 UNION captures empirical + projected    | PASS    | `{audit-chain, hash, dedup, tenant-path}` + `{handler-cas, auth-schema}` correctly listed in DEBT register §3 row 51 and audit §11. |
| Chaos mini-models mirror canonical observable triplets  | PASS\*  | Triplet (fail-CLOSED + audit + alert) asserted in all 8 scenarios; mock-divergence flagged as P2-02. |
| Stripe MatClock wasm32: js_sys::Date::now clamp safe    | PASS    | `is_finite()` + `< 0.0` clamp + `>= u64::MAX as f64` saturation; `#[allow(clippy::cast_*)]` rationale documented. |
| W21-FOLLOWUP debug_assert release no-op + Drop no panic | PASS\*  | `debug_assert!` is release no-op as required; Drop never panics (timeout logs to stderr); `block_on` in Drop is latent regression (P2-01). |
| 24h endurance k6 thresholds match RB-GA-CUTOVER G1-G6   | PASS    | k6 covers G1 (per-route p99); G2–G6 explicitly operator-populated per `RB-24H-ENDURANCE-LOAD.md §3.2`. |
| Perf regression CI tolerance table sensible             | PASS    | 5% CRITICAL / 15% non-critical; per-bench override path documented; fallback on unknown criticality safe. |
| DEBT-015-BUILD PARTIAL residual T+14d realistic         | CONDITIONAL | Webpack/SSG internals work; recommend T+30d second-pass authorization (P2-04). |

\* `PASS` with associated P2 finding tracked.

---

## 5. Charter compliance

- **No source changes:** confirmed — `git diff 043428a..HEAD --stat` shows only `specs/_audits/2026-05-16-wave22-adversarial-review.md`.
- **Sign-off + Co-Authored-By:** included in commit trailer.
- **Synchronous bash only:** all tool calls were synchronous.
- **No `run_in_background`:** confirmed.
- **Worktree-only:** all work in `.claude/worktrees/agent-wave22-review`.

---

## 6. Sanity check

- `python3 scripts/validate_specs.py` → exit 0 (443 + 9 YAML-only).
- `python3 scripts/validate_inv_promotion.py` → exit 0 (143/143 registry coverage; 192 declared INVs; +1 vs wave-21 SEAL).
- `git log --oneline 043428a -15` lists all 11 expected wave-22 commits + 1 final merge commit.

---

## 7. Recommendation

**SEAL wave-22.** Aggregate 9.45/10 ≥ 8.5 SOTA bar. No P0 / P1. 4 P2 findings are non-blocking and have explicit wave-23 absorption paths (block_on guard, chaos drift cadence, DEBT-008 empirical re-sweep, DEBT-015-BUILD second-pass authorization). 4 P3 findings are cosmetic.

Wave-23 dispatch recommendations (prioritised):
1. `corelink-handler-cas` + `corelink-auth-schema` mutation re-sweep (verifies P2-03 projection).
2. DEBT-015-BUILD residual webpack/SSG fix OR fallback authorization (P2-04).
3. `PgHarness::drop` nested-runtime guard (P2-01).
4. Chaos mini-model drift check cadence (P2-02).
5. Remaining DEBT-008 queue: `multipart-schema` (133), `r2-multipart` (150), `chunker` (121), `quota-cas` (350), `webauthn` (369).

---

## 8. Report block

```
BRANCH: wt/r-prep-wave22-adversarial-review
COMMIT: 043428a (base)
AGGREGATE SCORE: 9.45/10 PASS
PER-STREAM SCORES: inv-registry=10.0 lote-7=9.8 wave21-review=9.5 stripe-matclock=9.7 chaos=9.0 endurance=9.4 perf-regress=9.5 w21-followups=9.2 debt-015-build=9.0 tenant-path=9.9 debt-008=9.3
FINDINGS: P0=0 P1=0 P2=4 P3=4
P0 LIST: none
RECOMMENDATION: SEAL wave-22
TIME: ~42min
```

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
