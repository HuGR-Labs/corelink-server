---
id: "DISPATCH-MATRIX-2026-05-27"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "2.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["dispatch", "engineering", "wave32", "parallel", "sota", "15-agents", "refined"]
references:
  - "specs/_audits/2026-05-27-deep-prep-inputs.md (1009 LOC inventory — source of truth for every pre-computed input below)"
  - "specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md"
  - "ROADMAP-TO-LAUNCH.md"
---

# 15-agent dispatch matrix — engineering refocus (v2.0 refined)

> **v2.0 changelog (2026-05-27):** v1.0 was template-grade. v2.0 grounds
> every WP in the 1009-LOC deep-prep inventory at
> `specs/_audits/2026-05-27-deep-prep-inputs.md` (commit `66ea10a8`).
> Every snippet / path / gate command below has been verified against
> current code. The 4 user-decided open questions resolved (see §0.8).
> The 23 orchestrator-decided questions resolved per inventory data.

## §0 Master constraints (apply to EVERY agent)

### 0.1 Pre-flight (mandatory first action)
```bash
pwd                                          # must end with agent-<task-id>
test -d ".git" -o -f ".git"                  # worktree marker present
cat .git 2>/dev/null | grep -q gitdir && echo "WORKTREE-OK" || echo "NOT-A-WORKTREE: ABORT"
```
If output is `NOT-A-WORKTREE: ABORT`, STOP immediately. Do not edit.

### 0.2 Charter — Rust
- `#![forbid(unsafe_code)]` at every new crate root
- `#[non_exhaustive]` on every new public enum + struct
- No `unwrap()` / `expect()` / `panic!` / `todo!()` / `unimplemented!()`
  outside `#[cfg(test)]`
- No `tokio` import in `src/` (only `#[cfg(test)]`)
- Audit emit ordering: `lookup → emit_audit → mutate_state`
- Secrets never logged (grep `tracing::|log::|println!|dbg!` + verify
  any secret-adjacent line redacts)
- `proptest!` configured via `proptest_cases(N)` helper

### 0.3 Charter — TypeScript / Next.js
- No `// @ts-ignore`, no `as any`, no `as unknown as`
- No `--no-verify`, no `eslint-disable`
- No `console.log` of any `process.env` value

### 0.4 Charter — GitHub Actions
- Every `uses:` SHA-pinned to 40-char commit hash (FF-HR-005)
- No floating tags (`@v4`, `@main`, `@latest`, `@stable`)

### 0.5 Charter — Python
- mypy clean (`python -m mypy <pkg>` exits 0)
- ruff clean (`python -m ruff check <pkg>` exits 0)
- pytest exits 0
- Type-hints required on all public functions

### 0.6 Commit + push policy
- Agent commits its own work BEFORE reporting SEAL
- Commit msg includes `Co-Authored-By:` trailer
- DO NOT use `--no-verify`
- DO NOT touch `main` directly
- ONLY push the worktree branch (orchestrator handles main merge)

### 0.7 SEAL report contract (every agent returns)
```
result: <one-line summary>
DoD: 1/✅ 2/✅ ... N/✅  (or ❌ + reason)
files changed: <absolute paths>
commit SHA: <full SHA>
external-repo changes: <if any, repo + SHA>
blockers: <list or NONE>
```

### 0.8 RESOLUTIONS (orchestrator decisions, locked-in)

User-decided (2026-05-27):
- **R1** [WP-7.3] Implement `corelink doctor` as new CLI command —
  diagnostic command following standard CLI UX (gh/rustup/brew). Moves
  into WP-2.1 scope (4 commands total).
- **R2** [WP-1.1] Achieve 70% coverage via miniflare integration tests
  for the Durable Object. NO per-file exempt gambiarra.
- **R3** [WP-5.1] Schema-correct path for `followup` type: add
  `followup` to `front_matter.schema.json` enum AND introduce
  `followup_status` enum (OPEN / IN_PROGRESS / CLOSED). No SKIP_ALL
  gambiarra.
- **R4** [WP-6.2] Add mypy as dev-dep + CI gate (SOTA SDK rigor).

Orchestrator-decided:
- **R5** [WP-1.1] Phase B gate "wrangler dev boots" interpreted as
  miniflare local; CF Containers beta path is Phase E territory.
- **R6** [WP-1.2] `wrangler.toml` is authoritative — script renames
  `PLACEHOLDER_PROD_*` → `PLACEHOLDER_*` to match.
- **R7** [WP-1.2] `corelink-prod-session-kv` + `corelink-prod-pilot-signup-kv`
  are reserved namespaces; bindings added to `wrangler.toml`
  `[[env.prod.kv_namespaces]]` blocks.
- **R8** [WP-1.2] `--dry-run` bypasses `.env.local` requirement.
- **R9** [WP-2.1] `cargo-init` writes project-local `.cargo/config.toml`
  (NOT user-global).
- **R10** [WP-2.1] `npm-init` targets `turbo.json` (most common); add
  `nx.json` as follow-up if customer asks.
- **R11** [WP-2.1] JSON upsert helper inline in `npm_init.rs` (not
  shared module — keeps blast radius low).
- **R12** [WP-5.1] `references:` uplift = WARNING-only retroactively;
  ERROR for new docs after the rule lands.
- **R13** [WP-5.2] Re-run includes all 3 crates (hash, byok,
  audit-chain). Floor: 80% kill-rate per DEBT-008.
- **R14** [WP-5.2] Full test run (no `--check`-only). Real baseline only.
- **R15** [WP-6.1] Dashboards are PER-TENANT with `tenant` label
  variable (matches existing dashboard pattern).
- **R16** [WP-6.2] Python `>=3.10` floor (broad community reach;
  httpx + pydantic both support 3.10).
- **R17** [WP-6.2] Package name `corelink` on import, `corelink-py` on
  PyPI metadata (private during MVP — no actual PyPI upload).
- **R18** [WP-7.1] Each public surface gets its own BetterStack monitor
  (5 monitors total). Worker-level `/health` AND server-level `/api/health`
  monitored separately for layered visibility.
- **R19** [WP-7.1] Use existing BetterStack policy ID from Phase A SEAL
  audit (read from `2026-05-22-w32-phaseA-betterstack-live.md`).
- **R20** [WP-7.2] CycloneDX-format SBOM (industry standard; auditor
  expectation). Output formats: JSON for tooling, XML for compliance.
- **R21** [WP-7.2] Sentry SDK FSSA license: server-side use only; no
  redistribution to end-users → commercial restriction does NOT apply.
  Document the reasoning in the license-allowlist update.
- **R22** [WP-7.3] Quickstart Step 1 = `corelink doctor` (now valid
  after WP-2.1 lands).
- **R23** [WP-7.3] All 4 locale translations updated in parallel with
  EN (no lag — pre-launch polish).
- **R24** [WP-1.1] CORS prototype pollution attack: closed via citing
  CF V8 fresh-isolate model + adding regression test for `Array.isArray`
  pre-check on `ALLOWED_ORIGINS`.
- **R25** [WP-1.1] URL %2F tenant-boundary: closed via integration
  test that asserts `tenant-A%2F../tenant-B` is rejected with 400 before
  reaching auth middleware.
- **R26** [WP-2.1] `cargo-init` checks `sccache --version` and emits
  actionable error if missing (does NOT auto-install).
- **R27** [WP-7.1] `corelink-signup.humangr.com` and
  `corelink-admin.humangr.com` get separate monitors (R18) even though
  same Worker — different routes, different SLOs.

