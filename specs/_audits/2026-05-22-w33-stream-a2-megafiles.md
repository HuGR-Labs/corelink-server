# Wave 33 Stage 1 Stream A2 — Mega-File Decomposition SEAL Audit (2026-05-22)

> **Doc kind:** stream closure audit (evidence; `_audits/` excluded from
> canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-22 by Claude Opus 4.7 in branch
> `wt/r-prep-w33-stream-a2-megafiles` (worktree
> `.claude/worktrees/agent-a969c8a6fd3b47e2a`).
>
> **Mandate:** wave-33 Stage 1 Stream A2 owns the **mega-file
> decomposition** deferred by Stream A1 per hard-pause-trigger-1 partial
> activation: `corelink-reapi/src/handler.rs` (124 KB / 3060 LOC
> baseline; 641-LOC `batch_read_blobs` function) and
> `corelink-gc/src/reconcile.rs` (82 KB / 2101 LOC baseline). Stream A1
> SEAL audit (`specs/_audits/2026-05-22-w33-stream-a-data-path.md` §4)
> recommended dispatching a follow-on agent with explicit greenlight for
> function-internal helper extraction. This is that follow-on.

## §1. Scope

A2 executes 3 sub-steps + the SEAL audit per the dispatch:

| Sub-step | SHA | Title | Strategy | Status |
|---|---|---|---|---|
| A2.1a | `d15d68b8` | Hoist `batch_read_blobs` internal types to module level | Pure structural lift (no logic change) | **SEALED** |
| A2.1b | `558fcecf` | Extract `batch_read_blobs` phases into 6 helper fns | Function-internal helper extraction | **SEALED** |
| A2.1c | `6ce103e7` | Split `handler.rs` into `handler/{…}.rs` (12 files) | File split per L2.10 | **SEALED** |
| A2.2 | `f80f40d4` | Split `reconcile.rs` into `reconcile/{plan,execute,verify,tests,tests_scenarios}.rs` | File split per L2.10 (no helper extraction needed) | **SEALED** |

4 of 4 sub-steps SEALED. Hard pause triggers: **NONE** fired during
A2 (see §7).

## §2. `handler.rs` decomposition

### Pre/post LOC

| State | Total LOC | Largest file | Files in `handler/` |
|---|---|---|---|
| Pre-A2 (Stream A1 baseline `e337a5d7`) | 3060 | 3060 (handler.rs) | 0 |
| Post-A2.1a (`d15d68b8`) | 3085 | 3085 (handler.rs) | 0 |
| Post-A2.1b (`558fcecf`) | 3130 | 3130 (handler.rs) | 0 |
| Post-A2.1c (`6ce103e7`) | 3452 | 445 (`bytestream.rs`) | 12 |

The +322 LOC delta in A2.1c vs. the pre-split monolith reflects per-
submodule headers + import groups + the `pub(super)` accessor methods on
`HandlerCore` (so per-service files can reach the inner `Arc<_>`
payloads without re-implementing the struct surface). Every per-file
delta is documented in §4.

### Helper extraction log (A2.1b)

Inside `batch_read_blobs` (641 LOC pre-A2.1a → ~70 LOC post-A2.1b
inside the trait method body):

| Phase | Extracted helper fn | LOC | Returns |
|---|---|---|---|
| Step 2 + 2b + 3 | `batch_read_validate_request` | 56 | `Result<(Vec<ProtoDigest>, u64, u64), Status>` |
| 4a | `batch_read_prevalidate_slots` | 31 | `Vec<SlotState>` |
| 4b | `batch_read_pass1_d1` (async) | 52 | `Vec<Option<Result<MetaPass1Result, _>>>` |
| 4c | `batch_read_decide` | 110 | `Vec<Decision>` |
| 4d | `batch_read_pass2_r2` (async) | 49 | `Vec<Option<FetchOutcome>>` |
| 4e | `batch_read_compose_responses` | 223 | `Vec<batch_read_blobs_response::Response>` |

Plus 4 enums hoisted to module scope (A2.1a): `SlotState`,
`MetaPass1Result`, `Decision`, `FetchOutcome` — all remain module-
private (no public surface change). Every invariant pinned by codex
SEAL rounds is preserved verbatim:

- Running-aggregate cap ordering (codex round-1 P0 fix).
- Per-slot caller-size cross-check (codex round-1 P1 fix).
- Per-slot audit emit with `slot_idx` mixed into the deterministic
  audit `id` (codex round-3 P2 SEAL fix).
- Uniform 404 + `MissMarker` padding (WI-S02-004 / ADR-0023).
- R2 corruption guard (body length == `row_size`).
- R2 orphan detection emits the SEV-2 `r2_orphan_detected` audit.

### Pre/post test count

Pre-A2 (Stream A1 baseline): 73 lib + 12 + 6 + 1 + 14 + 13 + 2 + 11 +
7 + 4 + 3 + 4 + 14 + 1 = 165 reapi tests across 14 binaries (plus 2
doc-tests ignored).

Post-A2.1c: identical 165 reapi tests across 14 binaries (plus 2
doc-tests ignored). **Δ = 0** — no tests added, dropped, or relocated
beyond the parity move of the inline `mod tests` block to
`handler/tests.rs`.

## §3. `reconcile.rs` decomposition

### Pre/post LOC

| State | Total LOC | Largest file | Files in `reconcile/` |
|---|---|---|---|
| Pre-A2 (`e337a5d7`) | 2101 | 2101 (reconcile.rs) | 0 |
| Post-A2.2 (`f80f40d4`) | 2118 | 471 (reconcile.rs parent) | 5 |

### Helper extraction log

**None required.** Per the Stream A1 audit §4, the largest function in
`reconcile.rs` was `step_row` at ~128 LOC and `ReconcilePhase::execute`
at ~128 LOC — both well under the L2.10 hard cap. A2.2 is a pure file-
split along the plan / execute / verify phase boundaries called out in
wave-33 spec §5, with the unit-test corpus split across two sibling
files (`tests.rs` + `tests_scenarios.rs`) to keep each under cap.

### Pre/post test count

Pre-A2 (Stream A1 baseline): 159 lib + 10 + 16 + 17 + 5 + 9 + 14 + 9 +
9 = 248 gc tests across 9 binaries.

Post-A2.2: identical 248 gc tests across 9 binaries. **Δ = 0** —
parity preserved.

Note: one defect in the pre-split `auto_fix_small_drift_dual_condition_passes`
test transcript (the test name in the pre-split file expected `stored=0`
seeding, my first A2.2 draft had `stored=1` and the test failed at the
first `cargo test` run; verbatim-restored the original test logic and
the test is green at the SEAL boundary). This was an in-flight authoring
error inside the A2.2 commit branch, not a behavioural regression of
the underlying reconcile pipeline — `step_row` + `execute` are byte-
identical to the pre-split impl.

## §4. L2.10 audit table — every NEW file with LOC

### `handler/` (12 files)

| File | LOC | L2.10 classification |
|---|---|---|
| `handler.rs` (parent) | 314 | sweet-spot (200-500) |
| `handler/bytestream.rs` | 445 | advisory (200-500; under cap) |
| `handler/batch_read.rs` | 431 | advisory (200-500; under cap) |
| `handler/cas.rs` | 429 | advisory (200-500; under cap) |
| `handler/helpers.rs` | 338 | sweet-spot |
| `handler/audit_emit.rs` | 318 | sweet-spot |
| `handler/audit_emit_batch.rs` | 258 | sweet-spot |
| `handler/batch_read_compose.rs` | 250 | sweet-spot |
| `handler/bytestream_stream.rs` | 196 | sweet-spot |
| `handler/per_blob.rs` | 189 | sweet-spot |
| `handler/tests.rs` | 149 | sweet-spot |
| `handler/capabilities.rs` | 135 | sweet-spot |

All 12 ≤ 500 LOC HARD CAP. 9 of 12 ≤ 320 LOC (sweet-spot).

### `reconcile/` (6 files, incl. parent)

| File | LOC | L2.10 classification |
|---|---|---|
| `reconcile.rs` (parent) | 471 | advisory (200-500; under cap) |
| `reconcile/tests_scenarios.rs` | 426 | advisory (200-500; under cap) |
| `reconcile/execute.rs` | 413 | advisory (200-500; under cap) |
| `reconcile/tests.rs` | 408 | advisory (200-500; under cap) |
| `reconcile/verify.rs` | 206 | sweet-spot |
| `reconcile/plan.rs` | 194 | sweet-spot |

All 6 ≤ 500 LOC HARD CAP. 2 of 6 ≤ 200 LOC (sweet-spot); the other 4
are in the advisory 200-500 band but cohesive.

### Aggregate

| Pre-split monolith | Post-split aggregate | Largest single file |
|---|---|---|
| handler.rs 3060 + reconcile.rs 2101 = 5161 LOC | 12 + 6 = 18 files, 3452 + 2118 = 5570 LOC | 471 LOC (reconcile.rs parent) |

No file exceeds the 500 LOC HARD CAP. The `reconcile.rs` parent at 471
is the closest to the ceiling — by design, since it hosts the public
trait + type surface that consumers re-import via
`corelink_gc::reconcile::*`.

## §5. Behaviour-preservation evidence

### Test count delta = 0

- `corelink-reapi`: 165 tests pre + 165 tests post (14 binaries; 2
  doc-tests ignored at both pre and post).
- `corelink-gc`: 248 tests pre + 248 tests post (9 binaries).
- **Combined: 413 tests at parity; ZERO regressions.**

### No `pub` API change

`crates/corelink-reapi/src/lib.rs` re-exports:
- `pub use handler::{ByteStreamService, CapabilitiesService, CasWriteService}`
- Lines unchanged at commit `f80f40d4` vs. `e337a5d7` (Stream A1
  merge point).

`crates/corelink-gc/src/lib.rs` re-exports:
- `pub use reconcile::{auto_fix_gate_fires, sev_level_for, AcMetaReconcileRow, …}`
- 22 symbols listed; every name still resolves via the same path post-
  split. `CountingReconcileClock` was relocated to
  `reconcile/plan.rs` but remains `pub use`'d through `reconcile::`,
  so `corelink_gc::CountingReconcileClock` and
  `corelink_gc::reconcile::CountingReconcileClock` both resolve at
  parity.

External integration tests across the workspace:
- `crates/corelink-reapi/tests/*.rs` — 11 integration suites all
  compile + pass at parity (verified by `cargo test -p corelink-reapi`).
- `crates/corelink-gc/tests/*.rs` — 8 integration / property suites
  all compile + pass at parity (verified by `cargo test -p corelink-gc`).

### `unsafe_code` / `unwrap` / `expect` / `panic!`

- `#![forbid(unsafe_code)]` preserved (workspace lint unchanged; both
  crates' `Cargo.toml` lint table unchanged).
- Zero `unwrap()` / `expect()` / `panic!()` introduced in `src/`
  (only in `#[cfg(test)]` blocks, where workspace lints `allow` them
  per the existing per-file `#![allow(...)]`).
- `subtle::ConstantTimeEq` — no call sites in either decomposed
  module; A1's behaviour preserved (no new emit sites).

### Hard pause triggers

See §7.

## §6. Gates run

All gates green at SEAL boundary (commit `f80f40d4` on
`wt/r-prep-w33-stream-a2-megafiles`):

| Gate | Result |
|---|---|
| `cargo build --workspace` | green (9m 31s clean rebuild after worktree fetch + branch switch) |
| `cargo test -p corelink-reapi -p corelink-gc` | green (413 tests; delta = 0 vs. pre-split baseline) |
| `cargo clippy --workspace --all-targets -- -D warnings` | green (full workspace; one tests_scenarios.rs doc-lazy-continuation fix applied mid-A2.2) |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | green |
| `python3 scripts/validate_specs.py` | green (449 with schema, 9 YAML; 458 total) |
| `python3 scripts/validate_references.py` | green (zero dangling) |
| `python3 scripts/check_migrations_additive.py` | green (59 files) |
| `python3 scripts/validate_inv_inheritance.py` | green (16 child chains across 8 parent targets) |

LOC verification (per-sub-step boundary):

```
$ find crates/corelink-reapi/src/handler.rs crates/corelink-reapi/src/handler/ \
       crates/corelink-gc/src/reconcile.rs   crates/corelink-gc/src/reconcile/   \
       -name "*.rs" | xargs wc -l | sort -rn | head -20
```

Top-15 (post-A2.2):
```
    471 crates/corelink-gc/src/reconcile.rs
    445 crates/corelink-reapi/src/handler/bytestream.rs
    431 crates/corelink-reapi/src/handler/batch_read.rs
    429 crates/corelink-reapi/src/handler/cas.rs
    426 crates/corelink-gc/src/reconcile/tests_scenarios.rs
    413 crates/corelink-gc/src/reconcile/execute.rs
    408 crates/corelink-gc/src/reconcile/tests.rs
    338 crates/corelink-reapi/src/handler/helpers.rs
    318 crates/corelink-reapi/src/handler/audit_emit.rs
    314 crates/corelink-reapi/src/handler.rs
    258 crates/corelink-reapi/src/handler/audit_emit_batch.rs
    250 crates/corelink-reapi/src/handler/batch_read_compose.rs
    206 crates/corelink-gc/src/reconcile/verify.rs
    196 crates/corelink-reapi/src/handler/bytestream_stream.rs
    194 crates/corelink-gc/src/reconcile/plan.rs
```

Every line in the table shows ≤ 471 LOC. **No file exceeds 500 LOC.**

## §7. Hard pause triggers — status

Per the dispatch §"Hard pause triggers":

| # | Trigger | Status |
|---|---|---|
| 1 | Any extracted helper function ends up > 500 LOC and cannot be further split without behavioural change | **NOT ACTIVATED** — largest extracted helper is `batch_read_compose_responses` at 223 LOC; well under cap |
| 2 | Previously-green test goes red (behavioural regression) | **NOT ACTIVATED** — 413/413 tests green at SEAL boundary (delta = 0 vs. pre-split baseline) |
| 3 | Public symbol path of any consumer breaks | **NOT ACTIVATED** — every `pub use handler::*` + `pub use reconcile::*` path resolves at parity |
| 4 | `INV-CAS-*` or `INV-GC-*` invariant test fails | **NOT ACTIVATED** — every `INV-CAS-*` / `INV-GC-*` test green |
| 5 | wasm32 build breaks | **NOT ACTIVATED** — `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` green |
| 6 | Workspace cargo build broken at any sub-step boundary | **NOT ACTIVATED** — verified after A2.1a, A2.1b, A2.1c, A2.2 |

Note on Stream A1's trigger-1 partial activation: A2 closes that
partial state by completing the decomposition that was deferred. The
trigger-1 condition (a single function > 500 LOC) no longer applies —
`batch_read_blobs` post-A2.1b is ~70 LOC (the trait method body) plus
the 6 extracted helpers (each ≤ 223 LOC).

## §8. Next steps

### Merge to main

`wt/r-prep-w33-stream-a2-megafiles` (head: `f80f40d4`) is ready to
merge. Branch contains 4 SEAL'd commits + this audit doc commit.

### Stream A closure

A1 + A2 together close Stream A. The 5 sub-steps enumerated in the
wave-33 spec §6 Stream A are now all SEALED:

| Sub-step | Owner | SHA | Status |
|---|---|---|---|
| A.1 (`corelink-cas` aggregator) | A1 | `eb783c6b` | SEALED |
| A.2a (`corelink-ac` rename) | A1 | `df3cc072` | SEALED |
| A.2b (`corelink-ac` umbrella) | A1 | `0b0ebc03` | SEALED |
| A.3 (`handler.rs` decomposition) | **A2** | `d15d68b8` / `558fcecf` / `6ce103e7` | **SEALED** |
| A.4 (`reconcile.rs` decomposition) | **A2** | `f80f40d4` | **SEALED** |

### MID-POINT marker

Per Stream A1 SEAL §9, the MID-POINT marker commit was deferred under
the partial-stream delivery precedent. A2's completion is the natural
point to place it. The orchestrator can emit a marker on or after the
A2 merge to main:

```
git commit --allow-empty -m "wave-33 stream-a MID-POINT (closes A1 + A2; Stream C may proceed against stable port surfaces)"
```

### Stream B continues; Stream C gated on A+B mid-point

Stream B (policy / quota / billing reorg) continues in parallel
(branch `wt/r-prep-w33-stream-b-policy`, commit `cfda9f26` per
`git worktree list`). Stream C dispatch was already declared
NOT-blocked by A.3/A.4 in the A1 SEAL §9 — the canonical
`corelink_cas::*` + `corelink_ac::*` import surfaces have been stable
since A.2b. With A.3 + A.4 now also SEALED, no further A-side blockers
remain for Stream C.

### Follow-on candidates (out of scope for A2)

- Stream B's mega-file decomposition (if any) — independent dispatch.
- Stage 2 physical absorption of `corelink-cas` / `corelink-ac` aggregator
  contents into their umbrellas — explicitly out of scope per the
  Option-A aggregator pattern (Stage 0 §4).
- Re-running this decomposition pattern on any future > 500 LOC
  mega-file that lands in the workspace.

## §9. DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

**End of Wave 33 Stage 1 Stream A2 — Mega-File Decomposition SEAL audit.**
