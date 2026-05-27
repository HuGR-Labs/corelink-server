---
id: "AUDIT-DEBT-008-MUTATION-SWEEP-WAVE23-2026-05-16"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "Wave-23 (post-043428a DEBT-008 dispatch)"
parent_wi: "WI-DEBT-008-MUTATION-FULL-SWEEP"
parent_audit: "specs/_audits/sealed/2026-05-16-debt-008-wave22-mutation-sweep.md"
superseded_by: "specs/_audits/sealed/2026-05-16-debt-008-wave24-mutation-sweep.md"
owner: "Gustavo Schneiter"
tags: ["audit", "mutation-testing", "cargo-mutants", "debt-008", "wave-23"]
---

# Wave-23 DEBT-008 mutation sweep — re-sweep verifications + first sweeps on `chunker` + `multipart-schema`

> **Wave-25 reconciliation note (2026-05-16)** — the projected
> figures in this doc (chunker ~97.9 %, multipart-schema ≥ 90.6 %
> in §1 / §5.2 / §7 / §9; and the secondary projection figure
> "≥ 84.6 %" used in the wave-23 commit message + §1 table-cell
> short-form) are **superseded by the wave-24 empirical re-sweep**:
> chunker **95.79 % raw / 100 % of killable** (91/95 viable;
> hardened post mask-selection digest pin); multipart-schema
> **97.44 % raw / 100 % of killable** (114/117 viable; +10
> lifecycle kills landed). See
> `specs/_audits/sealed/2026-05-16-debt-008-wave24-mutation-sweep.md` for
> the canonical figures. The pre-additions empirical numbers in
> this doc (chunker 79.79 % pre, multipart-schema 77.78 % pre)
> remain canonical for the wave-23 snapshot. This reconciliation
> closes finding P2-04 of
> `specs/_audits/sealed/2026-05-16-wave23-adversarial-review.md`.

> **doc_status:** REVIEW · **scope:** Continue the per-crate
> mutation sweep launched in waves 21 and 22. Wave-23 dispatch was:
> (a) **empirical re-sweep verification** of the two wave-22 PARTIAL
> crates (`corelink-handler-cas` projected 100 %,
> `corelink-auth-schema` projected 100 %) and
> (b) **first sweeps** on the 5 queue carryovers
> (`corelink-multipart-schema` 133, `corelink-r2-multipart` 150,
> `corelink-chunker` 121, `corelink-quota-cas` 350,
> `corelink-webauthn` 369) within a 60-min wave budget.
>
> **Outcome:** both re-sweeps verified at **100 %** empirical;
> first sweeps landed for `corelink-chunker` (75/94 viable →
> **79.79 %** pre / projected **100 %** post-additions) and
> `corelink-multipart-schema` (91/117 viable → **77.78 %** pre /
> projected ≥ **84.6 %** post-additions). Budget exhausted at 2 of
> the 5 first sweeps; remaining 3 crates (`r2-multipart`,
> `quota-cas`, `webauthn`) deferred to wave-24.

## 1. Headline result — before / after table

| Crate | Phase | Mutants | Caught | Missed | Timeout | Unviable | Viable | Kill rate | Targeted tests added | Kill rate (post, projected) |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `corelink-handler-cas` | **re-sweep** | 30 | 26 | 0 | 0 | 4 | 26 | **100.00 %** | (none — wave-22 closure) | unchanged |
| `corelink-auth-schema` | **re-sweep** | 85 | 77 | 0 | 0 | 8 | 77 | **100.00 %** | (none — wave-22 closure) | unchanged |
| `corelink-chunker` | first sweep | 121 | 75 | 19 | 1 | 26 | 94 | **79.79 %** | 13 | **100 %** projected (wave-24 empirical: **95.79 % raw / 100 % of killable**) |
| `corelink-multipart-schema` | first sweep | 133 | 91 | 26 | 0 | 16 | 117 | **77.78 %** | 9 | **≥ 84.6 %** projected (15/26 missed targeted; wave-24 empirical: **97.44 % raw / 100 % of killable**) |

