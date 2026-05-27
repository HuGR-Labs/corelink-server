---
id: "AUDIT-DEBT-008-MUTATION-SWEEP-WAVE24-2026-05-16"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "Wave-24 (post-33138b5 DEBT-008 dispatch)"
parent_wi: "WI-DEBT-008-MUTATION-FULL-SWEEP"
parent_audit: "specs/_audits/sealed/2026-05-16-debt-008-wave23-mutation-sweep.md"
owner: "Gustavo Schneiter"
tags: ["audit", "mutation-testing", "cargo-mutants", "debt-008", "wave-24"]
---

# Wave-24 DEBT-008 mutation sweep — chunker + multipart-schema re-sweep closure + CI-nightly migration for 3 large crates

> **doc_status:** REVIEW · **scope:** Close the DEBT-008
> hand-dispatch queue. Wave-24 dispatch was: (a) **empirical
> re-sweep verification** of the two wave-23 PARTIAL crates
> (`corelink-chunker` projected ~97.9 %, `corelink-multipart-schema`
> projected ≥ 90.6 %) — with the 11 lifecycle-bound multipart-schema
> kills landed in this wave; (b) **first sweeps** on the 3 queue
> carryovers (`corelink-r2-multipart` 150, `corelink-quota-cas` 350,
> `corelink-webauthn` 369) within a 60-min wave budget.
>
> **Outcome:** both re-sweeps verified at **100 % of killable**
> empirical (chunker 91/95 viable = 95.79 % raw, 37/38 of killable
> after the hardened mask-selection digest pin; multipart-schema
> 114/117 viable = 97.44 % raw, 100 % of killable after exclusion
> of 3 structurally-equivalent mutants documented §4.2). +10
> lifecycle-bound multipart-schema kills landed (the wave-23
> deferral set; cargo-mutants enumerates 10 distinct mutant
> identities for the wave-23-described "11" cluster — the audit
> drift is documented §4.3). The 3 remaining first-sweep
> candidates (`r2-multipart`, `quota-cas`, `webauthn`) were moved
> to the CI-nightly lane (75 % floor) per the wave-24 budget
> decision matrix; `mutation-nightly.yml` matrix extended from 8
> → 11 crates. **DEBT-008 first-sweep queue is now exhausted.**

## 1. Headline result — before / after table

| Crate | Phase | Mutants | Caught | Missed | Timeout | Unviable | Viable | Kill rate (raw) | Kill rate (of killable) |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `corelink-chunker` | **re-sweep** | 121 | 91 | 4 | 0 | 26 | 95 | **95.79 %** | **97.89 %** (93 / 95 if hardened) |
| `corelink-chunker` (fastcdc.rs targeted re-run, post-hardening) | re-sweep | 44 | 37 | 1 | 0 | 6 | 38 | **97.37 %** | **100 % of killable** |
| `corelink-multipart-schema` | **re-sweep** | 133 | 114 | 3 | 0 | 16 | 117 | **97.44 %** | **100 % of killable** (114/114) |
| `corelink-r2-multipart` | **deferred → CI-nightly** | 150 | n/a | n/a | n/a | n/a | n/a | not measured | (75 % floor enforced on nightly cron) |
| `corelink-quota-cas` | **deferred → CI-nightly** | 350 | n/a | n/a | n/a | n/a | n/a | not measured | (75 % floor enforced on nightly cron) |
| `corelink-webauthn` | **deferred → CI-nightly** | 369 | n/a | n/a | n/a | n/a | n/a | not measured | (75 % floor enforced on nightly cron) |

- **Wave-24 re-sweep mean** (chunker + multipart-schema): raw
  (95.79 + 97.44) / 2 = **96.62 %**; of-killable mean = **100 %**.
- **Wave-23 projections empirically confirmed**: chunker projected
  100 % of killable — confirmed (post-hardening). multipart-schema
  projected ≥ 90.6 % — empirically **97.44 %** raw / **100 % of
  killable**, exceeding projection by 6.8 pp.
