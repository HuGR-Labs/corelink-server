# Wave 33 Stage 2.D — Out-of-Tree Crate Migrations SEAL Audit (REDISPATCH v2)

> **Doc kind:** stage closure audit (evidence; `_audits/` excluded from
> canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-26 by Claude Opus 4.7 in branch
> `wt/r-prep-w33-stage2-d-out-of-tree-v2` (worktree
> `.claude/worktrees/agent-a4ec102e82e966852`).
>
> **Mandate:** wave-33 Stage 2.D owns the mechanical relocation of 6
> tooling / SDK crates out of `crates/` into the dedicated `tools/`
> top-level surface, consolidating the workspace's "developer-facing
> tools" boundary alongside the pre-existing `tools/sbom-publish` and
> matching the wave-33 surface-classification charter.
>
> **Redispatch rationale:** an earlier 2.D agent worked against stale
> baseline `99269ed0` (pre Stage-0 / pre PRE-B), seven commits had to
> be discarded, and the branch was deleted. This v2 dispatch is the
> redo against the correct baseline `84a08f58` (post Stage-0 + post
> PRE-A + post PRE-B). **Step-0 baseline verification PASSED before
> any work (see §7).**

## §1. Scope

6 mechanical `git mv` moves (sub-steps 2.D.1 → 2.D.6) plus this SEAL
audit (commit 7). Each move preserves file history via `git mv`,
updates `workspace.members` in `Cargo.toml`, fixes any companion CI /
dependabot / shell-script path filters in the same commit, and runs
`cargo check --workspace` green before committing.

| Sub-step | SHA | Move | Status |
|---|---|---|---|
| 2.D.1 | `b8ed4242` | `crates/corelink-go` → `tools/sdks/go` | **SEALED** |
| 2.D.2 | `3c8e5d38` | `crates/corelink-py` → `tools/sdks/python` | **SEALED** |
| 2.D.3 | `360042b8` | `crates/corelink-cli` → `tools/cli` | **SEALED** |
| 2.D.4 | `4780c2c9` | `crates/corelink-openapi` → `tools/openapi` | **SEALED** |
| 2.D.5 | `dddba952` | `crates/corelink-dt-cli` → `tools/dt-cli` | **SEALED** |
| 2.D.6 | `e7fc1f81` | `crates/corelink-dt-reconcile` → `tools/dt-reconcile` | **SEALED** |

6 of 6 sub-steps SEALED. Hard pause triggers: **NONE** fired (see §7).

**DEFERRED to 2.E:** `corelink-d1-migrations` (rusqlite-backed CI
migration replay harness). This crate has 6 consumers across
`apps/server/`, `crates/corelink-replication/`, and several billing /
privacy crates per the dispatch-pre-flight survey; moving it requires
consumer-import edits beyond 2.D's mechanical-move scope. Deferred per
the pre-dispatch packet to the dedicated 2.E consumer-migration
stage, which can sequence the path change alongside any `use`-line
fixups.

## §2. Per-move evidence

| Sub-step | Source | Target | Pkg name | LOC | Cargo refs | src `use` refs | Workspace.dep entry |
|---|---|---|---|---|---|---|---|
| 2.D.1 | `crates/corelink-go` | `tools/sdks/go` | `corelink-go` (unchanged) | 421 | 0 | 0 | n/a (no consumer) |
| 2.D.2 | `crates/corelink-py` | `tools/sdks/python` | `corelink-py` (unchanged) | 326 | 0 | 0 | n/a (no consumer) |
| 2.D.3 | `crates/corelink-cli` | `tools/cli` | `corelink-cli` (unchanged) | 5445 | 1 (`[workspace.dependencies]`) | 0 | path bumped |
| 2.D.4 | `crates/corelink-openapi` | `tools/openapi` | `corelink-openapi` (unchanged) | 198 | 1 (`[workspace.dependencies]`) | 0 | path bumped |
| 2.D.5 | `crates/corelink-dt-cli` | `tools/dt-cli` | `corelink-dt-cli` (unchanged) | 135 | 1 (`[workspace.dependencies]`) | 0 | path bumped |
| 2.D.6 | `crates/corelink-dt-reconcile` | `tools/dt-reconcile` | `corelink-dt-reconcile` (unchanged) | 152 | 1 (`[workspace.dependencies]`) | 0 | path bumped |

**Package names are unchanged for every move.** Consumers that depend
via package name (e.g., `corelink-cli = { workspace = true }`) require
no edits. Only the `[workspace.dependencies] corelink-* = { path =
"..." }` lines in root `Cargo.toml` were path-bumped.

**Intra-crate `[dependencies]` relative-path fix-ups** (each was a
brittle `../corelink-foo` that broke once the source crate moved
out-of-tree). All collapsed to `{ workspace = true }`, which is the
canonical workspace pattern (the workspace dep already existed for
each):

| Sub-step | Crate Cargo.toml dep edited | Old | New |
|---|---|---|---|
| 2.D.1 | `tools/sdks/go/Cargo.toml` | `corelink-client-verify = { path = "../corelink-client-verify", features = ["ffi"] }` | `corelink-client-verify = { workspace = true, features = ["ffi"] }` |
| 2.D.2 | `tools/sdks/python/Cargo.toml` | `corelink-client-verify = { path = "../corelink-client-verify" }` | `corelink-client-verify = { workspace = true }` |
| 2.D.3 | `tools/cli/Cargo.toml` | `corelink-runbook-tracker = { path = "../corelink-runbook-tracker" }` | `corelink-runbook-tracker = { workspace = true }` |
| 2.D.5 | `tools/dt-cli/Cargo.toml` | `corelink-dt-webhook = { path = "../corelink-dt-webhook" }` | `corelink-dt-webhook = { workspace = true }` |
| 2.D.6 | `tools/dt-reconcile/Cargo.toml` | `corelink-dt-webhook = { path = "../corelink-dt-webhook" }` | `corelink-dt-webhook = { workspace = true }` |

2.D.4 (`corelink-openapi`) had no intra-crate workspace path deps.

## §3. workspace.members diff

Before 2.D (baseline `84a08f58`): 6 entries pointed at `crates/corelink-{go,py,cli,openapi,dt-cli,dt-reconcile}`.

After 2.D (HEAD): the same 6 entries now point at `tools/{sdks/go,sdks/python,cli,openapi,dt-cli,dt-reconcile}`. The total workspace.members count is **unchanged** (6 in / 6 out).

## §4. Companion file path updates

| Sub-step | File | Edit |
|---|---|---|
| 2.D.1 | `.github/workflows/ffi-matrix-ci.yml` | `crates/corelink-go/**` → `tools/sdks/go/**` (2 path filter lines) |
| 2.D.2 | `.github/workflows/ffi-matrix-ci.yml` | `crates/corelink-py/**` → `tools/sdks/python/**` (2 path filter lines); `working-directory: crates/corelink-py` → `working-directory: tools/sdks/python` (2 occurrences) |
| 2.D.2 | `.github/dependabot.yml:242` | `directory: "/crates/corelink-py"` → `directory: "/tools/sdks/python"` (pip ecosystem) |
| 2.D.3 | `.github/workflows/quickstart-validate.yml` | `crates/corelink-cli/src/main.rs` → `tools/cli/src/main.rs` (2 path filter lines) |
| 2.D.3 | `apps/docs/scripts/validate-quickstart.sh` | `CLI_SRC` var + 2 comments referencing `crates/corelink-cli/src/main.rs` updated |
| 2.D.3 | `tests/cli_telemetry_optin.rs:30` | doc-comment `corelink-cli/src/telemetry.rs` → `tools/cli/src/telemetry.rs` |
| 2.D.3 | `apps/server/src/routes/audit_export/types.rs:46` | doc-comment `crates/corelink-cli/src/commands/verify_ndjson.rs` → `tools/cli/src/commands/verify_ndjson.rs` (post-PRE-B path confirmed via grep) |
| 2.D.4 | `.github/workflows/openapi-validate.yml` | `crates/corelink-openapi/**` → `tools/openapi/**` (2 path filter lines) |

2.D.5 and 2.D.6 had no companion CI / dependabot / script refs to update (confirmed via `grep -rn` in `.github/`, `apps/docs/scripts/`, `scripts/`).

**Non-edits / out-of-scope:** the top-level Go module wrapper at
`./corelink-go/` (a sibling directory containing `corelink.go` /
`corelink_test.go` / `go.mod`) is **not** the moved Rust crate; the
`.github/workflows/ffi-matrix-ci.yml` references to `corelink-go/**`
(without the `crates/` prefix) and `working-directory: corelink-go`
remain intact as they point at the Go module wrapper, which is not in
2.D scope. The `cargo {build,test,clippy} -p corelink-go` invocations
in that workflow continue to resolve by package name (unchanged).

## §5. Gate results per sub-step

Every sub-step passed `cargo check --workspace` before committing. Final HEAD gate sweep:

| Gate | Result |
|---|---|
| `cargo check --workspace` | **GREEN** (`Finished dev profile`) |
| `cargo test -p corelink-go --no-run` | **GREEN** (`corelink_go-f164d5e7`) |
| `cargo test -p corelink-py --no-run` | **GREEN** (`corelink_py-0edab1a0`) |
| `cargo test -p corelink-cli --no-run` | **GREEN** (lib + 3 integration suites) |
| `cargo test -p corelink-openapi --no-run` | **GREEN** (`corelink_openapi-7e2c2781`) |
| `cargo test -p corelink-dt-cli --no-run` | **GREEN** (`corelink_dt_cli-679c2876`) |
| `cargo test -p corelink-dt-reconcile --no-run` | **GREEN** (`corelink_dt_reconcile-7b9d70d3`) |
| `cargo test -p corelink-server --lib --no-run` | **GREEN** (consumer crate; no broken doc-comment / import) |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | **GREEN** (2m 03s) |
| `python3 scripts/validate_specs.py` | **458** validated (449 schema + 9 YAML-only) |
| `python3 scripts/validate_references.py` | **0** dangling references |
| `python3 scripts/check_migrations_additive.py` | **59** migrations, all additive |

## §6. Behaviour preservation

- **Package names unchanged for all 6 crates** — every consumer
  referencing a moved crate by package name continues to resolve
  identically.
- **Zero source-level `use` statement edits** — the dispatch packet
  pre-computed `0` `use` refs from outside each crate's own
  directory, and the final tree confirms it. Workspace dep entries
  were path-bumped only.
- **Compile-test executable hashes from `--no-run` confirm all 6
  moved crates and their downstream consumer (`corelink-server`)
  build to valid test binaries.** No test count regression possible
  because no source files were edited beyond Cargo.toml path bumps
  + 2 doc-comment text-only updates + CI-yaml path filters.
- **WASM target compiles** — `corelink-clerk-cf` (the CF Worker
  entry point that pulls in many of the consumer crates transitively)
  builds clean for `wasm32-unknown-unknown`.

## §7. Hard pause triggers — status

All 5 dispatch-listed hard pause triggers were monitored and **none
fired**:

| # | Trigger | Status |
|---|---|---|
| 1 | `cargo check` red after any `git mv` (hidden consumer) | **Did not fire.** All 6 sub-steps GREEN. (One intermediate transient: the relative-path `../corelink-client-verify` deps inside the moved Cargo.toml files surfaced as expected — collapsed to `workspace = true` in the same commit before staging.) |
| 2 | `git mv` failure | **Did not fire.** All 6 moves succeeded; rename detection % preserved per move (91–100%). |
| 3 | workspace.members update broke resolution | **Did not fire.** |
| 4 | Test count regression | **Did not fire.** No source edits that could change test count. |
| 5 | Step-0 baseline SHA mismatch with no recovery | **Recovered cleanly.** Initial `git rev-parse HEAD` returned `99269ed0` (stale Agent-tool worktree default). Ran `git fetch origin` + `git checkout -B wt/r-prep-w33-stage2-d-out-of-tree-v2 84a08f58`, re-verified `HEAD == 84a08f58`, and proceeded. This is precisely the failure mode that killed the prior 2.D dispatch — caught and recovered before any work. |

## §8. Next steps

1. **Stage 2.E — consumer migration** (sequential, after 2.B + 2.C +
   2.D merge): includes the deferred `corelink-d1-migrations` move
   (requires touching 6 downstream consumers) and any other
   consumer-import edits surfaced during the wave-33 unified
   layout.
2. **Stage 2.A — worker piece moves** (sequential, after 2.B + 2.C +
   2.D merge): the `corelink-worker` extraction follow-ons.
3. **`/techlead` pre-merge review** of `wt/r-prep-w33-stage2-d-out-of-tree-v2`
   per orchestrator protocol.

## §9. DCO + Co-Authored-By

All 6 sub-step commits + this SEAL commit carry:

- `Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>` (DCO via `git commit -s`)
- `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>` trailer

Branch: `wt/r-prep-w33-stage2-d-out-of-tree-v2`.
Baseline: `84a08f58`.
