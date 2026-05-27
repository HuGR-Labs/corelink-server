# Wave 33 Stage 2.A — corelink-worker piece moves SEAL Audit (PARTIAL/HALT) (2026-05-26)

> **Doc kind:** stage closure audit (evidence; `_audits/` excluded from
> canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-26 by Claude Opus 4.7 in branch
> `wt/r-prep-w33-stage2-a-worker-moves` (worktree
> `.claude/worktrees/agent-aa7231a93e203be89`).
>
> **Baseline:** `ca6a3a55` (post Stage 2.B + Stage 2.D-v2 merge to main).
>
> **Disposition:** **PARTIAL / HALT** — zero LOC delta; physical
> decomposition of `corelink-worker` deferred per pre-mutation Hard
> Pause Triggers #6 + #7 activation. Stage 2.C precedent (commit
> `6c54a6e8`, doc `2026-05-22-w33-stage2-c-adapter-splits.md`)
> establishes the partial-SEAL halt pattern this audit follows.

## §1. Baseline verification + dispatch acknowledgment

Per techlead Section 0.5 mandatory step-0 baseline verify:

- `EXPECTED_BASELINE="ca6a3a55"`.
- `ACTUAL_BASELINE` returned `99269ed02f8e356dcff86a6c5abe560d752af172`
  (stale agent-tool worktree default — same failure mode PRE-A and 2.D-v2
  recovered from).
- Recovery executed exactly as dispatched: `git fetch origin` then
  `git checkout -B wt/r-prep-w33-stage2-a-worker-moves ca6a3a55`.
- Post-recovery `git rev-parse HEAD` = `ca6a3a5599b09dfc397021f4f821e1a0281a353f` (matches).
- `cargo check --workspace` GREEN in 2m 02s.

Dispatch packet baseline matches the actual workspace state. The
HALT decision below is purely architectural — not a baseline-mismatch
artefact, not a tool failure.

## §2. Dispatch plan vs. observed reality

### §2.1 Dispatch plan summary (verbatim from orchestrator packet §"Sub-step plan")

7 physical-move commits + SEAL:

1. `corelink-worker/src/storage/` → `corelink-cas/src/r2_storage/`
2. `corelink-worker/src/cache/` → `corelink-cas/src/cache/`
3. `corelink-worker/src/middleware/` → `corelink-auth/src/middleware/`
4. `corelink-worker/src/reapi/` → `corelink-reapi/src/worker_adapter/`
5. `corelink-worker/src/region.rs` → `corelink-replication/src/region_resolver.rs`
6. `corelink-worker/src/auth/` → `corelink-auth/src/worker_session/`
7. Slim `corelink-worker` to ≤100 LOC wasm32 shell.

Constraint: "Behavior-preserving" + "package `corelink-worker` stays;
paths reorganize" + "consumers may need updated `use` paths (atomic
with each move)" + 39 external consumer files (briefing said 38;
actual count 39 external + 24 internal = 63 total — see §4).

### §2.2 What was actually observed in the target crates

Reading `crates/corelink-cas/src/lib.rs` (133 LOC), the corelink-cas
crate is a pure **Option-A aggregator** per the Stage 0 SEAL audit
§1 (`2026-05-22-w33-stage0-foundation.md` lines 25-86). Its `src/`
directory contains **zero implementation code** — only 1-line
`pub use corelink_X::*;` re-export files:

```
crates/corelink-cas/src/lib.rs          (133 LOC; pub mod declarations + smoke tests)
crates/corelink-cas/src/chunker.rs      (1 LOC: pub use corelink_chunker::*;)
crates/corelink-cas/src/dedup.rs        (1 LOC: pub use corelink_dedup::*;)
crates/corelink-cas/src/edge.rs         (1 LOC: pub use corelink_edge::*;)
crates/corelink-cas/src/eviction.rs     (1 LOC: pub use corelink_eviction::*;)
crates/corelink-cas/src/handler.rs      (9 LOC: pub use corelink_handler_cas::*;)
crates/corelink-cas/src/lru_tracker.rs  (1 LOC)
crates/corelink-cas/src/manifest.rs     (1 LOC)
crates/corelink-cas/src/meta.rs         (1 LOC)
crates/corelink-cas/src/multipart_schema.rs (1 LOC)
crates/corelink-cas/src/r2_multipart.rs (1 LOC)
```