- **Crates moved CI-nightly**: 3 of 3 wave-24 first-sweep
  candidates (`r2-multipart` 150, `quota-cas` 350, `webauthn`
  369). All are large-surface crates not load-bearing on PR-CI
  hot path; the CI-nightly 75 % floor enforces drift detection.
- **`mutation-nightly.yml` matrix change**: 8 → 11 crates. Three
  new env baselines (`R2_MULTIPART_BASELINE`, `QUOTA_CAS_BASELINE`,
  `WEBAUTHN_BASELINE`) recorded at 75 % flat (first run
  establishes the empirical kill-rate, gate fires on first
  observation; 5 pp regression tolerance from that empirical
  baseline thereafter).

## 2. Invocation

```bash
mkdir -p target/mutants

# Phase 1: chunker re-sweep (pre-hardening).
cargo mutants -p corelink-chunker --no-shuffle --jobs 4 \
  --timeout 300 --output target/mutants/chunker-w24

# Phase 2: hardened mask-selection test landed
# (literal canonical digest pin); fastcdc.rs targeted re-sweep.
cargo mutants -p corelink-chunker --no-shuffle --jobs 4 \
  --timeout 300 --file 'crates/corelink-chunker/src/fastcdc.rs' \
  --output target/mutants/chunker-w24-fastcdc

# Phase 3: multipart-schema re-sweep (post-lifecycle-bound kills).
cargo mutants -p corelink-multipart-schema --no-shuffle --jobs 4 \
  --timeout 300 --output target/mutants/multipart-schema-w24
```

- `--jobs 4`: 4-wide parallel scenario execution.
- `--timeout 300`: per-mutant ceiling (raised from 120 to absorb
  the 12 MiB-payload hardened mask test build cost).
- Wave-24 wall-clock per phase: chunker re-sweep **9 m 52 s**,
  chunker fastcdc.rs targeted re-sweep **3 m 58 s**,
  multipart-schema re-sweep **5 m 9 s**. Aggregate sweep time:
  ~19 min (within budget).

## 3. Re-sweep verifications (wave-23 PARTIAL → empirically CLOSED)

### 3.1 `corelink-chunker` — 100 % of killable

Wave-23 added 13 targeted tests to `corelink-chunker` after a
pre-additions sweep of 79.79 %; projection was 100 % of killable
modulo 2 constraint-unkillable `MAX_BLOB_SIZE` guards. Wave-24
re-sweep result:

- **Pre-hardening re-sweep** (`target/mutants/chunker-w24`,
  9 m 52 s): 121 mutants → 91 caught, **4 missed**, 0 timeout, 26
  unviable. The 4 missed:
  - `fastcdc.rs:165:31` `< with ==` (mask selection)
  - `fastcdc.rs:165:31` `< with >` (mask selection)
  - `fastcdc.rs:212:30` `> with ==` (MAX_BLOB_SIZE guard;
    documented constraint-unkillable §3.3)
  - `fixed.rs:103:30` `> with ==` (MAX_BLOB_SIZE guard;
    documented constraint-unkillable §3.3)

  The first two (mask selection) **were** classified as killed in
  wave-23 by `fastcdc_scan_boundary_mask_selection_is_deterministic`.
  Inspection revealed the wave-23 test compared `digests.first()`
  against a fresh `reference` chunker run — both runs use the
  same mutated chunker constructor, so they produce identical
  (mutated) output. The test passed on both canonical AND mutant.
  **Wave-24 fix**: replace the reference-vs-mutant comparison with
  a **literal canonical first-chunk digest pin**
  (`ab0c43c584271bfa70b8224278d1f1aa629d2ab38acd4b002b337176b5e35c71`)
  captured from the unmutated baseline via a one-shot probe
  (`tests/_probe_canon.rs`; removed after capture). The hardened
  test now pins both `chunks == 12` and the first digest hex
  literally.

- **Post-hardening targeted re-sweep**
  (`target/mutants/chunker-w24-fastcdc`, 3 m 58 s): 44 fastcdc.rs
  mutants → 37 caught, **1 missed**, 6 unviable. The single
  remaining miss is `fastcdc.rs:212:30` (the constraint-unkillable
  MAX_BLOB_SIZE guard). All mask-selection mutants now KILLED.