---

## §1 WP catalog × conflict matrix

```
CONTEXT 1 — Wave 32 prod-deploy advance (2 agents)
  WP-1.1  Phase B  worker-shim audit + miniflare DO integration + adversarial review
  WP-1.2  Phase C  CF provisioning idempotency + placeholder rename + missing bindings

CONTEXT 2 — CLI feature expansion (1 agent — expanded scope)
  WP-2.1  corelink-cli: cargo-init + npm-init + docker-init + doctor

CONTEXT 3 — Comparison pages remaining (3 agents)
  WP-3.1  vs bazel-remote+S3        (1500-2500 words)
  WP-3.2  vs sccache+S3              (1500-2500 words)
  WP-3.3  vs Turborepo Remote Cache  (1500-2500 words)

CONTEXT 4 — Educational content cadence (2 agents)
  WP-4.1  Blog #2 — BLAKE3 internals + why we picked it
  WP-4.2  Blog #3 — Audit-chain re-derivation walkthrough

CONTEXT 5 — Engineering quality (2 agents)
  WP-5.1  validate_specs schema extension (followup type+lifecycle) + uplift
  WP-5.2  cargo-mutants baseline (hash + byok + audit-chain @ 80%+ floor)

CONTEXT 6 — Observability + SDK (2 agents)
  WP-6.1  Grafana SLO dashboards (real metric names from corelink-telemetry)
  WP-6.2  Python SDK MVP (3 ops from openapi-corelink-v1.yaml + mypy gate)

CONTEXT 7 — Production-readiness hardening (3 agents)
  WP-7.1  BetterStack synthetic probes (5 flat-name URLs verified)
  WP-7.2  SBOM CycloneDX + license audit (Phase 1 deps) via cargo-deny + cdxgen
  WP-7.3  Quickstart docs refresh aligned to CLI MVP + corelink doctor
```

### Conflict matrix (no shared-file collisions)

```
                worker/   scripts/   apps/docs/   apps/docs/   specs/    crates/   sdks/   dashboards/  monitoring/  .sbom/   marketing/  external
                                     compare/     blog/                                                                                     repo
WP-1.1  █                                                       ░audit                                                                      
WP-1.2            █                                                                                                                          
WP-2.1                                                                                                                                       corelink-cli
WP-3.1                              ░vs-bazel..                  ░audit                                                                      
WP-3.2                              ░vs-sccache                  ░audit                                                                      
WP-3.3                              ░vs-turbo                    ░audit                                                                      
WP-4.1                                          ░2026-05-28..    ░audit                                                                      
WP-4.2                                          ░2026-05-29..    ░audit                                                                      
WP-5.1                                                           █(schema)                                                                   
WP-5.2                                                                    ░mutants.out                                                       
WP-6.1                                                           ░audit                                          █                           
WP-6.2                                                           ░audit             █                                                         
WP-7.1            ░one-script                                                                       █                                         
WP-7.2            ░one-script                                                                                   █                            
WP-7.3                                          ░docs/tutorial                                                                                

█ = primary edit zone   ░ = scoped subdir/file
```

**Hot zones + safety:**
- `apps/docs/blog/tags.yml` (WP-4.1 + WP-4.2): each agent APPENDS new
  tag rows only, never edits existing. Orchestrator union-merges.
- `specs/_audits/` (every WP creates a SEAL doc): all SEAL doc names
  are unique (`2026-05-27-<wp-slug>-seal.md`). No collision.
- `scripts/`: WP-1.2 and WP-7.1/7.2 all add scripts. Each agent only
  CREATES new files in `scripts/`, never edits existing.

---

## §2 Per-WP contracts (v2.0 refined)

> Each contract is self-contained per `feedback_prompts_mastigados`.
> All `INPUTS` sections cite the deep-prep inventory by §.

---

### WP-1.1 — Phase B worker-shim audit + miniflare DO integration + adversarial review

**CONTEXT:** Wave 32 Phase B per
`specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md` §Phase B.
Worker shim at `worker/` partially landed; needs coverage uplift +
adversarial review to seal Phase B.

**SCOPE:**
- `worker/src/{index.ts,durable_object.ts,rollout_controller.ts}`
- `worker/tests/*.ts` + NEW `worker/tests/integration/do.miniflare.test.ts`
- `worker/vitest.config.mts` (per-file thresholds; NOT lowering global)
- `worker/package.json` (add `miniflare` + `@miniflare/cf` dev-deps if absent)
- `worker/tsconfig.json` if integration test imports require config updates
- `wrangler.toml` uncomment `main = "worker/src/index.ts"` line (if commented)
- NEW: `specs/_audits/2026-05-27-w32-phaseB-worker-shim-seal.md`

**NON-SCOPE:**
- `apps/` (any subdir)
- `crates/` (any subdir)
- Real CF infra (Phase C territory)

**INPUTS (verified by deep-prep §1):**

Current state:
- `worker/src/index.ts` — main handler, exports `default { fetch }`; CORS at line 106
- `worker/src/durable_object.ts` — `CoreLinkServer` DO class; 40.55% coverage
- `worker/src/rollout_controller.ts` — feature-flag-style rollout
- `worker/tests/`: index.test.ts (517 LOC), integration.test.ts (312 LOC),
  rollout_controller.test.ts (114 LOC), setup.ts (16 LOC)
- vitest current global threshold: **65%** (lines 38-50 of vitest.config.mts)
- Current global coverage: **67.32%**; index.ts 94.64%; DO 40.55%

Gap → fix:
- Global must reach ≥70% → DO coverage needs lift via miniflare
  integration tests. R2 user decision: write integration tests, NO
  per-file exempt.
- miniflare lets us exercise `container.start()`, `getTcpPort()`,
  `container.running`, `container.destroy()` in a local CF-runtime
  emulator. Need ≥4 integration tests covering each container API call.

5 adversarial attacks (per inventory §1 lines 70-80, with R24+R25 resolutions):
1. **Tenant boundary `%2F` bypass** (index.ts URL parsing) → R25 closes
   via integration test asserting `tenant-A%2F../tenant-B` → 400
2. **CORS prototype pollution** (index.ts:106) → R24 closes via
   `Array.isArray(ALLOWED_ORIGINS)` pre-check + regression test
3. **DO storage corruption** (durable_object.ts) → miniflare integration
   test simulates partial write + asserts recovery
4. **Container race on cold start** (durable_object.ts) → test asserts
   serialized startup via `state.blockConcurrencyWhile`
5. **Malformed gRPC frame from container** (durable_object.ts) → unit
   test with mocked frame asserts graceful error mapping

**DOD:**
1. `cd worker && pnpm test --coverage` shows ≥70% global line coverage
2. `worker/tests/integration/do.miniflare.test.ts` exists, ≥4 tests
3. `cd worker && pnpm test` exit 0 (all green)
4. `cd worker && pnpm exec tsc --noEmit` exit 0
5. `cd worker && pnpm exec wrangler dev --local --port 8787 &` boots
   (use `worker/node_modules/.bin/wrangler`; wrangler is NOT global —
   inventory §1 line 67-68); `curl localhost:8787/health` → 200; kill bg
6. `python3 scripts/validate_specs.py` exit 0
7. SEAL doc covers: file inventory, before/after coverage, 5 attacks
   each closed with code-line citation OR accepted-risk paragraph
8. Single commit on worktree branch

