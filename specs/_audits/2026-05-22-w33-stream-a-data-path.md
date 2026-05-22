# Wave 33 Stage 1 Stream A — Data Path SEAL Audit (2026-05-22)

> **Doc kind:** stream closure audit (evidence; `_audits/` excluded from
> canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-22 by Claude Opus 4.7 in branch
> `wt/r-prep-w33-stream-a-data-path` (worktree
> `.claude/worktrees/agent-a8c7fe8aec535c4ed`).
>
> **Mandate:** wave-33 Stage 1 Stream A owns the **data path**
> bounded-contexts (`corelink-cas`, `corelink-ac`) plus the
> mega-file decomposition of `corelink-reapi/src/handler.rs` (124 KB)
> and `corelink-gc/src/reconcile.rs` (82 KB). Charter:
> `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §6 Stream A;
> Stage 0 foundation lessons:
> `specs/_audits/2026-05-22-w33-stage0-foundation.md` §4 + §7.

## §1. Scope

Stream A executes 5 sub-steps as planned per the wave-33 spec:

| Sub-step | SHA | Title | Strategy | Status |
|---|---|---|---|---|
| A.1 | `eb783c6b` | `corelink-cas` (10 → 1 aggregator) | Option-A aggregator (re-export façade) | **SEALED** |
| A.2a | `df3cc072` | `corelink-ac` → `corelink-ac-core` rename | Pre-umbrella rename (Option-A1) | **SEALED** |
| A.2b | `0b0ebc03` | `corelink-ac` umbrella (3 absorbed) | Option-A aggregator | **SEALED** |
| A.3 | (deferred) | `corelink-reapi/handler.rs` decomposition (124 KB / 3060 LOC) | — | **DEFERRED — hard pause trigger #1** |
| A.4 | (deferred) | `corelink-gc/reconcile.rs` decomposition (82 KB / 2101 LOC) | — | **DEFERRED — orchestrator decision required** |

3 of 5 sub-steps SEALED; 2 deferred via the wave-33 spec §7
hard-pause-trigger-1 mechanism documented in §7 below. Stage 0
established the precedent for partial-delivery streams when a hard
pause trigger fires (see Stage 0 SEAL §7 trigger-1 partial
activation).

## §2. Crates created + restructured

| Crate | Disposition | Cargo deps | LOC (src/) | Test count |
|---|---|---|---|---|
| `corelink-cas` | NEW (aggregator) | core, crypto, audit + 10 absorbed | 137 (lib.rs) + 10×7 LOC submodules + eviction.rs 30 LOC | 10 smoke |
| `corelink-ac` | NEW (aggregator) | core, crypto, audit + 3 absorbed | 96 (lib.rs) + 3×7 LOC submodules | 4 smoke |
| `corelink-ac-core` | RENAMED (was `corelink-ac`) | unchanged | unchanged | unchanged (61 unit + 6×integration suites) |

Workspace member delta: **110 → 112** (`+corelink-cas`,
`+corelink-ac` umbrella; `corelink-ac` package-name reused for the
umbrella, original crate renamed to `corelink-ac-core`).

Net `crates/` directory delta: `crates/corelink-ac/` (umbrella) +
`crates/corelink-ac-core/` (renamed original) where previously there
was a single `crates/corelink-ac/` (original codec). Zero crates
removed (Option-A aggregator pattern keeps absorbed crates as
canonical sources; physical absorption deferred to Stage 2).

### Aggregator surface

Per `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §4 Crate
Mapping, Stream A's two contexts now have canonical import surfaces:

```
corelink_cas::chunker::*           # was corelink_chunker::*
corelink_cas::dedup::*             # was corelink_dedup::*
corelink_cas::edge::*              # was corelink_edge::*
corelink_cas::eviction::*          # was corelink_eviction::*
corelink_cas::lru_tracker::*       # was corelink_lru_tracker::*
corelink_cas::r2_multipart::*      # was corelink_r2_multipart::*
corelink_cas::multipart_schema::*  # was corelink_multipart_schema::*
corelink_cas::meta::*              # was corelink_meta::*
corelink_cas::manifest::*          # was corelink_manifest::*
corelink_cas::handler::*           # was corelink_handler_cas::*

corelink_ac::core::*               # was corelink_ac::*
                                   # (now corelink_ac_core::*)
corelink_ac::schema::*             # was corelink_ac_schema::*
corelink_ac::handler::*            # was corelink_handler_ac::*
corelink_ac::*                     # historical top-level path
                                   # preserved via top-level
                                   # `pub use corelink_ac_core::*`
```

Every original crate path still resolves (aggregator pattern is purely
additive). Stage 2 will physically absorb sources once consumers have
migrated.

## §3. Stream A absorption strategy — Option-A aggregator + Option-A1 rename

Per Stage 0 SEAL §4, the **Option-A aggregator interpretation** is
the canonical Stage 1 pattern: the new umbrella crate `pub use`s each
absorbed crate at the canonical submodule path while the originals
remain the source of truth unchanged. Rationale (preserved verbatim
from Stage 0 §4):

1. **Behaviour preservation**: zero risk of breaking
   cbindgen/cdylib/fuzz/bench/example surfaces on the CAS hot path.
2. **Charter rule "Behavior-preserving refactor ONLY"**: physical
   relocation across 10 + 3 = 13 crates with rich test corpora was
   assessed as exceeding the safety margin a single Stream A pass
   can hold.
3. **Stage 2 atomic ownership**: physical absorption coordinated with
   consumer rewrites lands atomically once all Stage 1 streams merge.

For `corelink-ac`, an additional **Option-A1 rename** was required to
resolve the umbrella vs. original name collision: the original
`corelink-ac` (Merkle codec + dual-side verifier) was renamed to
`corelink-ac-core` in sub-step A.2a (commit `df3cc072`), freeing the
`corelink-ac` package name for the umbrella aggregator landing in
A.2b (commit `0b0ebc03`). The new umbrella's top-level
`pub use corelink_ac_core::*` preserves the historical
`use corelink_ac::{MerkleVerifier, sig, …}` import paths so existing
consumers need zero source-line edits.

## §4. `handler.rs` + `reconcile.rs` decomposition — DEFERRED

### A.3 (handler.rs, 124 KB / 3060 LOC) — hard pause trigger #1 ACTIVATED

Inspection of `crates/corelink-reapi/src/handler.rs` revealed the
following structure (line ranges from `99269ed0` baseline; identical
on the Stream A branch):

| Region | Lines | LOC | Notes |
|---|---|---|---|
| Module preamble + use stmts | 1–77 | 77 | |
| `READ_CHUNK_SIZE_BYTES` const + `Clock` trait + `SystemClock` impl | 78–127 | 50 | |
| `HandlerCore` struct + impls | 128–257 | 130 | |
| `CasWriteService` struct + Debug + Clone + new + into_server | 258–328 | 71 | |
| `CasWriteService` impl `ContentAddressableStorage::batch_update_blobs` | 329–419 | 91 | |
| **`CasWriteService` impl `ContentAddressableStorage::batch_read_blobs`** | **420–1061** | **641** | **OVER L2.10 hard cap (500 LOC) as single function.** |
| `CasWriteService` impl `ContentAddressableStorage::find_missing_blobs` | 1062–1219 | 158 | |
| `CapabilitiesService` struct + impl `Capabilities` | 1220–1348 | 129 | |
| `ByteStreamService` struct + impl `ByteStream` (read + write + query_write_status) | 1349–1755 | 407 | |
| `process_one_blob` + `per_blob_failure` helpers | 1756–1914 | 159 | |
| gRPC plumbing helpers + audit-emit helpers | 1915–2890 | 976 | |
| Tests | 2891–3060 | 170 | |

The wave-33 spec §7 hard-pause-trigger-1 fires:

> 1. handler.rs decomposition reveals a single function > 500 LOC that
>    cannot be split without refactor.

`batch_read_blobs` is a 641-LOC single function. The dispatch's
techlead L2.10 hard cap (500 LOC) on every NEW `.rs` file means a
naive cut-paste of the function into its own file (`handler/cas/batch_read.rs`)
hard-caps. Safe decomposition requires **function-internal helper
extraction** — extracting the 5 logical phases (`4a` digest
validation → `4b` D1 size lookup → `4c` per-slot decision →
`4d` R2 body fetch → `4e` response composition) into module-private
helper free functions, each ≤ 200 LOC.

Function-internal helper extraction IS behavior-preserving (the
charter's "Behavior-preserving refactor ONLY" rule permits it), but
the wave-33 spec §7 trigger #1 was authored precisely to require an
Owner approval gate before such a refactor lands inside a single
sub-step. Per charter:

> On any trigger: HALT and escalate to Owner if [#1] handler.rs
> decomposition reveals a single function > 500 LOC that cannot be
> split without refactor. … On any trigger: STOP, document in current
> sub-step audit doc § for triggers, escalate. Do NOT auto-recover.

The Stage 0 SEAL §7 established the partial-activation precedent: a
hard-pause trigger may fire, the work proceeds with the in-scope
delivery, and the deferred work is flagged for the next decision gate.

**Recommendation (a) — accept Stream A as 3/5 sub-steps SEALED with
A.3 + A.4 explicitly deferred**:

The architectural goal of Stream A — establishing the
`corelink_cas::*` and `corelink_ac::*` bounded-context import
surfaces so Stream C can dispatch against stable ports — is fully
delivered by A.1 + A.2a + A.2b. The mega-file decomposition is
internal to `corelink-reapi` + `corelink-gc` (which retain their
existing crate identities); it does NOT block Stream C dispatch nor
Stage 2 consolidation.

**Recommendation (b) — dispatch a follow-on agent with explicit
greenlight for function-internal helper extraction**:

The follow-on agent owns the decomposition of `handler.rs` (5
helpers extracted; 6 new files: `handler/{mod,cas/{mod,batch_update,
batch_read/{mod,validate,d1_pass,decide,r2_pass,compose},find_missing},
capabilities,bytestream,helpers}`) + `reconcile.rs` (4 new files:
`reconcile/{mod,plan,execute,verify}.rs` per spec §5). Each file ≤
500 LOC HARD CAP; aim ≤ 200 LOC sweet spot. The agent operates with
explicit budget for the function-internal helper extraction +
re-validation that every `INV-CAS-*` and `INV-GC-*` test stays green.

### A.4 (reconcile.rs, 82 KB / 2101 LOC) — bundled deferral

Inspection of `crates/corelink-gc/src/reconcile.rs` shows the
file's largest single function is `step_row` (lines 1016–1143, 128
LOC) and the largest impl is `ReconcilePhase for
InMemoryReconcilePhase` (`execute` at lines 1155–1282, 128 LOC). No
hard-pause-trigger-1 activation here — the file is genuinely
splittable without function-internal refactor along the spec §5
plan/execute/verify boundaries:

- `reconcile/plan.rs` — config + `sev_level_for` + `auto_fix_gate_fires`
  (~290 LOC).
- `reconcile/execute.rs` — `step_row` + `ReconcilePhase` impl
  (~270 LOC).
- `reconcile/verify.rs` — InMemory store helpers + conditional
  `set_refcount` (~250 LOC).
- `reconcile/mod.rs` — re-exports + shared types (~300 LOC, including
  `RefcountSource`/`BlobMetaRefcountStore` traits + materialised row
  types + `ReconcileDecision`/`ReconcileResult`/`SevLevel`/
  `ReconcileError`/`ReconcileConfig`).
- Tests (~800 LOC) — leave in their original `#[cfg(test)] mod tests`
  module attached to `reconcile/mod.rs`.

A.4 is decoupled from A.3's trigger-1 escalation. To minimise the
"two-state confusion" risk during the Stream A → Owner decision gate
window, A.4 is bundled with A.3's deferral so that the follow-on
agent owning the mega-file decomposition handles both within a single
atomic dispatch. This keeps the Stream A merge boundary clean (no
half-decomposed mega-files leaking from A.4 while A.3 is pending).

If the Owner prefers Recommendation (a) (accept partial), A.4 SHOULD
be dispatched as its own follow-on once the Stream A merge lands —
splitting A.4 alone is a safer, lower-coordination operation than
splitting A.3 because no function-internal refactor is required.

## §5. `AuditEmitter` adoption status

Stream A's mandate (per dispatch §"Hard constraints"):

> Audit fail-CLOSED preserved; if you introduce a new audit emit
> site, use `corelink_audit::ports::AuditEmitter::emit`
> (Result-returning, fail-CLOSED at type level).

Stream A introduced **zero new audit emit sites**. The Option-A
aggregator pattern is purely additive (re-exports only); the existing
emit sites in absorbed crates (e.g.
`corelink_handler_cas::AuditSink`, `corelink_handler_ac::AuditSink`)
remain unchanged at their original call paths.

The AuditEmitter trait surface (Stage 0 §3) IS consumed by the new
umbrella crates at the `[dependencies] corelink-audit = { workspace = true }`
seam (so consumers writing `use corelink_cas::audit;` could in
principle reach `corelink_audit::ports::AuditEmitter` through a
single-hop import), but no Stream A code WRITES through that trait
yet. The consumer migration (replace `AuditSink`→`AuditEmitter`)
remains explicit Stage 1+ work per Stage 0 §3 Migration Plan.

## §6. Test count delta per affected crate

Pre-Stream-A baseline captured at `5d3d32c9` (Stage 0 SEAL):

| Crate | Pre-Stream-A tests | Post-Stream-A tests | Δ |
|---|---|---|---|
| `corelink-cas` | (did not exist) | 10 smoke | +10 |
| `corelink-ac` | (was: corelink-ac codec crate) | 4 smoke | new aggregator |
| `corelink-ac-core` | (was: `corelink-ac` 61+6+6+9+11+10+52 ≈ 155) | unchanged | 0 (renamed; tests follow the crate) |
| `corelink-ac-schema` | unchanged | unchanged | 0 |
| `corelink-handler-ac` | unchanged | unchanged | 0 |
| `corelink-handler-cas` | unchanged | unchanged | 0 |
| `corelink-chunker` ... `corelink-manifest` | unchanged | unchanged | 0 |
| `corelink-worker` (consumer of renamed AC) | unchanged | unchanged | 0 (Cargo.toml dep + 2 `use` stmts updated; tests pass at parity) |
| `corelink-manifest` (consumer of renamed AC) | unchanged | unchanged | 0 (Cargo.toml dep + ~10 `use` stmts in src/tests/examples updated; tests pass at parity) |

**Net Δ: +14 new smoke tests; ZERO regressions.** Hard pause trigger
#2 ("CAS path regression") NOT activated — every `INV-CAS-*` and
`INV-AC-*` test that was green pre-Stream-A is green post-Stream-A
(the only changes are aggregator re-exports + cargo dep renames; no
algorithmic changes).

## §7. Hard pause triggers — status

Per `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §7 +
dispatch §"Hard pause triggers":

| # | Trigger | Status |
|---|---|---|
| 1 | handler.rs decomposition reveals a single function > 500 LOC that cannot be split without refactor | **ACTIVATED — partial: A.3 deferred. `batch_read_blobs` is 641 LOC. Safe split requires function-internal helper extraction (5 phase helpers + Decision/SlotState/FetchOutcome enums hoisted to module scope). Recommendation: dispatch a follow-on agent with explicit greenlight; see §4 above.** |
| 2 | Test was previously green and is now red (CAS path regression) | NOT ACTIVATED |
| 3 | `INV-CAS-*` invariants broken | NOT ACTIVATED (zero algorithmic changes; aggregator re-exports only) |
| 4 | wasm32 build (`cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf`) breaks | NOT ACTIVATED (re-verified at every sub-step boundary; wave-26 `getrandom_backend="wasm_js"` fix preserved) |
| 5 | Workspace cargo build broken at any sub-step boundary | NOT ACTIVATED (verified after A.1, A.2a, A.2b) |
| 6 | Cargo.lock chaos (multiple resolution changes you can't isolate) | NOT ACTIVATED |

### Trigger #1 — partial activation detail

The wave-33 spec §6 Stream A scope explicitly enumerates four
sub-steps (cas + ac aggregators + handler.rs decompose + reconcile.rs
decompose). The dispatch's L2.10 hard cap (500 LOC) + the empirical
640+-LOC `batch_read_blobs` function creates the trigger-1 condition.

The Stage 0 SEAL §7 precedent (trigger-1 partial activation for
`corelink-client-verify` FFI-aware physical absorption) is followed
here: the trigger is documented + recommendation surfaced + the
Owner approves the deferral OR re-dispatches with explicit
greenlight. Stage 0 §7 explicitly authorised partial-stream delivery
when a hard trigger fires.

## §8. Gates — full output

### Build

- `cargo build -p corelink-cas`: green at A.1 (`eb783c6b`).
- `cargo build -p corelink-ac`: green at A.2b (`0b0ebc03`).
- `cargo build --workspace`: green at A.1, A.2a, A.2b (3/3 sub-step
  boundaries).

### Clippy (workspace-wide, -D warnings)

- `cargo clippy -p corelink-cas --all-targets -- -D warnings`: clean
  at A.1.
- `cargo clippy -p corelink-ac-core -p corelink-manifest -p corelink-worker
  --all-targets -- -D warnings`: clean at A.2a.
- `cargo clippy -p corelink-ac --all-targets -- -D warnings`: clean
  at A.2b.

### Test

- `cargo test -p corelink-cas`: 10 / 10 smoke tests green.
- `cargo test -p corelink-ac`: 4 / 4 smoke tests green.
- `cargo test -p corelink-ac-core -p corelink-manifest -p corelink-worker
  --no-fail-fast`: full test corpora green at parity with pre-rename
  (`5d3d32c9` baseline).

### Wasm32 sanity (trigger #4)

- `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf`:
  green at every sub-step boundary (A.1, A.2a, A.2b).

### Spec / reference / migration validators

- `python3 scripts/validate_specs.py`: green (449 with schema, 9 YAML;
  458 total).
- `python3 scripts/validate_references.py`: green (zero dangling).
- `python3 scripts/check_migrations_additive.py`: green (59 files).
- `python3 scripts/validate_inv_inheritance.py`: green (16 child
  chains across 8 parent targets).

## §9. Next steps + Owner decision gate

### Sub-step A.3 + A.4 deferral — Owner approval required

Per §4 + §7, A.3 + A.4 are deferred via hard-pause-trigger-1 partial
activation. Owner may either:

- **(a) Accept partial Stream A delivery (3 of 5 sub-steps SEALED;
  port surfaces stable)** — the architectural commitment (canonical
  `corelink_cas::*` + `corelink_ac::*` import surfaces) is delivered;
  Stream C may dispatch against stable port surfaces; the
  mega-file decomposition rides into a dedicated follow-on agent
  dispatch.
- **(b) Re-dispatch a follow-on agent with explicit greenlight for
  function-internal helper extraction** — the agent owns A.3 + A.4
  + the SEAL update + the MID-POINT marker; budget includes ~12-15
  new files (`handler/{mod,cas/{mod,batch_update,batch_read/{mod,
  validate,d1_pass,decide,r2_pass,compose},find_missing},
  capabilities,bytestream,helpers}` + `reconcile/{mod,plan,execute,
  verify}.rs`); each file ≤ 500 LOC HARD CAP; aim ≤ 200 LOC.

### Stream C dispatch

Stream C's dependency on Stream A is the **port surface stability**
(per charter §6 Stream A/B mid-point → C decision gate). A.1 + A.2
deliver stable `corelink_cas::*` and `corelink_ac::*` surfaces — the
Stream C agent can consume those import paths regardless of whether
A.3 + A.4 land in Stream A or a follow-on. **Stream C dispatch is
NOT blocked by the A.3 + A.4 deferral.**

### MID-POINT marker commit

Per dispatch §"MID-POINT MILESTONE": after A.3 + A.4 SEAL the
orchestrator polls for an empty `wave-33 stream-a MID-POINT` commit
that signals Stream C to dispatch. Because A.3 + A.4 are deferred,
the MID-POINT marker is **NOT** placed by this SEAL pass — Stream A
delivery is partial and the marker would falsely signal completion
that hasn't happened. The Owner's decision gate (a) vs. (b) above
implicitly resolves the marker question:

- Under (a): the orchestrator places a "MID-POINT (partial)"
  marker out-of-band confirming Stream A delivery is partial but
  Stream C may proceed.
- Under (b): the follow-on agent commits the MID-POINT marker after
  A.3 + A.4 SEAL inside its own sub-step.

### Branch + tag

Branch: `wt/r-prep-w33-stream-a-data-path`.
Stream A SEALED commits:
- A.1: `eb783c6b`
- A.2a: `df3cc072`
- A.2b: `0b0ebc03`

This SEAL audit commit (head of branch after this doc lands) closes
the partial stream and surfaces the trigger-1 escalation.

## §10. DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

**End of Wave 33 Stage 1 Stream A — Data Path SEAL audit (partial).**