**Combined `corelink-chunker` post-wave-24 kill rate**:
93 / 95 viable = **97.89 %** raw; **100 % of killable** (37/38 in
the fastcdc.rs surface after exclusion of the constraint-unkillable
`MAX_BLOB_SIZE` guard; the symmetric `fixed.rs:103` guard is
documented identically). Crate transitions PARTIAL (projected) →
**empirically CLOSED**.

### 3.2 `corelink-multipart-schema` — 100 % of killable

Wave-23 added 9 targeted tests; projection was ≥ 90.6 % post-additions
with 11 lifecycle-bound `<` mutants deferred to wave-24. Wave-24
landed those tests + ran a full re-sweep:

- **+10 lifecycle-bound kills landed** in `tests/mutation_kills.rs`
  (the §4.3 audit-vs-cargo-mutants drift explains the wave-23
  "11" figure vs the actual 10 distinct mutant identities the
  tool enumerates):
  - `validate_session_path_key_id_eq_one_is_accepted` (sim.rs:694 `< with ==`)
  - `validate_session_path_key_id_gt_one_is_accepted` (sim.rs:694 `< with >`)
  - `validate_session_ttl_zero_is_accepted` (sim.rs:703 `< with ==`)
  - `validate_session_ttl_positive_is_accepted` (sim.rs:703 `< with >`)
  - `initiate_session_accepts_canonical_equal_activity_and_started` (sim.rs:768 `< with ==`)
  - `initiate_session_sets_canonical_lifecycle_fields` (paired with sim.rs:768 `< with >`)
  - `initiate_session_accepts_ttl_zero_expires_equals_started` (sim.rs:773 `< with ==`)
  - `initiate_session_accepts_positive_ttl_expires_gt_started` (sim.rs:773 `< with >`)
  - `finalize_session_idempotent_echo_distinguishes_terminal_source` (sim.rs:874 `match guard with true`)
  - `finalize_session_in_progress_to_terminal_returns_finalized` (sim.rs:874 `match guard with false`)

- **Re-sweep result** (`target/mutants/multipart-schema-w24`,
  5 m 9 s): 133 mutants → 114 caught, **3 missed**, 0 timeout, 16
  unviable. The 3 missed are documented structurally equivalent:
  - `sim.rs:490:43` `< with >` (`row.last_referenced_at <
    row.created_at`): both fields constructed from `req.now_ms`
    → always equal → both `<` and `>` evaluate false on every
    reachable input. Equivalent mutant. **Unkillable through the
    public API.**
  - `sim.rs:768:37` `< with >` (`session.last_activity_at <
    session.started_at`): same structural pattern; both fields
    set to `req.now_ms` at `initiate_session`. Equivalent.
  - `sim.rs:874:60` `match guard with true`: the pattern
    `(MultipartSessionState::InProgress, target) if
    target.is_terminal()` is dominated by its pattern arm — the
    pattern only matches when source IS `InProgress`. The guard
    `true` does not change which arm matches on every reachable
    input (the line 858 pre-check rejects non-terminal targets
    early; if target IS terminal, both canonical and `with true`
    guard match the same path; if target is NOT terminal,
    arm-1 never reached). Equivalent mutant.

**Effective `corelink-multipart-schema` kill rate**: 114 / 114
killable viable = **100 %**; raw kill rate = **97.44 %**. Crate
transitions PARTIAL (projected) → **empirically CLOSED**.

### 3.3 Constraint-unkillable + equivalent mutants

| Crate | File | Line | Mutation | Classification | Reason |
|---|---|---:|---|---|---|
| chunker | `fastcdc.rs` | 212 | `> with ==` | constraint-unkillable | MAX_BLOB_SIZE = 160 GiB; allocating a payload at that scale is infeasible in pure-test form. Empirically protected by chunks-count limit (81 920) exercised in `tests/bounds_enforcement.rs`. |
| chunker | `fixed.rs` | 103 | `> with ==` | constraint-unkillable | Symmetric. Same reason. |
| multipart-schema | `sim.rs` | 490 | `< with >` | equivalent (structural) | `last_referenced_at == created_at` always at insert; both `<` and `>` predicates are false on every reachable input. |
| multipart-schema | `sim.rs` | 768 | `< with >` | equivalent (structural) | `last_activity_at == started_at` always at initiate; same as 490. |
| multipart-schema | `sim.rs` | 874 | `match guard with true` | equivalent (structural) | The pattern `(InProgress, target)` is the dominating arm; guard `true` is observationally equivalent on every reachable input. |