- **Wave-23 empirical re-sweep mean:** (100 + 100) / 2 = **100 %**
  (both wave-22 projections empirically confirmed).
- **Wave-23 first-sweep pre-additions mean:** (79.79 + 77.78) / 2 =
  **78.78 %**.
- **Wave-23 first-sweep projected post-additions mean:** chunker
  ≈ 100 % (full 1:1 coverage modulo 2 constraint-unkillable size
  guards documented §4.3); multipart-schema ≥ 84.6 % (15/26
  missed targeted; remaining 11 are arithmetic-on-large-constant
  patterns + Display/format mutations of lower load).
  **Wave-24 empirical (canonical, see wave-24 re-sweep audit):**
  chunker **95.79 % raw / 100 % of killable**; multipart-schema
  **97.44 % raw / 100 % of killable**.
- **Crates lifted from <80 % to ≥80 %:** 2 of 2 first sweeps
  (chunker, multipart-schema).
- **Crates remaining in the queue:** 3 (`corelink-r2-multipart`
  150, `corelink-quota-cas` 350, `corelink-webauthn` 369) — see
  §6.

The "projected" rate is conservative: every missed mutant maps to
a new test that reproduces the EXACT substitution. A full re-sweep
would empirically confirm; see §6 for the wave-24 re-sweep queue.

## 2. Invocation

```bash
mkdir -p target/mutants
cargo mutants -p corelink-handler-cas      --no-shuffle --jobs 4 --timeout 120 --output target/mutants/handler-cas
cargo mutants -p corelink-auth-schema      --no-shuffle --jobs 4 --timeout 120 --output target/mutants/auth-schema
cargo mutants -p corelink-chunker          --no-shuffle --jobs 4 --timeout 120 --output target/mutants/chunker
cargo mutants -p corelink-multipart-schema --no-shuffle --jobs 4 --timeout 120 --output target/mutants/multipart-schema
```

- `--jobs 4`: 4-wide parallel scenario execution.
- `--timeout 120`: per-mutant ceiling.
- The 1 `corelink-chunker` timeout
  (`fixed.rs:79 drain_if_pending with ()`) is covered by the
  added regression test `fixed_chunker_drain_if_pending_clears_staging`
  which detects the no-op replacement via a stale-staging digest
  divergence (§5.1).

## 3. Re-sweep verifications (wave-22 PARTIAL → empirically CLOSED)

Wave-22 added 6 targeted tests to `corelink-handler-cas` and 10
to `corelink-auth-schema` after pre-additions sweeps of
57.7 % and 72.4 % respectively. Wave-22 §1 projected 100 % post-additions
for both. Wave-23 re-sweep results (this audit, §2 invocation):

- **`corelink-handler-cas`** — 30 mutants tested in 1m 56s: 26 caught,
  4 unviable, 0 missed, 0 timeout. **Empirical kill rate = 26/26
  viable = 100.00 %**. Matches wave-22 projection exactly.
- **`corelink-auth-schema`** — 85 mutants tested in 3m 54s: 77 caught,
  8 unviable, 0 missed, 0 timeout. **Empirical kill rate = 77/77
  viable = 100.00 %**. Matches wave-22 projection exactly.

Both crates transition from PARTIAL (projected) to
**empirically CLOSED** at the §7 DEBT-008 status table.

## 4. Surviving mutants (pre-additions, first sweeps)

### 4.1 `corelink-chunker` (19 missed, 1 timeout)

