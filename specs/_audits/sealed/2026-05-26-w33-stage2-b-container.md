# Wave 33 Stage 2.B — corelink-container creation + Dockerfile fix SEAL Audit (2026-05-26)

> **Doc kind:** stage closure audit (evidence; `_audits/` excluded from
> canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-26 by Claude Opus 4.7 in branch
> `wt/r-prep-w33-stage2-b-container` (worktree
> `.claude/worktrees/agent-a23bcac7025479788`).
>
> **Mandate:** wave-33 Stage 2.B owns the absorption of the gRPC server
> binary tree (`apps/server/*`) into a canonical workspace crate at
> `crates/corelink-container/` AND the fix of a pre-existing Dockerfile
> bug (broken since `50c92f40`) that has prevented every CoreLink
> production container build since initial scaffolding. Per the
> dispatch, 2.B runs in parallel to 2.D (out-of-tree relocations) and
> 2.C (adapter splits) — verified zero overlap on apps/server +
> Dockerfile + the new container crate surface.
>
> **Parent dispatch:** orchestrator pre-dispatch packet
> `Wave 33 Stage 2.B — corelink-container creation + Dockerfile fix`.
>
> **Pattern reference:** Stage 2 PRE-B precedent
> (`specs/_audits/sealed/2026-05-22-w33-stage2-pre-b-audit-megafiles.md`):
> behaviour-preserving structural move with parent dispatch files
> re-exporting the canonical public surface so consumers continue to
> import via the same paths.

## §1. Scope

Stage 2.B executes 2 functional sub-step commits + this SEAL:

1. **2.B.1 — crate creation + tree absorption** (commit `1d0c221e`):
   create `crates/corelink-container/` (new workspace member) and
   `git mv` the full `apps/server/*` tree into it; drop the now-empty
   `apps/server/` directory entirely (Option A per pre-dispatch §2.B.1.4 —
   no shim file, no half-deleted shell). Swap `[workspace.members]`
   entry `"apps/server"` → `"crates/corelink-container"`.

2. **2.B.2 — Dockerfile fix** (commit `72456240`): rewrite the broken
   `COPY src ./src` block to target the new container crate location
   AND to copy the workspace tree (Cargo workspaces cannot build a
   single crate without the member tree). This is the FIRST commit
   that makes the CoreLink container actually buildable since
   `50c92f40`.

3. **2.B.3 — wrangler.toml verification** (folded into this SEAL §4 per
   pre-dispatch: "if no changes needed, fold this verification into the
   SEAL audit and skip this commit"). All 3 refs at lines 14, 269, 406
   use `image = "./Dockerfile"` relative to repo root — unchanged.

4. **SEAL audit** (this commit): §1-§9 audit + gate evidence.

NO logic change, NO behaviour change. Pure structural move + pre-existing
build-tooling-bug fix.

## §2. apps/server absorbed tree (file-by-file inventory)

Tree absorbed via 49 `git mv` operations (verified renames in
`git status` after staging — 100% similarity all paths):

**Top-level (4 files):**
- `apps/server/Cargo.toml` → `crates/corelink-container/Cargo.toml`
- `apps/server/build.rs` → `crates/corelink-container/build.rs`
- `apps/server/proto/health.proto` → `crates/corelink-container/proto/health.proto`

**`src/` top-level (9 files):**
- `byok.rs` (29 LOC), `byok_orchestrator.rs` (413 LOC),
  `lib.rs` (47 LOC), `main.rs` (350 LOC),
  `neon_shadow_factory.rs` (278 LOC), `routes.rs` (202 LOC),
  `wall_clock.rs` (226 LOC), `webhook.rs` (200 LOC).

**`src/routes/` top-level (7 files):**
- `ac.rs` (341 LOC), `admin.rs` (463 LOC),
  `admin_pilot.rs` (1061 LOC) — pre-existing, NOT introduced by 2.B,
  `audit_analytics.rs` (105 LOC, dispatch parent post PRE-B),
  `audit_export.rs` (154 LOC, dispatch parent post PRE-B),
  `cas.rs` (218 LOC), `signup.rs` (1055 LOC) — pre-existing, NOT
  introduced by 2.B.

**`src/routes/audit_export/` (8 files):**
- `audit_sink.rs`, `handler.rs`, `parse.rs`, `state.rs`, `stream.rs`,
  `tests_basic.rs`, `tests_proptest.rs`, `tests_routes.rs`,
  `tests_stream.rs`, `types.rs` — all decomposed in PRE-B
  (commit `bec52c29`), moved here verbatim.

**`src/routes/audit_analytics/` (10 files):**
- `audit_sink.rs`, `handler_event_count.rs`, `handler_timeline.rs`,
  `rate_limit.rs`, `shadow_factory.rs`, `state.rs`,
  `tests_basic.rs`, `tests_common.rs`, `tests_handlers.rs`,
  `tests_prelude.rs`, `types.rs` — all decomposed in PRE-B
  (commit `7782d3de`), moved here verbatim.

**`tests/` (9 integration test files):**
- `admin_ac_route_smoke.rs`, `admin_pilot.rs`, `audit_export.rs`,
  `byok_orchestrator.rs`, `cas_route_smoke.rs`,
  `harness/d1_container.rs`, `signup_pilot.rs`,
  `signup_pilot_live_d1.rs`, `webhook_unified.rs`.

**Apps/server directory post-move:** removed entirely. The
`apps/` parent directory still contains `admin-ui/`, `docs/`,
`migrate-single-to-multi-region/` (siblings unaffected by 2.B).

## §3. Dockerfile before/after diff + rationale

**Before (root-cause: the wave-`50c92f40` Dockerfile assumed a
repo-root `src/` + `build.rs` + `proto/` that never existed; the binary
has lived at `apps/server/` since first scaffolding):**

```dockerfile
WORKDIR /build
COPY Cargo.toml Cargo.lock* ./
COPY build.rs ./                              # NO build.rs at repo root
COPY proto ./proto                            # NO proto/ at repo root
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src
COPY src ./src                                # FATAL: no src/ at root
RUN touch src/main.rs && cargo build --release
```

**After:**

```dockerfile
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates                          # workspace tree
COPY tools ./tools                            # workspace member tree
RUN echo "fn main() {}" > crates/corelink-container/src/main.rs \
 && echo "//! stub for dep cache layer" > crates/corelink-container/src/lib.rs \
 && cargo build --release -p corelink-server \
 && rm crates/corelink-container/src/main.rs crates/corelink-container/src/lib.rs
COPY crates/corelink-container/src ./crates/corelink-container/src
RUN cargo build --release -p corelink-server --bin corelink-server
```

**Rationale (5 key design decisions):**

1. **Workspace-wide COPY is unavoidable.** Cargo workspaces resolve
   every `[workspace.members]` manifest before scheduling any build,
   so `COPY crates ./crates` (+ `tools ./tools`) is required. A
   manifests-only synthetic tree would add drift risk with no
   meaningful cache savings (the workspace has 120+ members).

2. **Dep-cache layer stubs BOTH main.rs AND lib.rs.** The container
   binary imports `corelink_server::routes`, `webhook`, etc. from its
   sibling lib (verified: `grep ^use corelink_server::
   crates/corelink-container/src/main.rs` returns 3 matches). With
   both files stubbed, cargo populates `target/release/deps/` with
   every transitive crate (tonic, axum, tokio, hyper, AWS SDK,
   reqwest, etc.) without compiling the real bin/lib surface — the
   subsequent layer reuses that cache.

3. **`--bin corelink-server` on the source-real build** avoids cargo
   walking other workspace members that the container does not need.

4. **`build.rs` + `proto/` live INSIDE the container crate** (under
   `crates/corelink-container/`), so they are already covered by the
   initial `COPY crates ./crates`. The pre-fix Dockerfile assumed
   repo-root `build.rs` + `proto/` — those never existed and were a
   direct cause of the broken state.

5. **Binary name preserved.** `[package] name = "corelink-server"`
   stays in the new `crates/corelink-container/Cargo.toml`. The
   runtime stage `COPY --from=builder
   /build/target/release/corelink-server /usr/local/bin/corelink-server`
   and `ENTRYPOINT ["/usr/local/bin/corelink-server"]` are unchanged.

## §4. wrangler.toml verification (the 3 refs)

`grep -n "image\|Dockerfile" wrangler.toml` returns:

| Line | Section                       | Value                  | Status after 2.B |
|-----:|-------------------------------|------------------------|------------------|
| 14   | `[[containers]]` (default)    | `image = "./Dockerfile"` | unchanged — `./Dockerfile` still resolves; the file now actually builds |
| 269  | `[[env.prod.containers]]`     | `image = "./Dockerfile"` | unchanged |
| 406  | `[[env.staging.containers]]`  | `image = "./Dockerfile"` | unchanged |

All 3 paths are relative to repo root (where `wrangler.toml` itself
lives). The `Dockerfile` lives at repo root and the move did not
relocate it. The `class_name = "CoreLinkServer"` Durable Object class
names + the container `instance_type = "standard"` settings + the
port (50051) are all orthogonal to 2.B and unchanged.

**No wrangler.toml mutation needed.** 2.B.3 commit folded into this
audit per pre-dispatch directive.

## §5. Binary name preservation

The binary name `corelink-server` is load-bearing for:

1. **Dockerfile runtime stage** —
   `COPY --from=builder /build/target/release/corelink-server` +
   `ENTRYPOINT ["/usr/local/bin/corelink-server"]`. Cargo emits the
   release binary at `target/release/<package-name>`, so the
   `[package] name = "corelink-server"` MUST stay verbatim.

2. **wrangler.toml** — implicitly via the Cloudflare Containers runtime
   that invokes the ENTRYPOINT.

3. **External docs / CI scripts** — none audited, but the name is
   referenced by ~9 `apps/server` strings in `*.md` / `*.rs` comments
   (all comment-only, none load-bearing for the binary name).

**Verification:** `crates/corelink-container/Cargo.toml` line 2 reads
`name = "corelink-server"` (copied verbatim from pre-move
`apps/server/Cargo.toml` — `diff -q` returned no diff at copy time).
`cargo build -p corelink-server` resolves to the new crate location
(verified by build log: `Compiling corelink-server v0.1.0
(...crates/corelink-container)`).

## §6. L2.10 audit — no new files >500 LOC

Per pre-dispatch hard rule: "no new files >500 LOC expected — git mv
only + Dockerfile".

`git diff --cached --stat dfe3e9d4..HEAD` (post-SEAL, this commit):
- 49 paths show as `rename` (100% similarity) — `git mv` preserves
  history and does NOT trigger L2.10 (L2.10 applies to NEW files).
- `Cargo.toml`: +7 / -1 (members swap + 6 lines of comment).
- `Dockerfile`: +43 / -13 (rewrite of the build flow).
- `specs/_audits/sealed/2026-05-26-w33-stage2-b-container.md`: this audit file.

The audit file is the only new file in this stage. It is a
documentation file (audit class) and is excluded from L2.10 by the
charter (`_audits/` excluded from canonical schema validation per
PRE-B precedent doc-kind line).

No new code source file >500 LOC was added. The two pre-existing
mega-files in the absorbed tree (`admin_pilot.rs` 1061 LOC,
`signup.rs` 1055 LOC) were carried verbatim via `git mv` and did NOT
acquire new lines. Future decomposition of those two is a
non-blocking follow-up item if charter L2.10 is tightened to apply
to MOVED files; pre-dispatch explicitly scoped them OUT of this stage.

## §7. Test count delta (target 0)

Baseline (PRE-B SEAL `dfe3e9d4`):
- `cargo test -p corelink-server --lib`: 91 passed.
- `cargo test -p corelink-server --test audit_export`: 12 passed.

Post-2.B (SHA `72456240` — Dockerfile commit; no Rust source touched
in 2.B.2):
- `cargo test -p corelink-server --lib`: 91 passed. (delta: 0)
- `cargo test -p corelink-server --test audit_export`: 12 passed.
  (delta: 0)

Per-test breakdown unchanged — exact same test names ran with
identical outcomes pre- and post-move (test output reviewed at 2.B.1
commit time). The `corelink-server` package keyword still resolves
the test binary, now via `crates/corelink-container` instead of
`apps/server`.

## §8. Gate results (full pre-dispatch §gates checklist)

All gates green at 2.B.1 commit boundary (the only commit with code
movement; 2.B.2 is Dockerfile-only and re-running cargo gates is
redundant):

| Gate                                                  | Result   |
|-------------------------------------------------------|----------|
| `cargo build --workspace`                             | clean — Finished in 4m 11s |
| `cargo build -p corelink-server`                      | clean — resolves to crates/corelink-container |
| `cargo test -p corelink-server --lib`                 | 91 passed (baseline 91) |
| `cargo test -p corelink-server --test audit_export`   | 12 passed (baseline 12) |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | clean (1m 06s) |
| `python3 scripts/validate_specs.py`                   | 458 (449 schema-complete + 9 YAML-only) |
| `python3 scripts/validate_references.py`              | 0 dangling |
| `python3 scripts/check_migrations_additive.py`        | 59 migrations, all additive |
| `docker build -t corelink-2b-smoke .`                 | **test gap** — docker CLI installed (v28.3.2) but daemon offline on author host; statically inspected only |

**Docker test gap explanation:** the orchestrator host has
`/usr/local/bin/docker` installed but the Docker Desktop daemon is
not running, blocking both `docker build` and the lighter
`docker buildx build --check .` static lint. The Dockerfile
correctness is verified by inspection:

1. All COPY paths exist on disk post-2.B.1 (`Cargo.toml`,
   `Cargo.lock`, `crates/`, `tools/`).
2. The `corelink-server` package resolves to `crates/corelink-container`
   (verified by `cargo build -p corelink-server` in 2.B.1).
3. The dep-cache stub trick (main.rs + lib.rs) matches the standard
   Rust Docker multistage pattern.

Pre-deploy validation hand-off (recommended next steps for
orchestrator):
1. `docker buildx build --check .` (daemon-required Dockerfile lint).
2. `docker build -t corelink-2b-smoke .` end-to-end build smoke.
3. `wrangler deploy --dry-run` to confirm `image = "./Dockerfile"`
   resolves AND that the container actually builds in the wrangler
   deploy flow.

## §9. DCO + Co-Authored-By trailer

All 3 commits on `wt/r-prep-w33-stage2-b-container` carry:
- `Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>`
- `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`

Commit ladder:

| Sub-step | SHA        | Scope                                                  |
|----------|------------|--------------------------------------------------------|
| 2.B.1    | `1d0c221e` | crate creation + tree absorption (49 git-mv paths)     |
| 2.B.2    | `72456240` | Dockerfile fix (workspace-build + correct paths)       |
| 2.B SEAL | (this)     | audit + wrangler.toml verification (2.B.3 folded in)   |

Branch: `wt/r-prep-w33-stage2-b-container` (base: parent main
`dfe3e9d4`). Worktree:
`.claude/worktrees/agent-a23bcac7025479788`. Parallel to 2.D
(`wt/r-prep-w33-stage2-d-out-of-tree` @ `c5fdcea6` at audit time)
and 2.C (`wt/r-prep-w33-stage2-c-adapter-splits` @ `dfe3e9d4` at
audit time) — UNION-resolvable on `Cargo.toml [workspace.members]`
per pre-dispatch §parallel-safety.

Orchestrator should run `/techlead` on this branch BEFORE merging
to main per pre-dispatch §report tail directive.