**ACCEPTANCE GATES:**
```bash
cd worker
pnpm install --frozen-lockfile=false 2>&1 | tail -3
pnpm test --coverage 2>&1 | tail -20
pnpm exec tsc --noEmit 2>&1 | tail -3
# Background wrangler smoke:
pnpm exec wrangler dev --local --port 8787 &
WRANGLER_PID=$!
sleep 5
curl -sI localhost:8787/health | head -1   # expect HTTP/1.1 200
kill $WRANGLER_PID 2>/dev/null
cd ..
python3 scripts/validate_specs.py 2>&1 | tail -3
```

**COMMIT MSG TEMPLATE:**
```
seal(w32-phaseB): worker-shim audit + miniflare DO integration + adversarial review

Wave 32 Phase B sealed per spec sealed/2026-05-22-wave32-prod-deploy-spec.md §B.

Coverage: <before>% → <after>% global (target ≥70 reached). Added
worker/tests/integration/do.miniflare.test.ts with <N> tests
exercising container start/destroy + DO storage + grpc frame mapping.

Adversarial review: 5/5 attacks closed.
- Tenant %2F boundary (R25): integration test
- CORS prototype (R24): Array.isArray pre-check + regression test
- DO storage corruption: miniflare partial-write test
- Container cold-start race: blockConcurrencyWhile assertion
- Malformed gRPC frame: graceful error-mapping unit test

Audit: specs/_audits/2026-05-27-w32-phaseB-worker-shim-seal.md.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

### WP-1.2 — Phase C CF provisioning idempotency + placeholder rename + missing bindings

**CONTEXT:** Wave 32 Phase C. Script `scripts/provision-cf-corelink-prod.sh`
exists but has 3 problems flagged in inventory §2: PLACEHOLDER naming
mismatch (HIGH-2 blocker), 2 KV bindings absent from wrangler.toml,
unsupported `--dry-run` flag.

**SCOPE:**
- `scripts/provision-cf-corelink-prod.sh`
- `scripts/teardown-cf-corelink-prod.sh`
- `wrangler.toml` (only the `[[env.prod.kv_namespaces]]` blocks — add
  the 2 missing bindings for session-kv + pilot-signup-kv)
- NEW: `specs/_audits/2026-05-27-w32-phaseC-provisioning-seal.md`

**NON-SCOPE:**
- Any source code outside `scripts/` and the 2 wrangler.toml binding
  blocks
- Any live `curl` to api.cloudflare.com (Owner-only)
- Substantive wrangler.toml changes (just the 2 binding additions)

**INPUTS (verified by deep-prep §2):**

Current script state:
- `scripts/provision-cf-corelink-prod.sh` exists; current LOC ~unknown
  (read file to get exact count before changes)
- Current bash flags: NO `--dry-run`, NO `--help` (inventory §2)
- Idempotency pattern: PARTIAL — name-match-then-PATCH for D1, but
  KV/R2 missing this pattern
- Token-scope validation: MISSING
- `shellcheck` count: read full output during agent execution

PLACEHOLDER mismatch (HIGH-2 blocker — inventory §2 Q1):
- Script uses: `PLACEHOLDER_PROD_D1_CONFIG_DB_ID`, `PLACEHOLDER_PROD_*`
- wrangler.toml uses: `PLACEHOLDER_D1_CONFIG_DB_ID`, `PLACEHOLDER_*`
- **R6 resolution:** wrangler.toml is authoritative. Rename script
  identifiers `PLACEHOLDER_PROD_*` → `PLACEHOLDER_*`.

12 resources to provision (per Wave 32 §Phase C, mapped from inventory §2):
- D1: `corelink-prod-d1` (1)
- KV: `corelink-prod-jwks-kv`, `corelink-prod-cache-kv`,
  `corelink-prod-rate-limit-kv`, `corelink-prod-session-kv`,
  `corelink-prod-pilot-signup-kv` (5)
- R2: `corelink-cas-prod`, `corelink-ac-sam`, `corelink-ac-iad`,
  `corelink-ac-lhr`, `corelink-ac-nrt`, `corelink-ac-syd` (6)

wrangler.toml MISSING bindings (inventory §2 Q2 → R7 resolution):
- session-kv → add `[[env.prod.kv_namespaces]]` with `binding =
  "SESSION_KV"`, `id = "PLACEHOLDER_SESSION_KV_ID"`
- pilot-signup-kv → add with `binding = "PILOT_SIGNUP_KV"`,
  `id = "PLACEHOLDER_PILOT_SIGNUP_KV_ID"`

Reference idempotency pattern: `scripts/statuspage-bootstrap.sh` (Phase A,
sealed). Mirror its name-match-then-PATCH.

**DOD:**
1. `bash scripts/provision-cf-corelink-prod.sh --help` exits 0, shows
   `--dry-run`, `--validate-token`, `--help` flags
2. `bash scripts/provision-cf-corelink-prod.sh --dry-run` exits 0,
   prints the 12 resources, NO API calls made (R8: no `.env.local` req
   for dry-run)
3. Script handles re-run idempotency on all 12 resources (D1 + 5 KV + 6 R2)
4. Teardown script symmetric with `--dry-run`
5. Both scripts pass `shellcheck` clean (zero warnings)
6. `wrangler.toml` has 2 new `[[env.prod.kv_namespaces]]` blocks (session
   + pilot-signup) with PLACEHOLDER_* IDs
7. All script PLACEHOLDER references match wrangler.toml exactly
   (`grep -rn "PLACEHOLDER" scripts/provision-cf-corelink-prod.sh
   wrangler.toml` shows aligned names)
8. SEAL doc includes: token-scope checklist, dry-run output capture,
   D-day execution checklist, rollback ladder
9. Single commit on worktree

**ACCEPTANCE GATES:**
```bash
shellcheck scripts/provision-cf-corelink-prod.sh scripts/teardown-cf-corelink-prod.sh
bash scripts/provision-cf-corelink-prod.sh --help 2>&1 | head -20
bash scripts/provision-cf-corelink-prod.sh --dry-run 2>&1 | head -30
bash scripts/teardown-cf-corelink-prod.sh --dry-run 2>&1 | head -30
# Verify PLACEHOLDER alignment:
diff <(grep -oE "PLACEHOLDER_[A-Z_]+" scripts/provision-cf-corelink-prod.sh | sort -u) \
     <(grep -oE "PLACEHOLDER_[A-Z_]+" wrangler.toml | sort -u)
# (Empty diff = perfectly aligned)
```

**COMMIT MSG TEMPLATE:**
```
seal(w32-phaseC): provisioning idempotency + placeholder rename + missing KV bindings

Wave 32 Phase C hardened (resolves inventory §2 HIGH-2 blocker).

- PLACEHOLDER_PROD_* → PLACEHOLDER_* (alignment with wrangler.toml as
  source-of-truth)
- Added 2 missing [[env.prod.kv_namespaces]] blocks: SESSION_KV +
  PILOT_SIGNUP_KV (both PLACEHOLDER IDs; D-day script fills them)
- --dry-run + --validate-token + --help flags added
- Name-match-then-PATCH idempotency on all 12 resources
- Symmetric teardown script
- shellcheck clean on both

Audit: specs/_audits/2026-05-27-w32-phaseC-provisioning-seal.md.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

### WP-2.1 — corelink-cli: cargo-init + npm-init + docker-init + doctor (EXPANDED)