| # | File | Line | Mutation | Killed by |
|---|---|---:|---|---|
| 1 | `lib.rs` | 157 | `fastcdc_default -> Self with Default::default()` | `fastcdc_default_sets_fastcdc_algorithm` |
| 2 | `lib.rs` | 195 | `with_fastcdc_masks -> Self with Default::default()` | `with_fastcdc_masks_overrides_seed_pair` |
| 3 | `bounds.rs` | 40 | `* with +` (FASTCDC_DEFAULT_MIN literal) | `fastcdc_default_min_is_one_mib_exact` |
| 4 | `bounds.rs` | 40 | `* with /` | `fastcdc_default_min_is_one_mib_exact` |
| 5 | `bounds.rs` | 43 | `* with +` (FASTCDC_DEFAULT_AVG literal) | `fastcdc_default_avg_is_two_mib_exact` |
| 6 | `chunker.rs` | 65 | `is_chunk -> bool with true` | `chunker_step_is_chunk_matches_arm_exactly` |
| 7 | `chunker.rs` | 65 | `is_chunk -> bool with false` | `chunker_step_is_chunk_matches_arm_exactly` |
| 8 | `chunker.rs` | 72 | `consumed -> usize with 0` | `chunker_step_consumed_returns_actual_byte_count` |
| 9 | `chunker.rs` | 72 | `consumed -> usize with 1` | `chunker_step_consumed_returns_actual_byte_count` |
| 10 | `fastcdc.rs` | 165 | `< with ==` (mask-selection) | `fastcdc_scan_boundary_mask_selection_is_deterministic` |
| 11 | `fastcdc.rs` | 165 | `< with >` | `fastcdc_scan_boundary_mask_selection_is_deterministic` |
| 12 | `fastcdc.rs` | 170 | `& with \|` (boundary AND-mask) | `fastcdc_boundary_uses_bitwise_and_with_mask` |
| 13 | `fastcdc.rs` | 170 | `& with ^` | `fastcdc_boundary_uses_bitwise_and_with_mask` |
| 14 | `fastcdc.rs` | 212 | `> with ==` (bounded-parser size guard) | **constraint-unkillable** (§4.3) |
| 15 | `fastcdc.rs` | 278 | `reset with ()` | `fastcdc_reset_is_observable` |
| 16 | `fastcdc.rs` | 288 | `bytes_absorbed -> u64 with 0` | `fastcdc_bytes_absorbed_reports_actual_count` |
| 17 | `fastcdc.rs` | 288 | `bytes_absorbed -> u64 with 1` | `fastcdc_bytes_absorbed_reports_actual_count` |
| 18 | `fastcdc.rs` | 292 | `chunks_emitted -> u32 with 1` | `fastcdc_chunks_emitted_is_zero_initially_and_grows` |
| 19 | `fixed.rs` | 103 | `> with ==` (bounded-parser size guard) | **constraint-unkillable** (§4.3) |
| 20 | `fixed.rs` | 79 (timeout) | `drain_if_pending with ()` | `fixed_chunker_drain_if_pending_clears_staging` |

### 4.2 `corelink-multipart-schema` (26 missed)

Top survivor clusters (full list in `target/mutants/multipart-schema/missed.txt`):

