---
id: "AUDIT-2026-05-26-W35-P2-REPLICATION-ABSORPTION"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags:
  - "audit"
  - "wave-35"
  - "phase-2"
  - "replication"
  - "absorption"
  - "rollout-controller"
  - "umbrella-consolidation"
  - "physical-move"
  - "seal"
references:
  - "specs/_audits/2026-05-26-wave-33-34-closure-followups.md"
  - "specs/_audits/2026-05-22-wave33-code-reorg-spec.md"
  - "specs/_audits/2026-05-22-w33-stage0-foundation.md"
  - "crates/corelink-replication/Cargo.toml"
  - "crates/corelink-replication/src/lib.rs"
  - "crates/corelink-replication/src/rollout_controller.rs"
---

# Wave 35 Phase 2 — `corelink-replication` umbrella absorption

> **Branch:** `w35-p2-replication`
>
> **Trigger:** `specs/_audits/2026-05-26-wave-33-34-closure-followups.md`
> §4 — "absorb 1 sub-crate into `corelink-replication` umbrella" as
> part of the broader Wave-33 Stage-1 Option-A → physical-move
> transition that retires shim-only re-export crates.
>
> **Scope:** physical absorption of `corelink-rollout-controller`
> (single sub-crate; 2329 LOC; 56 tests) into
> `corelink-replication::rollout_controller`. Pure refactor: no public
> behaviour changes; no invariant changes; no new public API; no
> consumer-side changes (no external consumers of the absorbed crate
> existed prior to the move).

## §1. Mandate (Phase 2 of W35)

The Wave 33 Stage 1 Stream C aggregator landed
`corelink-replication` as an **Option-A re-export façade** over five
absorbed crates (`corelink-region`, `corelink-replica-worker`,
`corelink-replication-coordinator`, `corelink-failover-router`,
`corelink-rollout-controller`). Per the Stage 0 SEAL §4
interpretation, physical absorption was deferred until each
absorbed crate's consumers could be migrated atomically.

Wave 35 Phase 2 begins that transition for the
**replication umbrella**, starting with the single crate whose
consumer surface has fully migrated to `corelink_replication::*`
paths (`corelink-rollout-controller`).

The remaining four absorbed crates (`corelink-region`,
`corelink-replica-worker`, `corelink-replication-coordinator`,
`corelink-failover-router`) retain Option-A re-export shims pending
their own consumer-migration windows.

## §2. Inventory (1 crate)

| # | Absorbed | LOC (src) | Tests | Examples | Shim removed |
|---|----------|-----------|-------|----------|---------------|
| 1 | `crates/corelink-rollout-controller` | 2329 | 38 unit + 8 adversarial + 10 prop = **56** | 3 | `crates/corelink-replication/src/rollout.rs` |

`corelink-rollout-controller` directory is **gone** post-merge
(`git rm -r crates/corelink-rollout-controller`).

## §3. Physical layout (post-absorption)

```
crates/corelink-replication/
├── Cargo.toml                              # +thiserror/serde/serde_json/uuid; +5 [[test]]/[[example]] tables
├── src/
│   ├── lib.rs                              # pub mod rollout_controller; (renamed from `rollout`)
│   ├── rollout_controller.rs               # crate root for the absorbed module (no `#![...]`)
│   ├── rollout_controller/
│   │   ├── audit.rs
│   │   ├── auto_rollback.rs
│   │   ├── budget.rs
│   │   ├── controller.rs
│   │   ├── error.rs
│   │   ├── metrics.rs
│   │   ├── state_machine.rs
│   │   └── types.rs
│   ├── coordinator.rs                      # (untouched Option-A shim)
│   ├── failover.rs                         # (untouched Option-A shim)
│   ├── region.rs                           # (untouched Option-A shim)
│   └── replica.rs                          # (untouched Option-A shim)
├── tests/
│   ├── rollout_controller_adversarial.rs   # was crates/corelink-rollout-controller/tests/adversarial.rs
│   └── rollout_controller_prop_rollout.rs  # was crates/corelink-rollout-controller/tests/prop_rollout.rs
└── examples/
    ├── rollout_controller_budget_exceeded.rs
    ├── rollout_controller_manual_abort.rs
    └── rollout_controller_start_rollout.rs
```

### Rationale for `rollout_controller.rs` (not `rollout_controller/mod.rs`)

The umbrella enforces `clippy::mod_module_files = "deny"` (denies
`foo/mod.rs` style; prefers `foo.rs` + `foo/`). The physical move
follows that convention.

### Rationale for `rollout_controller::*` (not `rollout::*`)

Wave 33 originally exposed the absorbed surface at
`corelink_replication::rollout::*`. Wave 35 Phase 2 renames this to
`corelink_replication::rollout_controller::*` to preserve the
absorbed crate's original name (`corelink-rollout-controller`) and
remove a unique-prefix collision risk with the workspace
`corelink-rotation-*` and `corelink-rollback-*` neighbourhoods. No
external consumers were impacted (search across `crates/`, `apps/`,
`tools/` showed zero external `corelink_replication::rollout::`
references prior to the rename).

### Internal `crate::*` rewrites

The absorbed module's internal cross-references previously took the
form `crate::audit::*`, `crate::budget::*`, etc. (its old crate
root). They are now rewritten to `super::audit::*`, `super::budget::*`,
etc. — sibling-module form within the absorbed module subtree. Inside
unit-test modules (`#[cfg(test)] mod tests { ... }`) the references
become `super::super::audit::*` / `super::super::budget::*` because
the test module nests one level below the file scope. Verbatim
behaviour preserved.

## §4. Charter compliance