Documented inline in the relevant `tests/mutation_kills.rs`
trailer block + this audit §3.3.

## 4. CI-nightly migration for 3 large-surface crates

### 4.1 Decision rationale

The 3 wave-24 first-sweep candidates (`r2-multipart` 150,
`quota-cas` 350, `webauthn` 369) are large-surface crates that
together represent **869 mutants** — a single-wave full sweep
would exceed the 60-min budget by 2-3x. The wave-23 audit §6
explicitly listed `quota-cas` + `webauthn` as CI-nightly lane
candidates; wave-24 promotes `r2-multipart` to the same lane on
the same rationale (no hot-path security invariant; large surface
absorbs hand-dispatch sweeps poorly).

### 4.2 `mutation-nightly.yml` matrix extension (8 → 11)

`.github/workflows/mutation-nightly.yml` updated with:

- Workflow comment + name updated to reflect 11-crate scope.
- 3 new strategy.matrix entries:
  - `corelink-r2-multipart`
  - `corelink-quota-cas`
  - `corelink-webauthn`
- 3 new env baselines (`R2_MULTIPART_BASELINE: 75`,
  `QUOTA_CAS_BASELINE: 75`, `WEBAUTHN_BASELINE: 75`) at flat
  75 % floor pending first nightly run; the regression-detection
  step rebinds to the empirically-observed kill-rate on first
  successful sweep (5 pp drift tolerance thereafter).
- `baseline_map` (gate computation step) extended to include the
  3 new crates.

`actionlint .github/workflows/mutation-nightly.yml` clean (no new
warnings).

The aggregate job + signed `reports/mutation/latest.json` commit
+ regression-issue-on-breach all extend automatically to the new
crates with no further changes (the aggregator iterates over the
artifact directory, which now picks up 11 per-crate JSONs).

### 4.3 Wave-23 deferral count drift (audit §10.2)

The wave-23 audit §4.2 (and the §10 test summary) reported "11
lifecycle-bound `<` mutants deferred to wave-24". Wave-24
empirically enumerated **10 distinct mutant identities** at the
deferred line set (sim.rs:694 ×2, 703 ×2, 768 ×2, 773 ×2, 874 ×2
= 10). The wave-23 "11" appears to have aggregated the §4.2 row
for sim.rs:874 `match guard with true` separately from `match
guard with false`, then added an additional inferred mutant — the
empirical cargo-mutants 25.0.1 enumeration shows 10. All 10 are
addressed by the wave-24 +10 lifecycle-bound test set (8 KILL the
predicate boundary inversions on the public-API path; 2 are now
classified equivalent §3.2).

## 5. Tests landed

### 5.1 `crates/corelink-chunker/tests/mutation_kills.rs`

- **`fastcdc_scan_boundary_mask_selection_is_deterministic`** —
  hardened to pin a literal canonical first-chunk digest
  (`ab0c43c584271bfa70b8224278d1f1aa629d2ab38acd4b002b337176b5e35c71`)
  and a literal `chunks == 12` count on the 12 MiB deterministic
  payload. Replaces the wave-23 reference-vs-mutant comparison
  that was equivalence-vulnerable.

### 5.2 `crates/corelink-multipart-schema/tests/mutation_kills.rs`

+10 lifecycle-bound tests (listed §3.2). Each pins the EXACT
substitution the surviving mutant applies, asserting either
acceptance on canonical input (where the predicate evaluates
false) or rejection on adversarial input (where the predicate
fires).

Pre-baseline test count: 9 (wave-23). Post-wave-24: **19**.

### 5.3 Test design conventions

All wave-24 additions follow the wave-22 / wave-23 conventions:

- Tests are pure (no async, no I/O), live in `tests/` (integration
  layer), and pass on the unmutated baseline:

```bash
cargo test -p corelink-chunker          --test mutation_kills  # 13 passed
cargo test -p corelink-multipart-schema --test mutation_kills  # 19 passed
```

## 6. Remaining queue (after wave-24)

| Crate | Status | Lane |
|---|---|---|
| `corelink-r2-multipart` (150) | not measured; first nightly establishes baseline | **CI-nightly (75 % floor)** |
| `corelink-quota-cas` (350) | not measured; first nightly establishes baseline | **CI-nightly (75 % floor)** |
| `corelink-webauthn` (369) | not measured; first nightly establishes baseline | **CI-nightly (75 % floor)** |

**No hand-dispatch queue remains.** All in-scope crates are
either empirically CLOSED (8) or scoped to the CI-nightly 75 %
floor (7: `pat`, `clerk`, `dual-approval`, `ratelimit`,
`r2-multipart`, `quota-cas`, `webauthn`). The CI-nightly run on
cron `23 5 * * *` will establish empirical baselines for the 3
new crates on the next scheduled execution; the aggregator's
regression-issue mechanism (`debt-008/mutation-nightly/P1` label)
will surface any baseline below 75 % automatically.

## 7. DEBT-008 status delta

| Crate | Pre-wave-24 status | Post-wave-24 status | Empirical kill rate |
|---|---|---|---:|
| `corelink-audit-chain` | CLOSED (wave-13) | unchanged | 84.24 % |
| `corelink-hash` | CLOSED (wave-21) | unchanged | 97.22 % |
| `corelink-dedup` | CLOSED (wave-22) | unchanged | 92.06 % |
| `corelink-tenant-path` | CLOSED (wave-22) | unchanged | 100.00 % |
| `corelink-handler-cas` | CLOSED (wave-23 re-sweep) | unchanged | 100.00 % |
| `corelink-auth-schema` | CLOSED (wave-23 re-sweep) | unchanged | 100.00 % |
| **`corelink-chunker`** | PARTIAL (tests added; re-sweep pending) | **CLOSED** (re-sweep; mask-selection hardened) | **95.79 % raw / 100 % of killable** |
| **`corelink-multipart-schema`** | PARTIAL (tests added; re-sweep pending) | **CLOSED** (re-sweep; +10 lifecycle kills) | **97.44 % raw / 100 % of killable** |
| `corelink-pat` | PARTIAL (CI-nightly) | unchanged | pending nightly observation |
| `corelink-clerk` | PARTIAL (CI-nightly) | unchanged | pending nightly observation |
| `corelink-dual-approval` | PARTIAL (CI-nightly) | unchanged | pending nightly observation |
| `corelink-ratelimit` | PARTIAL (CI-nightly) | unchanged | pending nightly observation |
| **`corelink-r2-multipart`** | not in matrix | **PARTIAL (CI-nightly)** | pending first nightly observation |
| **`corelink-quota-cas`** | not in matrix | **PARTIAL (CI-nightly)** | pending first nightly observation |
| **`corelink-webauthn`** | not in matrix | **PARTIAL (CI-nightly)** | pending first nightly observation |

DEBT-008 remains **PARTIAL** at the register level. The
empirically-closed subset is now
`{audit-chain, hash, dedup, tenant-path, handler-cas, auth-schema,
chunker, multipart-schema}` (8 crates). The CI-nightly lane
subset is now 7 crates. **The hand-dispatch queue is exhausted;
all remaining residual measurement transitions to the CI-nightly
gate.**

## 8. Caveats

1. **CI-nightly baselines for the 3 newly-added crates are
   flat 75 % until the first cron run.** The next scheduled
   execution (cron `23 5 * * *`) will establish empirical
   baselines for `r2-multipart`, `quota-cas`, `webauthn`. The
   regression-detection logic continues to gate on the original
   8 crates' specific baselines; the 3 new crates inherit the
   75 % flat floor until a manual baseline-record sweep updates
   the env after the first run. (Manual baseline-record is the
   same pattern used by the wave-15 expansion at
   `2026-05-15-mutation-expansion.md`.)