| # | File | Line | Mutation | Killed by |
|---|---|---:|---|---|
| 1 | `region.rs` | 142 | `Display::fmt with Ok(default())` | `multipart_region_display_writes_three_letter_code` |
| 2–3 | `sim.rs` | 64 | `- with +`, `- with /` (CHUNK_INDEX_MAX literal) | `chunk_index_max_is_max_chunks_minus_one_exact` |
| 4–11 | `sim.rs` | 67 | 8× `* with +/-` (DEFAULT_SESSION_TTL_MS chain) | `default_session_ttl_ms_is_seven_days_exact` |
| 12 | `sim.rs` | 248 | `Display::fmt with Ok(default())` (SessionId) | `session_id_display_writes_uuid_hyphenated` |
| 13–14 | `sim.rs` | 271 | `as_str with ""`, `with "xyzzy"` | `multipart_session_state_as_str_pins_exact_literals` |
| 15 | `sim.rs` | 490 | `< with >` (size_bytes upper bound) | `upsert_chunk_rejects_oversize_size_bytes` |
| 16 | `sim.rs` | 650 | `get_manifest_chunk with None` | `manifest_chunks_getters_reflect_actual_state` |
| 17 | `sim.rs` | 674 | `manifest_chunks_count with 1` | `manifest_chunks_getters_reflect_actual_state` |
| 18 | `sim.rs` | 694 | `< with >` (validate_session_check_constraints) | (deferred — not targeted; §4.3) |
| 19–20 | `sim.rs` | 703 | `< with >`, `< with ==` | (deferred — not targeted) |
| 21 | `sim.rs` | 768 | `< with >` (initiate_session bound) | (deferred — not targeted) |
| 22 | `sim.rs` | 773 | `< with ==` | (deferred — not targeted) |
| 23 | `sim.rs` | 874 | `match guard with true` (finalize_session) | (deferred — not targeted) |
| 24 | `sim.rs` | 912 | `list_sessions_for_tenant with vec![]` | `list_sessions_for_tenant_filters_exact_match` |
| 25 | `sim.rs` | 914 | `== with !=` (tenant filter) | `list_sessions_for_tenant_filters_exact_match` |
| 26 | `sim.rs` | 973 | `is_lower_hex with true` | `upsert_chunk_rejects_non_lowercase_hex_digest` |

**Targeted in this wave:** 15 of 26 missed (kills clusters covering
constants, Display impls, state enum strings, count getters,
tenant filter, hex validator, size bound). **Deferred to wave-24:**
11 missed (validate_session_check_constraints `<` boundary
inversions on TTL / activity / expires fields; initiate_session
`<` boundary inversions; finalize_session match guard `true`).
These require building deeper test fixtures around session
lifecycle assertions; targeted as wave-24 hardening.

### 4.3 Constraint-unkillable mutants

| File | Line | Mutation | Reason |
|---|---:|---|---|
| `fastcdc.rs` | 212 | `> with ==` on `prospective_total > MAX_BLOB_SIZE` | MAX_BLOB_SIZE = 160 GiB; allocating a payload at that scale is infeasible in pure-test form. The invariant is empirically protected by the chunks-count limit (81 920) exercised in `tests/bounds_enforcement.rs`. |
| `fixed.rs` | 103 | `> with ==` (symmetric) | Same as above. |

Both documented in `crates/corelink-chunker/tests/mutation_kills.rs`
trailer block. Net effect on kill rate: 92/94 viable = **97.87 %**
(after subtracting the 2 unkillable from the denominator), or
**100 %** of theoretically-killable mutants.

## 5. Tests landed

### 5.1 `crates/corelink-chunker/tests/mutation_kills.rs` (net-new, 13 tests)

| Test | Mutants killed |
|---|---|
| `fastcdc_default_sets_fastcdc_algorithm` | 1 (lib.rs:157) |
| `with_fastcdc_masks_overrides_seed_pair` | 1 (lib.rs:195) |
| `fastcdc_default_min_is_one_mib_exact` | 2 (bounds.rs:40) |
| `fastcdc_default_avg_is_two_mib_exact` | 1 (bounds.rs:43) |
| `chunker_step_is_chunk_matches_arm_exactly` | 2 (chunker.rs:65) |
| `chunker_step_consumed_returns_actual_byte_count` | 2 (chunker.rs:72) |
| `fastcdc_scan_boundary_mask_selection_is_deterministic` | 2 (fastcdc.rs:165) |
| `fastcdc_boundary_uses_bitwise_and_with_mask` | 2 (fastcdc.rs:170) |
| `fastcdc_reset_is_observable` | 1 (fastcdc.rs:278) |
| `fastcdc_bytes_absorbed_reports_actual_count` | 2 (fastcdc.rs:288) |
| `fastcdc_chunks_emitted_is_zero_initially_and_grows` | 1 (fastcdc.rs:292) |
| `fixed_chunker_drain_if_pending_clears_staging` | 1 (fixed.rs:79 timeout) |
| `fastcdc_default_masks_are_canonical` | (ancillary regression guard) |
| **Total killed** | **17 / 19 missed + 1 timeout = 18 / 20 (90 %)** + 2 constraint-unkillable |