Per `corelink-cas/src/lib.rs` doc comment (lines 32-42):

> Stage 2 will physically absorb the source files once every
> consumer has migrated to the canonical `corelink_cas::*` paths.
> Until then both the original crate paths (e.g.
> `corelink_chunker::*`) and the canonical paths (e.g.
> `corelink_cas::chunker::*`) resolve to the same types.
>
> **Behaviour preservation:** Every public symbol of the 10 absorbed
> crates remains reachable at its original path AND at the new
> canonical path. Zero behavioural change — purely additive
> re-export façade.

The same Option-A aggregator pattern is documented in
`corelink-auth/src/lib.rs` (171 LOC; 6 absorbed crates), and
`corelink-replication/src/lib.rs` (129 LOC; 5 absorbed crates).

**The target crates this dispatch instructs `git mv` into are pure
aggregator façades.** Stage 0 sub-step 2 deliberately rejected the
strict-reading "physical-move-into-aggregator" interpretation in
favour of the aggregator reading (Stage 0 SEAL §1 lines 42-58).

### §2.3 Architectural inconsistency the dispatch would create

Performing the 7 sub-step moves as specified would land 19K LOC of
production code (R2 storage, cache, auth middleware, REAPI handler,
region resolver, auth session) **inside crates whose entire `src/`
tree is currently 1-line re-export shims**. Post-move state:

- `corelink-cas/src/lib.rs` would mix `pub mod chunker;` (aggregator
  re-export from `corelink-chunker`) with `pub mod r2_storage;`
  (1268 LOC of actual implementation moved from corelink-worker).
- `corelink-auth/src/lib.rs` would mix `pub mod clerk;` (re-export)
  with `pub mod middleware;` (2996 LOC) and `pub mod worker_session;`
  (2200 LOC).
- The 10/6/5 absorbed crates would remain as canonical sources of
  truth for their own modules (Stage 0/1 pattern), creating a
  three-tier hierarchy (aggregator + implementation + absorbed
  source-of-truth) that no Stage 0/1 SEAL audit sanctions.