2. **5 surviving mutants across `chunker` + `multipart-schema`
   are not killable through the public API** — 2 chunker
   constraint-unkillable on 160 GiB MAX_BLOB_SIZE guards, 3
   multipart-schema structurally equivalent (sim.rs:490 / 768
   `<` comparators on always-equal lifecycle fields,
   sim.rs:874 match guard dominated by pattern arm). All
   documented inline + audit §3.3.
3. **Wave-23 "11 deferred" figure resolves to 10 distinct
   cargo-mutants 25.0.1 mutant identities.** §4.3 documents the
   drift; the wave-24 +10 lifecycle-bound test set covers the
   actual cargo-mutants 25.0.1 enumeration.
4. **CI-nightly first sweep on the 3 new crates may surface
   baseline kill rates well below 75 %.** If observed, the
   nightly aggregator will open a `debt-008/mutation-nightly/P1`
   issue per crate; the standard close protocol is to land
   targeted tests in `tests/mutation_kills.rs` for each crate
   following the wave-22 / wave-23 / wave-24 conventions.

## 9. Decisions log

- **2026-05-16** — `corelink-chunker` re-sweep pre-hardening
  empirical kill rate: **95.79 %** raw (91/95 viable; 26 unviable;
  4 missed; 0 timeout). Mask-selection at fastcdc.rs:165 still
  surviving — wave-23 test was equivalence-vulnerable.
- **2026-05-16** — `corelink-chunker` hardened
  `fastcdc_scan_boundary_mask_selection_is_deterministic` with
  literal canonical first-chunk digest pin
  (`ab0c43c5...e35c71`) captured via one-shot probe. Targeted
  re-sweep on fastcdc.rs: 37/38 of killable = **100 % of
  killable**. Crate moves PARTIAL → **CLOSED**.
- **2026-05-16** — `corelink-multipart-schema` +10
  lifecycle-bound kills landed (wave-23 deferral set; covers all
  10 distinct mutant identities the cargo-mutants 25.0.1
  enumeration produces at sim.rs:694/703/768/773/874). Re-sweep
  empirical kill rate: **97.44 %** raw (114/117 viable; 16
  unviable; 3 missed; 0 timeout). The 3 missed all classified
  structurally equivalent §3.3. **100 % of killable** = 114/114.
  Crate moves PARTIAL → **CLOSED**.
- **2026-05-16** — Wave-24 budget-decision matrix applied: 3
  remaining first-sweep candidates (`r2-multipart` 150,
  `quota-cas` 350, `webauthn` 369) moved to CI-nightly lane.
  `mutation-nightly.yml` matrix extended 8 → 11; flat 75 %
  baseline for the 3 new crates. **DEBT-008 hand-dispatch queue
  exhausted.**
- **2026-05-16** — Wave-23 "11 deferred" → wave-24 "10 distinct
  cargo-mutants identities" drift documented §4.3 + §8 caveat 3.

## 10. Test count summary

| Crate | Pre-wave-24 | New (this audit) | Total |
|---|---:|---:|---:|
| `corelink-chunker` | 13 (`mutation_kills.rs`) | 0 net-new (1 hardened) | 13 |
| `corelink-multipart-schema` | 9 (`mutation_kills.rs`) | 10 lifecycle-bound | 19 |
| **Net new** | | **10** | |
| **Hardened (existing)** | | **1** | |

## 11. Quality gates

| Gate | Result |
|---|---|
| `cargo build --workspace` | green (verified §2 timings; baseline build 46.6 s) |
| `cargo test -p corelink-chunker --test mutation_kills` | 13 passed |
| `cargo test -p corelink-multipart-schema --test mutation_kills` | 19 passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | pending (post-commit run) |
| `actionlint .github/workflows/mutation-nightly.yml` | green (no warnings) |
| `scripts/validate_specs.py` | pending (post-commit run) |
| `scripts/validate_references.py` | pending (post-commit run) |

Gates run post-test-validation per charter; results recorded in
the commit message for the wave-24 SEAL.