**CONTEXT:** External repo `humangr-labs/corelink-cli @ 71d7f6a9` ships
MVP (`ping` + `bazel-init`). This WP adds 4 new commands. The new
`doctor` command (per R1 user decision) replaces the placeholder
quickstart step referenced in WP-7.3.

**SCOPE (external repo only):**
- Clone `humangr-labs/corelink-cli` to `/tmp/corelink-cli-wp21-<task-id>/`
- Add `src/commands/cargo_init.rs`
- Add `src/commands/npm_init.rs`
- Add `src/commands/docker_init.rs`
- Add `src/commands/doctor.rs`
- Update `src/main.rs` clap subcommand registration (4 new)
- Update `src/commands/mod.rs` exports
- Tests in `tests/` (≥3 per command: detect-success, detect-miss,
  already-applied-noop)
- Update `README.md` usage
- Push to external `main`
- MONOREPO worktree: SEAL audit at
  `specs/_audits/2026-05-27-cli-polyglot-commands-seal.md`

**NON-SCOPE:**
- Monorepo source (only the SEAL audit)
- Real network calls in tests (use tempdir fixtures)
- Auto-installing sccache / docker / etc. (only diagnose + emit hint)

**INPUTS (verified by deep-prep §3):**

Existing template patterns:
- Marker-based block in `bazel_init.rs` (inventory §3 lines 197-198):
  ```
  MARKER_BEGIN = "# corelink-managed (do not edit between markers)"
  MARKER_END = "# /corelink-managed"
  ```
- `upsert_block` function (inventory §3 line 198) at `src/commands/
  bazel_init.rs` lines 55-76 — idempotent. Reuse for cargo/docker (TOML/
  shell-style files).
- For JSON files (`turbo.json` in npm-init): R11 — inline JSON upsert
  helper in `npm_init.rs`.

Per-command idiomatic config:

**cargo-init** (per R9 → project-local, inventory §3 lines 217-245):
- Detect file: `Cargo.toml` (project root)
- Target: `.cargo/config.toml` (project-local, NOT user-global)
- Snippet to append (between markers):
  ```toml
  [build]
  rustc-wrapper = "sccache"

  [env]
  SCCACHE_ENDPOINT = "<endpoint-arg>"
  SCCACHE_BUCKET = "corelink"
  SCCACHE_AUTH_TYPE = "bearer"
  SCCACHE_AUTH_TOKEN = "<token-arg>"
  ```
- R26: `cargo-init` must check `sccache --version`; if absent,
  print actionable error: `sccache binary not found; install via
  cargo install sccache OR brew install sccache; CoreLink uses sccache
  as the cargo-side cache wrapper.`
- Vendor cite: https://github.com/mozilla/sccache#configuring-cache-storage

**npm-init** (per R10 turbo.json target, inventory §3 lines 247-275):
- Detect file: `package.json`
- Target: `turbo.json` (create if missing; merge if exists)
- Snippet (use inline JSON upsert helper):
  ```json
  {
    "remoteCache": {
      "signature": true,
      "enabled": true,
      "apiUrl": "<endpoint-arg>",
      "token": "<token-arg>",
      "teamId": "corelink"
    }
  }
  ```
- Vendor cite: https://turbo.build/repo/docs/core-concepts/remote-caching
- R11: JSON upsert inline in `npm_init.rs` — read file, parse, merge
  the `remoteCache` key, re-serialize. Idempotency: detect existing
  `remoteCache.apiUrl == <endpoint-arg>` → noisy log + exit 0.

**docker-init** (inventory §3 lines 276-300):
- Detect file: `Dockerfile`
- Target: `.docker/buildx-cache.json` (create) + `.gitignore` line
- Snippet `.docker/buildx-cache.json`:
  ```json
  {
    "remoteCache": {
      "type": "registry",
      "ref": "<endpoint-arg>/cache",
      "auth": {
        "type": "bearer",
        "token": "<token-arg>"
      }
    },
    "buildkit": {
      "cacheFrom": "type=registry,ref=<endpoint-arg>/cache",
      "cacheTo": "type=registry,ref=<endpoint-arg>/cache,mode=max"
    }
  }
  ```
- `.gitignore` append (between markers): `.docker/buildx-cache.json`
  IF it contains a token; else commit it (token in env, not file).
- Vendor cite: https://docs.docker.com/build/cache/backends/registry/

**doctor** (per R1 — NEW command, inventory §10 Q1+Q2):
- Subcommand structure: `corelink doctor [--verbose]`
- Checks (each prints ✓ / ✗ with hint):
  1. CLI version + build target (always ✓)
  2. `CORELINK_ENDPOINT` env var present + URL valid
  3. `CORELINK_TOKEN` env var present + token format check (starts `ct_`)
  4. Endpoint reachable: POST /v1/ping with 5s timeout
  5. Auth valid: response 200 (not 401/403)
  6. If `bazel-init` was run (detect `.bazelrc` corelink-managed block):
     verify `bazel` is on PATH
  7. If `cargo-init` was run: verify `sccache --version`
  8. If `npm-init` was run: verify `turbo --version`
  9. If `docker-init` was run: verify `docker buildx version`
- Exit code: 0 if all green, 1 if any ✗
- Verbose mode: prints HTTP request/response bodies, env values
  (REDACTED for token), full ✓/✗ table.

**DOD:**
1. `cargo check` exit 0 (in cloned cli dir)
2. `cargo build --release` exit 0
3. `cargo test` exit 0 (≥3 tests per command × 4 commands = ≥12 new tests)
4. `./target/release/corelink --help` shows all 4 new commands
5. `./target/release/corelink doctor` runs end-to-end (without endpoint
   configured, prints ✗ for endpoint check with hint, exit 1)
6. All GHA `uses:` SHA-pinned
7. README usage section updated with all 4 commands
8. Pushed to external repo `main`
9. SEAL audit in monorepo worktree

**ACCEPTANCE GATES (cloned cli dir):**
```bash
cd /tmp/corelink-cli-wp21-<task-id>
cargo check 2>&1 | tail -3
cargo build --release 2>&1 | tail -3
cargo test 2>&1 | tail -15
./target/release/corelink --help | grep -E "cargo-init|npm-init|docker-init|doctor"
./target/release/corelink doctor 2>&1 | head -20   # should print check table
grep -E "uses:" .github/workflows/*.yml | grep -vE "[a-f0-9]{40}"   # must be empty
git log -1 --oneline
git push origin main 2>&1 | tail -3
```