This contradicts the explicit Stage 0/1 invariant restated in every
absorbed-crate's doc comment, the Wave-33 spec §6 sub-step 2 ("Pick
Option-A"), and the Stage 2.C SEAL audit §9 recommendation (a)
("Stage 2 closes WITHOUT … physical split; the Stage 1 aggregator
pattern remains in place as the canonical Wave-33 surface" — line 295).

### §2.4 Wasm32 entry-shell goal is undefined

Dispatch sub-step 7 specifies: "corelink-worker slim to wasm32 entry
shell (lib.rs ≤ 100 LOC; only Region + TenantCtx re-exports preserved
if needed)". Observations:

- `corelink-worker` is a library crate (no `[[bin]]`, no `cdylib`,
  no `lib.crate-type = ["cdylib"]`). It does not produce a wasm32
  binary; the cf binding crate (`corelink-cf-bindings`,
  `corelink-clerk-cf`) and the TS layer in `apps/worker/` (which
  per `ls apps/` does **not exist** — only `admin-ui`, `docs`,
  `migrate-single-to-multi-region` are present) are the runtime
  surface for the CF Worker.
- The spec §3 inventory line 59 listed `corelink-worker/` as a
  "CF Worker (wasm32) entry — TS in apps/worker/; this Rust crate
  hosts wasm-bridged logic". The TS app and the wasm-bridge model
  are not present in the post-Stage-2.B/2.D tree. Slimming
  corelink-worker to a "wasm32 entry shell" without that
  counterpart in place would leave a 100 LOC crate with no caller.
- Briefing fallback "only Region + TenantCtx re-exports preserved"
  conflicts with the same briefing's move-table row 5: `region.rs`
  moves to `corelink-replication`. Re-exporting it from corelink-worker
  after the move is exactly the Option-A aggregator pattern the
  dispatch was designed to retire — net change: zero.

## §3. Hard pause trigger activation

Per dispatch packet §"Hard pause triggers":

| # | Trigger | Status |
|---|---------|--------|
| 1 | Single file >500 LOC after a move | N/A (no mutation) |
| 2 | Previously-green test goes red | N/A (no mutation) |
| 3 | `INV-CAS-*` / `INV-AUTH-*` invariant test fails | N/A (no mutation) |
| 4 | Wasm32 build red | N/A (no mutation) |
| 5 | Workspace build red at sub-step boundary | N/A (no mutation) |
| 6 | Consumer surface bigger than 38 (hidden coupling — investigate) | **ACTIVATED** — see §4 |
| 7 | corelink-worker can't be slimmed to wasm32 shell because target/replication need a non-trivial peer (architectural finding — escalate) | **ACTIVATED** — see §2.3 + §2.4 |

Triggers #6 + #7 both fired **pre-mutation**. Per dispatch packet
literal text: "HALT + escalate. Stream A1 + 2.C precedents:
partial-SEAL with documented deferral is acceptable."

## §4. Consumer surface — verified

`grep -rln "corelink_worker::" --include="*.rs" crates/` returns **63
files**:

- **39 external** consumer files (non-`corelink-worker/`):
  - `crates/corelink-cf-bindings/`: 7 files (src + tests; production
    CF adapter — uses `corelink_worker::storage::r2::R2Backend`,
    `corelink_worker::cache::kv::KvBackend`, etc.)
  - `crates/corelink-reapi/`: 32 files (12 tests + 20 src;
    orchestrator/handler/per_blob etc.)
- **24 internal** files (corelink-worker itself: src, tests, benches,
  fuzz/fuzz_targets — these will become `crate::*` or external
  `corelink_cas::r2_storage::*` style imports per a sub-step plan).

Briefing stated 38 external; actual 39 (delta of +1 is within
inventory drift but the bigger finding is the briefing did not
count the 24 internal references that also need rewriting). Trigger
#6 ("Consumer surface is bigger than 38 — hidden coupling —
investigate") fires.

Hidden coupling examples (sampled via grep):

```
crates/corelink-cf-bindings/src/cf_kv.rs       — production CF KV
                                                 adapter; uses
                                                 corelink_worker::cache::kv::{KvBackend, KvError}.
                                                 Moving cache/ to corelink-cas
                                                 would put a CAS crate dep into
                                                 the CF binding crate; that
                                                 binding currently has clean
                                                 wasm32 deps only.
crates/corelink-cf-bindings/src/cf_r2.rs       — production CF R2 adapter;
                                                 same pattern.
crates/corelink-reapi/src/orchestrator.rs      — uses
                                                 corelink_worker::middleware::MissMarker,
                                                 corelink_worker::storage::error::R2Error,
                                                 corelink_worker::storage::r2::{*}.
                                                 Per dispatch row 3+4, these
                                                 would resolve to
                                                 corelink_auth::middleware::*
                                                 AND corelink_cas::r2_storage::*
                                                 simultaneously — orchestrator
                                                 spans cas+auth+reapi+replication
                                                 contexts in one file.
crates/corelink-reapi/src/handler/cas.rs       — depends on
                                                 corelink_worker::reapi::cas::*,
                                                 which row 4 moves to
                                                 corelink-reapi::worker_adapter::cas::*.
                                                 This creates an in-crate
                                                 circular reference (handler
                                                 imports worker_adapter; both
                                                 inside same crate). Tractable
                                                 mechanically but indicates
                                                 the row-4 decomposition is
                                                 semantically wrong.
```

The orchestrator/handler files in `corelink-reapi/src/` cross-cut
all four target contexts simultaneously. The proposed move table
maps each cross-cut import to a different target crate. After the
moves, every cross-cut file in corelink-reapi/src/ would need 4
distinct `use` statements (one per target crate) where today there
is one (`use corelink_worker::*`). This is mechanically tractable
but architecturally wrong — corelink-reapi handler/orchestrator code
should not need to import from 4 sibling context crates just to
read a chunk from R2.

## §5. Why physical decomposition violates the spec's own invariants

Wave-33 reorg spec (`2026-05-22-wave33-code-reorg-spec.md`):

- Line 86 (§4 row 5): "**`corelink-worker`** | `corelink-worker`
  (existing 19K LOC; **WILL BE DECOMPOSED**) | … **Decomposition
  plan §6 splits this across cas/ac/auth/replication contexts.**
  New `corelink-worker` becomes thin CF Worker entry."
- Line 110: "Move R2 reader/writer to `corelink-cas`; cache+middleware
  to `corelink-auth`; reapi adapter to `corelink-reapi`; region
  resolver to `corelink-replication`; auth to `corelink-auth`"
- Line 183 (Stage 2): "Decompose existing `corelink-worker` (19K LOC)
  per §4 row; redistribute logic across cas/ac/auth/replication;
  new thin `corelink-worker` becomes CF Worker (wasm32) entry shell."
- Line 227 (Acceptable risks): "Stage 2 `corelink-worker`
  decomposition reveals hidden coupling that requires Stream A/B
  re-work (architecturally invalid plan)."

Stage 0 SEAL audit (`2026-05-22-w33-stage0-foundation.md`):

- Lines 42-58: Stage 0 sub-steps 2+3+4 selected the **aggregator
  reading** of Option-A, NOT the strict-reading. Quoting:
  > "Behaviour preservation: zero risk of breaking cdylib/cbindgen
  > pipelines …; charter rule 'Behavior-preserving refactor ONLY'
  > … physical relocation across 12 crates with FFI / fuzz / bench
  > / example surfaces was assessed as exceeding the safety margin
  > a single orchestrator-direct sub-step can hold."

Stage 1 Stream A/B/C SEAL audits all use the aggregator pattern.

Stage 2.C SEAL audit (`2026-05-22-w33-stage2-c-adapter-splits.md`
line 295): "Stage 2 closes WITHOUT Stage 2.C physical split; the
Stage 1 aggregator pattern (`pub use corelink_X_real::*` from
sibling umbrella) remains in place as the canonical Wave-33 surface."

**The spec's Stage 2.A row contemplates physical decomposition
because the spec was written before Stage 0 SEAL chose the
aggregator reading.** When Stage 0 selected the aggregator reading,
it implicitly deferred ALL physical-move work — including 2.A — to
a future stage that owns the necessary atomic consumer migration
(named "Stage 1 atomic ownership" in Stage 0 SEAL line 54). 2.A
attempting strict-reading physical moves against Stage-0/1-built
aggregator façades is the architectural mismatch documented here.

Acceptable risk #5 (spec line 227) was foreseen by the spec author
and explicitly named: "decomposition reveals hidden coupling that
requires Stream A/B re-work (architecturally invalid plan)". This
HALT is the exact realisation of that risk.

## §6. What COULD be delivered in 2.A as additive value (deferred)

A safe-and-architecturally-consistent variant of 2.A would deliver
the canonical Wave-33 import surface without mutating any
implementation file:

1. Add `pub mod r2_storage { pub use corelink_worker::storage::r2::*; }`
   etc. to `corelink-cas/src/lib.rs` (or as a new `r2_storage.rs`
   re-export file). Mirrors the Stage 1 Stream A aggregator pattern
   exactly.
2. Add `pub mod middleware { pub use corelink_worker::middleware::*; }`
   and `pub mod worker_session { pub use corelink_worker::auth::*; }`
   to `corelink-auth/src/lib.rs`.
3. Add `pub mod region_resolver { pub use corelink_worker::Region; }`
   to `corelink-replication/src/lib.rs`.
4. Add `pub mod worker_adapter { pub use corelink_worker::reapi::*; }`
   to `corelink-reapi/src/lib.rs`.
5. Add `corelink-worker = { workspace = true }` to each target crate's
   `[dependencies]` if missing, conditionally on whichever `feature`
   set propagates (the `tower-middleware` feature is the gnarliest —
   adding it to corelink-auth would re-introduce the wasm32 build
   regression risk Stage 1 spent significant effort avoiding).
6. The 39 external consumers can THEN migrate to canonical paths in
   a follow-up dispatch ("Stage 2.E consumer migration", referenced
   in 2.C SEAL §9.next-steps), atomically per call site, without any
   in-flight package break.

The additive-aggregator variant matches every Stage 0/1 precedent
and Stage 2.C's "ACCEPT-PARTIAL with aggregator surface preserved"
disposition.

**This audit does NOT execute the additive variant.** Reason: the
orchestrator dispatch packet specified the strict-reading physical
moves AND specified that HALT + escalate is the prescribed response
when triggers #6 or #7 fire. Silently substituting a different
strategy would not be a HALT — it would be unsanctioned scope drift.
The additive-aggregator variant is enumerated here so the orchestrator
can re-dispatch it explicitly if desired.

## §7. Per-sub-step status

Per dispatch packet §"Sub-step plan" 8 expected commits:

| Sub-step | Description | Status | SHA |
|----------|-------------|--------|-----|
| 1 | storage → corelink-cas | NOT EXECUTED | — |
| 2 | cache → corelink-cas | NOT EXECUTED | — |
| 3 | middleware → corelink-auth | NOT EXECUTED | — |
| 4 | reapi → corelink-reapi | NOT EXECUTED | — |
| 5 | region.rs → corelink-replication | NOT EXECUTED | — |
| 6 | auth → corelink-auth | NOT EXECUTED | — |
| 7 | slim corelink-worker to ≤100 LOC | NOT EXECUTED | — |
| 8 | this SEAL audit | EXECUTED | (this commit) |

Zero source / Cargo.toml mutation. corelink-worker LOC unchanged at
the post-PRE-A state (every file ≤456 LOC per PRE-A SEAL audit §3).

## §8. Gates — none run (no mutation)

`cargo build --workspace` last green at baseline `ca6a3a55` (verified
§1). `cargo clippy`, `validate_specs.py`, `validate_references.py`,
`check_migrations_additive.py`, `validate_inv_inheritance.py`,
`cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf`:
none re-run because the audit produces zero source / Cargo / workspace
change.

If the orchestrator re-dispatches the **additive-aggregator** variant
(§6 above), the gate set in the original dispatch packet applies
unchanged.

## §9. Recommendation + next steps

### Recommended path forward

**(a) ACCEPT-PARTIAL** — Stage 2.A ships as a HALT-only audit (this
document) with zero LOC delta. The Stage 1 aggregator pattern
remains the canonical Wave-33 surface for the corelink-worker
modules in the interim. Consistent with Stage 2.C disposition.

**(b) RE-DISPATCH 2.A as the additive-aggregator variant (§6)** —
Orchestrator authorises a follow-on sub-stage that wires the four
target crates as aggregators of `corelink_worker::*` (4-5 lines per
crate), unblocking Stage 2.E consumer migration without any in-flight
package break. Estimated 1 commit per target crate + 1 SEAL = 5 commits.

**(c) RE-DISPATCH 2.A as a 4-crate physical split with full atomic
consumer migration** — Owner explicitly greenlights a Stream-sized
follow-on (sized like Stream A or B, not 2.A) that executes the 7
physical moves atomically against all 39 + 24 = 63 consumer files,
with per-target-crate Cargo.toml dep additions, with corelink-worker
slimmed to a defined wasm32 entry-shell shape, and with the wasm32
caller surface re-established (apps/worker/ or equivalent). Estimated
3-5 days wall-clock; touches multiple call sites in cf-bindings (the
adapter crate that must keep its clean wasm32 dep set); requires
fresh techlead review before merge.

**(d) DEFER Stage 2.A to a Wave 34 hex-boundary lockdown task** —
couple it with Stage 3's `cargo deny check` introduction so the
context-boundary enforcement happens at the same time as the
physical reorganisation. Lower risk because cargo-deny surfaces
remaining cross-context imports as compile-time errors during the
move, not after.

### Strong recommendation: (b) RE-DISPATCH additive-aggregator

Rationale:
- Delivers the canonical Wave-33 import surface the spec promised
  (`corelink_cas::r2_storage::*`, etc.) — same approach Stage 0/1
  used for every other context.
- Unblocks Stage 2.E consumer migration (the 39 external import
  rewrites) without holding an in-flight half-state.
- Zero risk to corelink-worker's wasm32-clean default-feature build
  (the deps cross-pollination problem only fires under the
  `tower-middleware` feature, which is opt-in).
- Stage 3's cargo-deny lockdown remains the natural enforcement
  point for the physical inversion, mirroring 2.C's deferral.
- Charging through (c) here would break the green workspace, regress
  test counts (briefing's "test count delta target 0" is impossible
  under strict-reading physical moves because the 12 feature-gated
  `[[test]]` entries in corelink-worker/Cargo.toml depend on
  module-internal paths that vanish), AND coordinate-conflict with
  any future Wave-34 boundary-enforcement stream.

### Next steps (per dispatch packet §"Report")

- **Orchestrator runs `/techlead` before merge** — the briefing
  specifies this. Recommend `/techlead` verdict on this HALT
  parallels 2.C's verdict (ACCEPT-PARTIAL with deferral plan
  documented). techlead Section 0.5 step-0 baseline verify already
  passed (§1).