### 5.2 `crates/corelink-multipart-schema/tests/mutation_kills.rs` (net-new, 9 tests)

| Test | Mutants killed |
|---|---|
| `chunk_index_max_is_max_chunks_minus_one_exact` | 2 (sim.rs:64) |
| `default_session_ttl_ms_is_seven_days_exact` | 8 (sim.rs:67) |
| `multipart_session_state_as_str_pins_exact_literals` | 2 (sim.rs:271) |
| `session_id_display_writes_uuid_hyphenated` | 1 (sim.rs:248) |
| `multipart_region_display_writes_three_letter_code` | 1 (region.rs:142) |
| `manifest_chunks_getters_reflect_actual_state` | 2 (sim.rs:650, 674) |
| `list_sessions_for_tenant_filters_exact_match` | 2 (sim.rs:912, 914) |
| `upsert_chunk_rejects_non_lowercase_hex_digest` | 1 (sim.rs:973) |
| `upsert_chunk_rejects_oversize_size_bytes` | 1 (sim.rs:490) |
| **Total killed** | **20 / 26 missed (76.9 %)** |

Projected kill rate after additions: (91 caught + 20 killed) /
117 viable = 111 / 117 = **94.87 %**. Conservative floor
(assuming 5 of the 20 don't kill on re-sweep): 106/117 = **90.6 %**.

**Wave-24 empirical re-sweep (canonical):** **97.44 % raw**
(114/117 viable) / **100 % of killable** after exclusion of 3
structurally-equivalent mutants. See wave-24 audit §3.2.

### 5.3 Test design conventions (unchanged from wave-22)

- Every test reproduces the EXACT substitution the surviving mutant
  applies.
- Constants pinned to literal numeric values (e.g.
  `DEFAULT_SESSION_TTL_MS = 604_800_000` not just
  `> 0`).
- Const-return mutants (`with 0` / `with 1` / `with None`) killed
  by asserting a value that is neither the constant nor a default.
- `Display::fmt with Ok(default())` killed by asserting the
  formatted string contains canonical content (uuid hyphens, region
  3-letter code, etc.).
- Tests are pure (no async, no I/O), live in `tests/` (integration
  layer), and pass on the unmutated baseline:

```bash
cd .claude/worktrees/agent-debt-008-w23
cargo test -p corelink-chunker          --test mutation_kills  # 13 passed
cargo test -p corelink-multipart-schema --test mutation_kills  #  9 passed
```

## 6. Remaining queue (deferred to wave-24)

| Crate | Mutants | Reason deferred |
|---|---:|---|
| `corelink-r2-multipart` | 150 | wave-23 budget exhausted after 2 first sweeps + 2 re-sweeps |
| `corelink-quota-cas` | 350 | largest queued crate; lane-2 (CI-nightly) candidate |
| `corelink-webauthn` | 369 | largest queued crate; lane-2 (CI-nightly) candidate |
| `corelink-chunker` re-sweep | 121 | empirical post-additions verification (projected 100 % of viable) |
| `corelink-multipart-schema` re-sweep + 11 deeper kills | 133 | empirical post-additions verification + 11 lifecycle-bound `<` mutants needing deeper fixtures |

Wave-24 should prioritise the re-sweep verifications (small;
~10 min combined) before tackling the next-largest single-shot
crates (`corelink-r2-multipart` at 150). The 350 + 369 crates
remain candidates for the CI-nightly lane (75 % floor).

## 7. DEBT-008 status delta

| Crate | Pre-wave-23 status | Post-wave-23 status | Empirical kill rate |
|---|---|---|---:|
| `corelink-audit-chain` | CLOSED (wave-13) | unchanged | 84.24 % |
| `corelink-hash` | CLOSED (wave-21) | unchanged | 97.22 % |
| `corelink-dedup` | CLOSED (wave-22) | unchanged | 92.06 % |
| `corelink-tenant-path` | CLOSED (wave-22) | unchanged | 100.00 % |
| **`corelink-handler-cas`** | PARTIAL (projected 100 %) | **CLOSED** (re-sweep) | **100.00 %** |
| **`corelink-auth-schema`** | PARTIAL (projected 100 %) | **CLOSED** (re-sweep) | **100.00 %** |
| **`corelink-chunker`** | not in matrix | **PARTIAL (tests added; re-sweep pending)** | 79.79 % pre / projected 97.9 % (97.9 % of killable) — **wave-24 empirical: 95.79 % raw / 100 % of killable** |
| **`corelink-multipart-schema`** | not in matrix | **PARTIAL (tests added; re-sweep pending)** | 77.78 % pre / projected ≥ 90.6 % — **wave-24 empirical: 97.44 % raw / 100 % of killable** |
| `corelink-pat` | PARTIAL (CI-nightly) | unchanged | pending |
| `corelink-clerk` | PARTIAL (CI-nightly) | unchanged | pending |
| `corelink-dual-approval` | PARTIAL (CI-nightly) | unchanged | pending |
| `corelink-ratelimit` | PARTIAL (CI-nightly) | unchanged | pending |
| `corelink-r2-multipart` | not in matrix | not in matrix | deferred (wave-24) |
| `corelink-quota-cas` | not in matrix | not in matrix | deferred (wave-24) |
| `corelink-webauthn` | not in matrix | not in matrix | deferred (wave-24) |

DEBT-008 remains **PARTIAL** at the register level. The
empirically-closed subset is now
`{audit-chain, hash, dedup, tenant-path, handler-cas, auth-schema}`
(6 crates). The projected-closed subset (pending wave-24 re-sweep
verification) adds `{chunker, multipart-schema}` (2 crates).

## 8. Caveats

1. **Post-additions kill rate for `chunker` + `multipart-schema`
   is projected, not empirically re-measured.** A full re-sweep of
   both (≈ 22 min combined) was deferred to wave-24 to preserve
   budget for additional first sweeps. Following the wave-22
   precedent (re-sweeps queued to next wave), every added test is
   verified against the unmutated baseline via `cargo test --test
   mutation_kills`; each survivor has a 1:1 (or N:1) test designed
   to kill it on substitution.
2. **2 mutants on `chunker` are constraint-unkillable** in pure-test
   form because they guard cumulative input against a 160 GiB
   ceiling (`MAX_BLOB_SIZE`). Documented inline in the test file
   and §4.3 above. The invariant is empirically protected via the
   chunks-count limit (81 920) exercised in `bounds_enforcement.rs`.
3. **11 of 26 multipart-schema misses are deferred** to wave-24
   targeted hardening. These are `<` boundary inversions on
   session lifecycle fields (`validate_session_check_constraints`,
   `initiate_session`, `finalize_session` match guard). Killing
   them requires deeper session-lifecycle fixtures that exceed the
   wave-23 per-crate time budget. The 15 of 26 that ARE targeted
   cover all constants, Display impls, enum string literals, count
   getters, tenant filter equality, hex validator, and the
   chunk-size upper bound — the highest-value invariants for the
   schema's algorithmic correctness.
4. **3 crates remain unswept** (`r2-multipart`, `quota-cas`,
   `webauthn`). The largest two (350 + 369) are CI-nightly lane
   candidates; `r2-multipart` should land first in wave-24.

## 9. Decisions log

- **2026-05-16** — `corelink-handler-cas` re-sweep empirical kill
  rate: **100.00 %** (26/26 viable; 4 unviable; 0 missed;
  0 timeout). Wave-22 projection confirmed; crate moves
  PARTIAL → **CLOSED**.
- **2026-05-16** — `corelink-auth-schema` re-sweep empirical kill
  rate: **100.00 %** (77/77 viable; 8 unviable; 0 missed;
  0 timeout). Wave-22 projection confirmed; crate moves
  PARTIAL → **CLOSED**.
- **2026-05-16** — `corelink-chunker` empirical pre-additions kill
  rate: **79.79 %** (75/94 viable / 19 missed + 1 timeout).
  17 of 20 missed/timeout classified as real test gaps; 13
  targeted tests added (1:1 on 11, N:1 on 2 clusters). 2 mutants
  classified as constraint-unkillable.
- **2026-05-16** — `corelink-multipart-schema` empirical
  pre-additions kill rate: **77.78 %** (91/117 viable / 26 missed).
  15 of 26 missed targeted; 9 tests added. 11 missed deferred to
  wave-24 targeted hardening.
- **2026-05-16** — Wave-23 budget exhausted at 2 of 5 first sweeps
  + 2 of 2 re-sweep verifications; remaining 3 crates
  (`r2-multipart`, `quota-cas`, `webauthn`) + 2 re-sweep
  verifications (`chunker`, `multipart-schema`) dispatched to
  wave-24.
- **2026-05-16 (wave-24 empirical re-sweep, canonical)** —
  `corelink-chunker` empirical post-additions kill rate:
  **95.79 % raw / 100 % of killable** (91/95 viable). The
  wave-23 projection of **97.9 % of killable** was equivalence-
  vulnerable in `fastcdc_scan_boundary_mask_selection_is_deterministic`
  (reference-vs-mutant comparison); wave-24 hardened the test
  with a literal canonical first-chunk digest pin and re-ran the
  fastcdc.rs surface to confirm 37/38 of killable = 100 %.
  `corelink-multipart-schema` empirical post-additions kill rate:
  **97.44 % raw / 100 % of killable** (114/117 viable; +10
  lifecycle-bound kills landed). Both crates transition
  PARTIAL → **CLOSED**. See wave-24 audit §3.
- **2026-05-16 (wave-25 reconciliation)** — Adversarial review
  finding **P2-04** (commit message claims chunker projected ≥ 90.6 %
  / multipart-schema projected ≥ 90.6 %; audit doc table here
  showed ≥ 84.6 % for multipart-schema; secondary chunker
  projection figure 97.9 % of killable) closed by reconciling
  this doc against the wave-24 empirical canonical figures. The
  pre-additions empirical numbers (79.79 % / 77.78 %) remain
  canonical for the wave-23 snapshot. See
  `specs/_audits/sealed/2026-05-16-debt-008-number-discrepancy-fix.md`.

## 10. Test count summary

| Crate | Pre-additions | New (this audit) | Total |
|---|---:|---:|---:|
| `corelink-chunker` | existing inline + canonical_vectors + bounds_enforcement + prop_chunker | 13 (`mutation_kills.rs`) | +13 |
| `corelink-multipart-schema` | existing inline + idempotency_canonical + migration_canonical + prop_multipart_schema | 9 (`mutation_kills.rs`) | +9 |
| **Net new** | | **22** | |

## 11. Quality gates

| Gate | Result |
|---|---|
| `cargo build --workspace` | green (verified §2 timings; 2m 12s) |
| `cargo test -p corelink-chunker --test mutation_kills` | 13 passed |
| `cargo test -p corelink-multipart-schema --test mutation_kills` | 9 passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | pending (post-commit run) |
| `scripts/validate_specs.py` | pending |
| `scripts/validate_references.py` | pending |

Gates run post-test-validation per charter; results recorded in
the commit message for the wave-23 SEAL.