**COMMIT MSG TEMPLATE (monorepo audit doc):**
```
docs(specs): SEAL audit for corelink-cli polyglot commands + doctor

ROADMAP Phase 1.3 expansion: cargo-init + npm-init + docker-init +
doctor added to corelink-cli (4 new subcommands).

- cargo-init: project-local .cargo/config.toml with sccache wrapper
- npm-init: turbo.json remoteCache block (JSON inline upsert)
- docker-init: .docker/buildx-cache.json (BuildKit registry-cache)
- doctor: 9-point environment diagnostic (auth, endpoint, deps)

All commands idempotent (re-run = noisy noop). 12 new tests.
doctor pairs with the new WP-7.3 quickstart Step 1.

External repo: humangr-labs/corelink-cli @ <new-SHA>.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

### WP-3.1 — Comparison page: CoreLink vs bazel-remote + S3

**CONTEXT:** ROADMAP §6 Phase 4.6. 3/6 comparison pages done.
This WP delivers vs bazel-remote (Bazel REAPI cache, open-source).

**SCOPE:**
- `apps/docs/src/pages/compare/vs-bazel-remote-s3.mdx` (NEW)
- `apps/docs/src/pages/compare/index.mdx` if exists OR
  `apps/docs/docusaurus.config.ts` footer compare-list (LOCATE first)
- `apps/docs/i18n/{pt-BR,es-419,de}/.../vs-bazel-remote-s3.mdx`
- NEW: `specs/_audits/2026-05-27-compare-bazel-remote-s3-seal.md`

**NON-SCOPE:** Any other compare page; docs config beyond compare-list.

**INPUTS:** Existing patterns at `apps/docs/src/pages/compare/vs-{buildbuddy,engflow,nx-cloud}.mdx`. Match their structure exactly.

**STRUCTURE (mandatory sections, mirror existing comparisons):**
1. TL;DR (3 bullets)
2. What is bazel-remote (1 paragraph)
3. Where bazel-remote wins (≥3 wins)
4. Where CoreLink wins (≥4 wins, charter-cited)
5. Cost comparison at 3 scales (10 GB / 500 GB / 5 TB) with concrete
   numbers (S3 storage $0.023/GB/mo; egress $0.09/GB; CoreLink Free/Pro/
   Enterprise per `apps/docs/src/lib/pricing.ts`)
6. When to use which (3-question decision tree)
7. Migration path (parallel-run pattern)
8. Sources bibliography

**TONE:** Honest. bazel-remote is excellent for its niche. Cite charter
sources for every CoreLink claim.

**DOD:**
1. `wc -w` on rendered text 1500-2500 (excluding code blocks)
2. `cd apps/docs && pnpm build` exit 0 (4 locales)
3. 4 locales present (translated stubs OK with "Coming soon" + canonical)
4. Page linked from compare index/footer
5. Every external URL returns HTTP 200 (curl-verify in your acceptance run)
6. SEAL audit committed
7. Single commit on worktree

**ACCEPTANCE GATES:**
```bash
cd apps/docs && pnpm build 2>&1 | tail -5
wc -w src/pages/compare/vs-bazel-remote-s3.mdx
grep -oE "https://[^ )]+" src/pages/compare/vs-bazel-remote-s3.mdx | sort -u | head -20 | \
  while read u; do printf "%-80s " "$u"; curl -sI --max-time 8 "$u" | head -1; done