- `#![forbid(unsafe_code)]` — inherited from the umbrella crate root
  (`crates/corelink-replication/src/lib.rs`). The absorbed crate's
  former `#![forbid(unsafe_code)]` was a crate-level inner attribute
  and is now redundant; removed from the module file with a docs
  comment recording the lineage.
- `#[non_exhaustive]` on every public enum/struct — preserved
  verbatim (no edits to any `#[non_exhaustive]` site).
- No `unwrap` / `expect` / `panic` outside `#[cfg(test)]` modules —
  preserved verbatim.
- INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER — preserved verbatim
  (audit-fail-CLOSED envelope sites in
  `rollout_controller/audit.rs` and
  `rollout_controller/controller.rs` are untouched at the logic
  level; only `crate::` → `super::` rewrites apply).
- INV-ROLLOUT-NO-STAGE-SKIP, INV-ROLLOUT-COSIGN-GATE — preserved
  verbatim.
- `missing_docs = "deny"` and
  `missing_debug_implementations = "deny"` — inherited from the
  umbrella crate's `[lints.rust]` stanza.

## §5. Verification (cold-tool, this branch HEAD)

| Gate | Command | Result |
|------|---------|--------|
| Build | `cargo build -p corelink-replication` | **GREEN** |
| Clippy | `cargo clippy -p corelink-replication --tests -- -D warnings` | **GREEN** |
| Tests | `cargo test -p corelink-replication` | **61 passed; 0 failed** |
| Conflict markers | `grep -rEn '<<<<<<<\|>>>>>>>' --include='*.toml' --include='*.rs' --include='*.md' crates/corelink-replication Cargo.toml` | **clean** |
| Remnant directory | `test -d crates/corelink-rollout-controller` | **gone** |
| External consumers of removed crate | `grep -rln 'corelink-rollout-controller\|corelink_rollout_controller' --include='*.toml' --include='*.rs'` | only doc-comment mentions in the umbrella crate; **no compile-edge references** |
| Workspace members | `cargo metadata --no-deps` | **138** (was 139; −1 as expected) |

### Test count delta

| Lane | Before (replication) | After (replication) |
|------|----------------------|---------------------|
| Unit (lib) | 5 | 43 (5 + 38 absorbed) |
| Integration: adversarial | — | 8 |
| Integration: prop_rollout | — | 10 |
| **Total** | **5** | **61** |

Net gain: **+56** tests, matching the absorbed crate's prior count
(38 + 8 + 10 = 56). The §2 inventory claim of "58 tests" was a
rounded prior estimate; the exact cargo-counted figure is 56.

## §6. Cargo manifest changes

### `crates/corelink-replication/Cargo.toml`

- Removed: `corelink-rollout-controller = { workspace = true }`.
- Added (from absorbed crate's `[dependencies]`): `thiserror`,
  `serde`, `serde_json`, `uuid` (with `v7`, `serde`, `js` features
  per the wasm32 RNG pattern).
- Added: `[dev-dependencies] proptest`.
- Added: 2 `[[test]]` tables + 3 `[[example]]` tables for the
  relocated harnesses.
- Untouched: re-export targets for the remaining four absorbed
  crates (`corelink-region`, `corelink-replica-worker`,
  `corelink-replication-coordinator`, `corelink-failover-router`);
  `corelink-worker` (Stage 2.A-v2 additive aggregator).

### `Cargo.toml` (workspace root)

- Removed member: `"crates/corelink-rollout-controller"`.
- Removed workspace dep:
  `corelink-rollout-controller = { path = "crates/corelink-rollout-controller" }`.
- Replaced both lines with explanatory comments referencing this
  SEAL audit.

## §7. Parallel-safety / merge-surface

- **CONFLICT surface** with sibling W35-P2 branches
  (`W35-P2-CAS`, `W35-P2-TELEMETRY`, `W35-P2-ADAPTER-HOST`) is
  limited to the workspace-root `Cargo.toml` `[workspace] members =
  [...]` array and the `[workspace.dependencies]` block. Each W35-P2
  agent removes exactly one member-line and one dep-line in
  disjoint regions. Orchestrator UNION-merges per the
  spec §5.
- No touches outside `crates/corelink-replication` +
  `crates/corelink-rollout-controller` + root `Cargo.toml`. W36
  zones (`tests/proptests/wasm/*`, consumer-migration files in
  `apps/server/*` or `crates/corelink-container/*`) untouched.

## §8. Hard-pause triggers — none fired

- No `--no-verify` used.
- No `#[allow(...)]` added (the only `#[allow(unused_imports)]` in
  the path-resolution smoke tests was pre-existing in
  `crates/corelink-replication/src/lib.rs` from the Wave 33 landing).
- No `cargo build` at workspace level.
- No charter-invariant edits.
- No public-API signature changes (every public symbol of the
  absorbed crate is still callable; the only canonical path
  changes from `corelink_rollout_controller::X` →
  `corelink_replication::rollout_controller::X` and from
  `corelink_replication::rollout::X` →
  `corelink_replication::rollout_controller::X`. Both old paths
  were workspace-internal — no external consumers).

## §9. Output report

```
SEAL — W35-P2-REPLICATION
Branch:        w35-p2-replication
Commit SHA:    <set by §10 commit>
Crates removed (1):
  - corelink-rollout-controller (2329 LOC, 56 tests)
Workspace members: 139 → 138
Tests: 5 → 61 (net +56)
Build:   GREEN  (cargo build -p corelink-replication)
Clippy:  GREEN  (cargo clippy -p corelink-replication --tests -- -D warnings)
Tests:   GREEN  (61 passed; 0 failed)
SEAL audit: specs/_audits/2026-05-26-w35-p2-replication-absorption.md
```