- **Re-dispatch (b)** for additive-aggregator variant if desired —
  named "Stage 2.A-v2 additive aggregator" or similar.
- **Stage 2.E consumer migration** depends on aggregator surface; if
  (b) executes it can dispatch immediately after.
- **Stage 3 hex-boundary lockdown** — surface the deferred 2.A
  physical inversion as a Wave-33 Stage 3 prerequisite OR a Wave-34
  entry task, alongside the already-deferred 2.C inversion.

## §10. Files touched

```
specs/_audits/sealed/2026-05-22-w33-stage2-a-worker-moves.md  +<NN> LOC (this doc; created)
```

Zero source files mutated. Zero Cargo.toml mutated. Zero
workspace.members entries added or removed. Zero `git mv` executed.
corelink-worker `src/` LOC distribution unchanged from baseline
`ca6a3a55`.

## §11. DCO sign-off

Per charter:

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

## §12. Cross-references

- **Parent:** `specs/_audits/sealed/2026-05-22-wave33-code-reorg-spec.md`
  §6 Stage 2; §4 row 5 (corelink-worker decomposition row); §3 line 59
  (CF Worker entry inventory); acceptable risk #5 (line 227).
- **Direct precedent (same disposition):**
  `specs/_audits/sealed/2026-05-22-w33-stage2-c-adapter-splits.md` §9
  recommendation (a) ACCEPT-PARTIAL.
- **Aggregator-pattern foundation:**
  `specs/_audits/sealed/2026-05-22-w33-stage0-foundation.md` §1 lines 25-86
  (Option-A aggregator reading rationale).
- **Stage 2 PRE-A foundation:**
  `specs/_audits/sealed/2026-05-22-w33-stage2-pre-a-worker-megafiles.md`
  §8 (Stage 2.A continuation hand-off; per-file moves enabled but
  target-crate model not validated).
- **Sibling Stage 2 work:**
  `specs/_audits/sealed/2026-05-26-w33-stage2-b-container.md` (corelink-container
  creation — followed Option-A by absorbing apps/server in-place
  rather than into an aggregator),
  `specs/_audits/sealed/2026-05-22-w33-stage2-d-out-of-tree.md` (6 SDK/CLI
  crate path moves with `git mv` — package-level moves, no
  module-internal decomposition).
- **Charter:** `specs/03_architecture/invariant_registry.md`.
- **Memory:** `[[corelink-autonomous-execution-charter]]`,
  `[[corelink-autonomous-overnight-run]]`,
  `[[feedback-synchronous-agents]]`.

---

**End of Wave 33 Stage 2.A HALT audit.**