```

**COMMIT MSG TEMPLATE:** *(same shape as v1.0)*

---

### WP-3.2 — Comparison page: CoreLink vs sccache + S3

Same contract shape as WP-3.1 with substitutions:
- Path: `apps/docs/src/pages/compare/vs-sccache-s3.mdx`
- Subject: sccache (Mozilla compiler cache)
- bazel-remote wins → sccache wins (free, simple, well-known per
  inventory §3 sccache reference)
- CoreLink wins → multi-language (not just compiler caches), audit
  log, BYOK, no S3 ops, REAPI compatibility
- Cost: sccache+S3 at 3 scales vs CoreLink
- Migration: parallel-run (sccache local fallback + CoreLink primary)
- SEAL: `specs/_audits/2026-05-27-compare-sccache-s3-seal.md`

---

### WP-3.3 — Comparison page: CoreLink vs Turborepo Remote Cache

Same contract shape with:
- Path: `apps/docs/src/pages/compare/vs-turborepo.mdx`
- Subject: Turborepo Remote Cache (Vercel)
- Turbo wins: free with Vercel, auto-on, npm-native
- CoreLink wins: not Vercel-coupled, multi-language, audit log, BYOK,
  residency honesty
- Cost: Vercel Pro pricing vs CoreLink at equivalent
- Migration: REAPI shim for Turborepo workspaces
- SEAL: `specs/_audits/2026-05-27-compare-turborepo-seal.md`

> Per competitive audit §3.4, Turborepo is a "lose segment" for already-on-Vercel teams. Frame honest: "if you're on Vercel, Turborepo wins. If not, or you need multi-language..."

---

### WP-4.1 — Blog post #2: BLAKE3 internals + why we picked it

**CONTEXT:** ROADMAP §6 Phase 4.7 cadence. Blog #1 (CF Workers
architecture) live. Blog #2 covers hash choice.

**SCOPE:**
- `apps/docs/blog/2026-05-28-why-blake3.mdx` (NEW)
- `apps/docs/blog/tags.yml` (APPEND `cryptography` + `blake3` tags;
  do NOT edit existing)
- NEW: `specs/_audits/2026-05-27-blog-2-blake3-seal.md`

**INPUTS (verified by deep-prep §5):**

Real crate state to ground the post:
- `crates/corelink-hash/src/digest.rs` — `Digest::compute`,
  `Digest::from_hex`, `Digest::to_hex`, `Digest::verify_constant_time`,
  `Digest::as_bytes`
- `crates/corelink-hash/src/verified_body.rs` — `VerifiedBody::new`,
  `VerifiedBody::body`, `VerifiedBody::digest`, `VerifiedBody::into_parts`
- `crates/corelink-hash/src/store.rs` — `BlobStoreWrite` trait
- BLAKE3 paper: https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.pdf

**STRUCTURE:**
1. Hook — concrete CAS-cache problem
2. What is a content-addressable hash (200w, accessible)
3. The candidates — SHA-256, BLAKE2, BLAKE3, xxHash comparison table
4. BLAKE3 internals — Merkle tree, SIMD path, derive-key mode (ASCII
   diagrams OK)
5. Benchmarks — BLAKE3 paper numbers + reproducible (cite
   `cargo bench -p corelink-hash`)
6. Why CoreLink picked BLAKE3 — 4-5 bullets grounded in `corelink-hash`
   code (parallel hashing for large blobs, derive-key for tenant
   namespacing, no length-extension)
7. What we'd do differently — ≥2 honest "if we started over" thoughts
8. References — BLAKE3 paper, repo, file:line citations
   (`crates/corelink-hash/src/digest.rs:42`-style)

**DOD:**
1. `wc -w` 3000-5000
2. `cd apps/docs && pnpm build` exit 0
3. Post HTML at `apps/docs/build/blog/why-blake3/index.html` (or
   locale variants)
4. Tags appended to `tags.yml` (no existing-row edits)
5. SEAL audit committed
6. Single commit on worktree

**ACCEPTANCE GATES:**
```bash
wc -w apps/docs/blog/2026-05-28-why-blake3.mdx
cd apps/docs && pnpm build 2>&1 | tail -5
ls build/blog/ | grep -i blake3
```

---

### WP-4.2 — Blog post #3: Audit-chain re-derivation walkthrough

Same shape as WP-4.1 with:
- Path: `apps/docs/blog/2026-05-29-audit-chain-walkthrough.mdx`
- Subject: RFC-6962-inspired audit chain
- Tags appended: `audit`, `merkle`, `rfc-6962`
- Code refs: `crates/corelink-audit-chain/` (per inventory)
- Structure: ICP compliance problem hook → CT background → CoreLink
  design → re-derivation procedure (runnable independent of SaaS) →
  attacks it resists / doesn't resist → cost
- SEAL: `specs/_audits/2026-05-27-blog-3-audit-chain-seal.md`

---

### WP-5.1 — validate_specs schema extension (followup type + lifecycle) + uplift

**CONTEXT:** R3 user decision: SOTA path = extend schema to recognize
followup type + introduce `followup_status` enum (OPEN / IN_PROGRESS /
CLOSED) for tracking action items distinct from immutable audits.

**SCOPE:**
- `specs/_schemas/front_matter.schema.json` (extend enums)
- `scripts/validate_specs.py` (recognize new enum + uplift)
- `specs/_followups/2026-05-27-cosmetic-followups.md` (revert my
  gambiarra fix `type: audit` → proper `type: followup` +
  `followup_status: OPEN`)
- NEW: `specs/_audits/2026-05-27-validate-specs-uplift-seal.md`

**NON-SCOPE:** Substantive content changes to other specs; only
frontmatter edits to align with new schema.

**INPUTS (verified by deep-prep §4):**

Current state:
- `specs/_schemas/front_matter.schema.json` (inventory §4 lines 338-345)
- `scripts/validate_specs.py` rules extracted
- Current pass: 449 OK + 9 YAML-only = 458 total; 0 failures (after
  my fix)
- Failing-doc path before fix: `specs/_followups/2026-05-27-cosmetic-
  followups.md` (I patched with gambiarra: type:audit + audit_status:ACTIVE
  + 5 required fields)

R3 SOTA changes:
1. Add `followup` to schema `type` enum
2. Add new optional field `followup_status` enum: OPEN, IN_PROGRESS, CLOSED
3. When `type: followup`, `audit_status` is OPTIONAL (followups have
   their own lifecycle)
4. Revert cosmetic-followups doc to use:
   ```yaml
   type: followup
   followup_status: OPEN
   doc_status: ACTIVE
   ```
   (no audit_status field)

R12 strictness uplift (inventory §4 line 350-351):
- Rule: docs with `type: audit` lacking `references:` list → WARNING
  (retroactive). New audit docs after this rule lands → ERROR.
- Estimated affected docs: ~30 audit docs lack `references:` (per
  inventory). Mark each with `references: []` if appropriate OR add
  the WARNING annotation in validator output.

**DOD:**
1. `python3 scripts/validate_specs.py` exits 0
2. `python3 scripts/validate_references.py` exits 0
3. Schema validates: `cat specs/_schemas/front_matter.schema.json |
   python3 -c "import sys, json; json.load(sys.stdin); print('OK')"`
4. Cosmetic followups doc uses `type: followup` + `followup_status: OPEN`
5. WARNING (not ERROR) for retroactive `references:` gap on existing
   audits
6. SEAL doc documents: schema delta + uplift rule + retroactive
   policy
7. Single commit on worktree

**ACCEPTANCE GATES:**
```bash
python3 -c "import json; json.load(open('specs/_schemas/front_matter.schema.json'))"
python3 scripts/validate_specs.py 2>&1 | tail -5
python3 scripts/validate_references.py 2>&1 | tail -5
grep -E "^type|^followup_status" specs/_followups/2026-05-27-cosmetic-followups.md
```

---

### WP-5.2 — cargo-mutants baseline (hash + byok + audit-chain)

**CONTEXT:** ROADMAP §6 + DEBT-008. Prior baseline on corelink-hash =
**97.22%** confirmed in inventory §5 lines 417-477. WP extends to byok
+ audit-chain at 80% floor (R13).

**SCOPE:**
- `mutants.out/` (gitignored, ephemeral run output)
- NEW: `specs/_audits/2026-05-27-mutation-testing-baseline-seal.md`
- Optional: `.cargo/mutants.toml` for skip patterns IF needed

**NON-SCOPE:** Test-code changes (this is BASELINE measurement; follow-ups separate); non-target crates.

**INPUTS (verified by deep-prep §5):**

Installation state:
- `which cargo-mutants` — read output during agent execution
- `cargo mutants --version` — read output
- Prior baseline doc: `specs/_audits/sealed/...` (find by
  `grep -rn "97.22\|cargo-mutants" specs/_audits/sealed/ 2>/dev/null | head`)

Target crates (per inventory §5):
- `crates/corelink-hash/` — LOC + file count + public fn count from
  inventory lines 402-404
- `crates/corelink-audit-chain/` — read state
- `crates/corelink-byok/` — read state

R14: full test run (no `--check`-only). Real baseline.
Conservative `-j` value: 2 (per inventory §5 line 493+).

Estimated total runtime: 60-180 min depending on parallelism. Agent
runs in background AS LONG AS NEEDED; reports progress in SEAL audit.

**DOD:**
1. `cargo mutants -p corelink-hash --timeout 120 -j2` completes
2. Same for corelink-audit-chain and corelink-byok
3. Each crate's kill-rate ≥ 80% (DEBT-008 floor) OR documented escalation
4. SEAL audit reports: kill-rate per crate, surviving-mutant list
   classified (real-gap | equivalent | timeout), severity + follow-up WP
   recommendations
5. Single commit on worktree

**ACCEPTANCE GATES:**
```bash
which cargo-mutants || cargo install cargo-mutants --locked
cargo mutants --version
# Per-crate (each may take 20-60 min):
cargo mutants -p corelink-hash --timeout 120 -j2 2>&1 | tail -20
cargo mutants -p corelink-audit-chain --timeout 120 -j2 2>&1 | tail -20
cargo mutants -p corelink-byok --timeout 120 -j2 2>&1 | tail -20
```

---

### WP-6.1 — Grafana SLO dashboards (real metric names)

**CONTEXT:** ROADMAP §6 + Phase 1 observability. 5 existing dashboards
at `dashboards/grafana/DASH-*.json`. This WP adds 2 SLO dashboards
grounded in actual emitted metrics.

**SCOPE:**
- `dashboards/grafana/DASH-SLO-API.json` (NEW)
- `dashboards/grafana/DASH-SLO-AUDIT.json` (NEW)
- Existing DASH-*.json (additive panel updates only for any metric
  truly missed)
- `dashboards/README.md` (regen + import instructions update)
- NEW: `specs/_audits/2026-05-27-grafana-slo-dashboards-seal.md`

**INPUTS (verified by deep-prep §6):**

Existing dashboard shape (DASH-EXEC.json template):
- `schemaVersion: 39`
- Datasource: templated `$DS_PROMETHEUS` variable
- Variables: `tenant_tier` (custom multi-select), `tenant` (label_values),
  `region` (label_values)
- Panel types: timeseries, stat, table
- Refresh: 30s; Time range: now-6h to now

**Real metric names** (15 verified — inventory §6 lines 533-555):
Read inventory §6 for the full list with file references. Key metrics
for SLO dashboards:

API SLO panels:
- `corelink_request_duration_seconds` (histogram, by route + method + status)
- `corelink_request_total` (counter, by route + method + status)
- `corelink_cas_get_duration_seconds`, `corelink_cas_put_duration_seconds`
- `corelink_ac_get_duration_seconds`, `corelink_ac_put_duration_seconds`
- Cost-signal: `corelink_cf_cpu_time_us`, `corelink_r2_ops_total`,
  `corelink_d1_row_scans_total`

Audit SLO panels:
- `corelink_audit_emit_duration_seconds` (histogram)
- `corelink_audit_chain_size_bytes` (gauge — chain growth rate)
- `corelink_audit_verify_duration_seconds` (re-derivation cost)

NOTE per inventory §6 Q1: metric emission is via
`corelink_analytics::RedMetricKind` enum, NOT `metrics::counter!` macro
direct. Confirm OTLP export wiring at
`crates/corelink-telemetry/src/otel/exporter.rs` before dashboard panel
queries will return data.

R15 resolution: dashboards are PER-TENANT (use existing `tenant`
variable from template).

**DOD:**
1. Both new dashboards valid JSON (`jq . <file> > /dev/null`)
2. Both dashboards have ≥6 panels each
3. All panel queries use metric names from inventory §6 list (no
   invented metrics)
4. README updated with import instructions for both
5. SEAL audit cross-references each panel to the emitter file
6. Single commit on worktree

**ACCEPTANCE GATES:**
```bash
jq . dashboards/grafana/DASH-SLO-API.json > /dev/null
jq . dashboards/grafana/DASH-SLO-AUDIT.json > /dev/null
# Each panel's expr must reference a real metric name:
jq '.panels[].targets[]?.expr // empty' dashboards/grafana/DASH-SLO-API.json | \
  grep -oE "corelink_[a-z_]+" | sort -u
# Cross-check against real metrics (find in source):
grep -rEhn "fn .*Metric.*name|RedMetricKind::[A-Z]" crates/corelink-telemetry/src/ | head -20
```

---

### WP-6.2 — Python SDK MVP (3 ops from OpenAPI + mypy gate)

**CONTEXT:** ROADMAP §6 P1-target use case (Python notebooks).

**SCOPE:**
- `sdks/python/` (NEW dir)
- `sdks/python/pyproject.toml`, README, LICENSE (Apache-2.0)
- `sdks/python/corelink/{__init__.py,client.py,exceptions.py,types.py}`
- `sdks/python/tests/test_client.py`
- `sdks/python/Makefile` (lint/typecheck/test convenience)
- NEW: `specs/_audits/2026-05-27-python-sdk-mvp-seal.md`

**INPUTS (verified by deep-prep §7):**

OpenAPI source: `apps/docs/static/openapi-corelink-v1.yaml`
- Read full file at start (inventory §7 lists all ops)
- Auth scheme: Bearer token (verify in OpenAPI `components.securitySchemes`)
- Content types: JSON + binary (cas-put with octet-stream)

3 ops for MVP (pick from inventory §7 list — recommended):
1. `getHealth` — GET /health (no auth, simplest)
2. `casGetBlob` — GET /v1/cas/{key} (auth, returns binary)
3. `casPutBlob` — PUT /v1/cas/{key} (auth, accepts binary)

> If those operationIds don't match what's actually in the OpenAPI yaml,
> use the closest 3 ops following: 1 healthcheck-style, 1 read, 1 write.
> Cite the exact operationIds chosen in SEAL.

Python environment (per inventory §7):
- Python on machine: 3.14.3 (`python3 --version`)
- **R16**: package floor `>=3.10` (broad reach; httpx + pydantic both fine)
- mypy NOT installed → add to `[project.optional-dependencies.dev]` as
  per R4
- `which ruff pytest` — read output during execution (likely present
  given the broader monorepo); add if missing

Toolchain:
- httpx (sync) for HTTP
- pydantic v2 for response models
- pytest + httpx-mock for tests

R17: package name `corelink` on import; PyPI metadata `name =
"corelink-py"` (no actual PyPI upload during MVP).

**DOD:**
1. `cd sdks/python && pip install -e ".[dev]"` exits 0
2. `cd sdks/python && python -m pytest -v` exits 0 (≥5 tests)
3. `cd sdks/python && python -m mypy corelink/` exits 0
4. `cd sdks/python && python -m ruff check corelink/` exits 0
5. README has quickstart + install + usage + license
6. License Apache-2.0
7. SEAL audit committed
8. Single commit on worktree

**ACCEPTANCE GATES:**
```bash
cd sdks/python
python3 -m pip install -e ".[dev]" 2>&1 | tail -3
python3 -m pytest -v 2>&1 | tail -15
python3 -m mypy corelink/ 2>&1 | tail -3
python3 -m ruff check corelink/ 2>&1 | tail -3
```

**COMMIT MSG TEMPLATE:**
```
feat(sdks): Python SDK MVP (3 ops from openapi-corelink-v1.yaml)

In-monorepo at sdks/python/. Package metadata: corelink-py (no PyPI
upload during MVP per R17); import as `corelink`.

MVP ops: getHealth, casGetBlob, casPutBlob (or closest 3 — see SEAL).
Stack: httpx (sync) + pydantic v2. Python >=3.10 floor.

Gates: ruff + mypy + pytest all exit 0.

Audit: specs/_audits/2026-05-27-python-sdk-mvp-seal.md.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

### WP-7.1 — BetterStack synthetic probes (5 flat-name URLs)

**CONTEXT:** ROADMAP §6 + Phase A SEAL. Probes external uptime via
BetterStack from US/EU/AP regions.

**SCOPE:**
- `monitoring/synthetic/probes.yml` (NEW dir + file)
- `monitoring/synthetic/README.md`
- `scripts/apply-betterstack-probes.sh`
- NEW: `specs/_audits/2026-05-27-synthetic-monitoring-seal.md`

**INPUTS (verified by deep-prep §8):**

5 probes (R18+R27 resolutions, flat-name pattern):

| # | URL | Method | Interval | Status | Body assertion |
|---|---|---|---|---|---|
| 1 | `https://corelink-api.humangr.com/health` (Worker) | GET | 30s | 200 | `{"ok":true}` JSON |
| 2 | `https://corelink-api.humangr.com/api/health` (server-layer per R18) | GET | 30s | 200 | `"version"` substring |
| 3 | `https://corelink-app.humangr.com` | GET | 60s | 200 | `<title>` HTML present |
| 4 | `https://corelink-docs.humangr.com` | GET | 60s | 200 | `Docusaurus` substring |
| 5 | `https://corelink-signup.humangr.com` | GET | 60s | 200 | redirect-to-Clerk OK |

Plus monitors for `corelink-admin.humangr.com` + `corelink-get.humangr.com`
(R27): each gets own monitor.

BetterStack:
- Page ID: read from `specs/_audits/sealed/2026-05-22-w32-phaseA-
  betterstack-live.md` (R19)
- API endpoint base: `https://betteruptime.com/api/v2`
- Token in `wrangler secret list` (verify presence without
  extracting value)
- Probe-creation API shape: POST `/monitors` with JSON body fields:
  `url`, `monitor_type` ("status"), `pronounceable_name`, `check_frequency`
  (sec), `request_timeout` (sec), `regions` (array), `expected_status_codes`
  (array), `required_keyword` (string for body assertion)
- Vendor docs: https://betterstack.com/docs/uptime/api/create-a-new-monitor/

**DOD:**
1. `probes.yml` valid YAML (`python3 -c "import yaml; yaml.safe_load(open('monitoring/synthetic/probes.yml'))"` exits 0)
2. Apply script `--dry-run` lists all probes + would-be API calls; NO
   real calls
3. `shellcheck scripts/apply-betterstack-probes.sh` clean
4. README documents create/update/delete via script
5. SEAL audit committed
6. Single commit on worktree

**ACCEPTANCE GATES:**
```bash
python3 -c "import yaml; yaml.safe_load(open('monitoring/synthetic/probes.yml'))"
shellcheck scripts/apply-betterstack-probes.sh
bash scripts/apply-betterstack-probes.sh --dry-run 2>&1 | head -30
```

---

### WP-7.2 — SBOM CycloneDX + license audit (Phase 1 deps)

**CONTEXT:** R20: CycloneDX standard. Phase 1 added @sentry/*, @clerk/*,
resend, BetterStack badge fetch (no client SDK), etc. Refresh SBOM +
license audit.

**SCOPE:**
- `.sbom/` (NEW dir if absent; CycloneDX JSON outputs)
- `scripts/generate-sbom.sh` (harden / author)
- `specs/_compliance/license-allowlist.md` (extend if needed)
- NEW: `specs/_audits/2026-05-27-sbom-license-audit-seal.md`

**INPUTS (verified by deep-prep §9):**

Current tooling state:
- `cargo-deny --version` — already wired per Wave 36 stage 3
- `cargo-cyclonedx` — read `which cargo-cyclonedx` during execution
- `cdxgen` (universal cyclonedx CLI for npm) — read `which cdxgen`;
  install via `npm install -g @cyclonedx/cdxgen` if needed
- Existing `.sbom/` or `sbom/`: inventory §9 says directory does NOT
  exist (BLOCKER-5 medium); WP creates it

License allowlist (per Wave 36):
- Source: `cargo-deny.toml` `[licenses]` section
- Allowlist: Apache-2.0, MIT, BSD-2-Clause, BSD-3-Clause, ISC, MPL-2.0,
  CC0-1.0
- R21: Sentry SDK FSSA license — document in audit doc that server-side
  use only does NOT trigger commercial restriction; cite Sentry license
  doc URL.

Phase 1 deps to verify:
- `apps/admin-ui/package.json` and `apps/docs/package.json` — diff
  against pre-Phase-1 (use git):
  ```bash
  git log --since=2026-05-25 -p apps/admin-ui/package.json apps/docs/package.json | \
    grep '^+' | grep '"' | head
  ```

**DOD:**
1. `.sbom/cyclonedx-rust.json` exists + valid JSON
2. `.sbom/cyclonedx-npm-admin-ui.json` + `cyclonedx-npm-docs.json` exist
3. Each JSON has `bomFormat: "CycloneDX"` and `specVersion: "1.5"`
4. License audit table in SEAL doc: total deps + allowed + flagged
5. R21 Sentry FSSA paragraph in SEAL with vendor cite
6. `bash scripts/generate-sbom.sh` re-runs idempotently
7. Single commit on worktree

**ACCEPTANCE GATES:**
```bash
which cargo-cyclonedx cdxgen
bash scripts/generate-sbom.sh 2>&1 | tail -5
jq . .sbom/cyclonedx-rust.json | head -5
jq . .sbom/cyclonedx-npm-admin-ui.json | head -5
jq . .sbom/cyclonedx-npm-docs.json | head -5
```

---

### WP-7.3 — Quickstart docs refresh aligned to CLI MVP + `corelink doctor`

**CONTEXT:** ROADMAP §4 TTFV ≤10min. Phase 1 CLI MVP at `71d7f6a9`
(ping + bazel-init). WP-2.1 adds `doctor` (R1). Quickstart steps
realigned to actual UX.

**SCOPE:**
- Canonical quickstart at the path inventory §10 confirms (likely
  `apps/docs/docs/tutorial/quickstart.mdx` or similar — locate first
  via `find apps/docs/docs -iname "*quickstart*" -o -iname "*getting-started*"`)
- 4 locale variants at `apps/docs/i18n/<locale>/.../quickstart*.mdx`
- Sidebar wiring (`apps/docs/sidebars.ts` if changes needed)
- NEW: `specs/_audits/2026-05-27-quickstart-refresh-seal.md`

**INPUTS (verified by deep-prep §10):**

Find canonical: `find apps/docs/docs apps/docs/i18n -iname "*quickstart*"
-o -iname "*getting-started*" 2>/dev/null | head -10`

Confirmed gaps in current quickstart (inventory §10):
- References `corelink doctor` (HIGH-3 blocker; FIXED by WP-2.1
  delivering doctor command per R1)
- Stale `get.corelink.io` reference (must be `corelink-get.humangr.com`)

New step sequence (R22):
1. Prerequisites — Bazel 7+, browser
2. Sign up at https://corelink-app.humangr.com
3. Copy PAT from `/welcome` (point at actual screenshot if available)
4. `curl -fsSL https://corelink-get.humangr.com | sh -s -- --token=ct_xxx
   --region=ord` (flat-name verified)
5. `corelink doctor` — runs 9-point checklist from WP-2.1 (R22)
6. cd into Bazel repo + `corelink bazel-init`
7. `bazel build //... && bazel build //...` — second build mostly HITs
8. Troubleshooting (3 common failures: region mismatch, token-not-set,
   .bazelrc duplicate)
9. Next steps — bazel-example repo, blog #1, comparison pages

R23: all 4 locales updated in parallel.

**DOD:**
1. `cd apps/docs && pnpm build` exit 0 (all 4 locales)
2. Canonical quickstart has all 9 steps + troubleshooting
3. All 4 locales updated (translated OR canonical-EN-link stub)
4. Zero stale references (`grep -rn "get\.corelink\.io\|corelink\.dev"
   apps/docs/docs/ apps/docs/i18n/` returns empty for quickstart paths)
5. SEAL audit committed
6. Single commit on worktree

**ACCEPTANCE GATES:**
```bash
find apps/docs/docs apps/docs/i18n -iname "*quickstart*" -o -iname "*getting-started*"
cd apps/docs && pnpm build 2>&1 | tail -5
grep -rn "corelink doctor\|corelink-get\.humangr\.com" docs/ i18n/ | head
grep -rn "get\.corelink\.io\|corelink\.dev" docs/tutorial/ i18n/*/docusaurus-plugin-content-docs/current/tutorial/ 2>/dev/null
# (Last grep should be empty — zero stale references)
```

> **Sequencing note:** WP-7.3 depends on WP-2.1 delivering `corelink
> doctor`. Dispatch order at §3: WP-7.3 can be dispatched in parallel
> with WP-2.1; if WP-2.1's `doctor` ships first, WP-7.3 references it
> as a real command. If not, WP-7.3 documents the planned UX with a
> note that doctor lands in WP-2.1's commit. Either way, the quickstart
> doc is correct on the docs site at merge-into-main time.

---

## §3 Dispatch order

All 15 are dispatched in a **single parallel batch** with
`isolation: "worktree"`, `model: "sonnet"`, `run_in_background: true`.

Per the conflict matrix in §1, no two agents touch the same file
zone. The 2 hot zones (`tags.yml`, SEAL audit dir) are protected by
append-only + unique-naming respectively.

---

## §4 Merge order (when SEALs arrive)

When agents complete (unpredictable order), orchestrator merges **by
arrival time**. Each merge:
1. L0-L7 tech-lead verdict (per `techlead` skill)
2. Resolve conflicts via §1 matrix guidance
3. Push every 3-5 merges (L10)
4. Update `specs/_audits/2026-05-27-INDEX.md` after each merge

---

## §5 Hard pause triggers

Stop dispatching / merging if:
- ≥3 agents return identical blocker → systemic, investigate
- WP-1.1 flags a Wave 32 §7 hard pause trigger (CF Containers blocker)
- `validate_specs.py` regresses below 0 (NEVER allow)
- Disk free < 5 GB (clean target/ first)
- Any agent reports cwd-leak from worktree (per `feedback_agent_worktree_isolation`)

---

## §6 Open questions (all 27 resolved per §0.8)

All 27 open questions from `2026-05-27-deep-prep-inputs.md` §11 are
resolved in §0.8 (4 user + 23 orchestrator). No question carries forward
to dispatch time.

---

## §7 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of v2.0 dispatch matrix — ready for go.**
