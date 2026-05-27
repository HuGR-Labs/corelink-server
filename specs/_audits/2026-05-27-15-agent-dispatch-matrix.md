---
id: "DISPATCH-MATRIX-2026-05-27"
type: "audit"
doc_status: "ACTIVE"
audit_status: "PENDING"
version: "1.0.0"
created: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
tags: ["dispatch", "engineering", "wave32", "parallel", "sota", "15-agents"]
references:
  - "specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md"
  - "ROADMAP-TO-LAUNCH.md (Phase 4)"
  - "specs/_audits/2026-05-27-INDEX.md"
---

# 15-agent dispatch matrix — engineering refocus (2026-05-27)

> **Mandate (2026-05-27 verbatim):** *"Quero wps sota separados por contextos,
> feitos ja pensando em paralelizacao agressiva, organize todos os wps para
> serem divididos entre 15 agents sonnet. Briefings sota, rigor maximo.
> Prompts igualmente sota. Tudo organizado, mastigado, objetivo, pronto pra
> executar sem erros, e com quality standards sota."*
>
> **Plus (2026-05-27):** *"cada agent em uma worktree isolada dos demais."*

## §0 Master constraints (apply to EVERY agent)

These are non-negotiable and inherited by every WP contract below.

### 0.1 Pre-flight (mandatory first action)
```bash
pwd                                  # must end with agent-<task-id>
test -d ".git" || test -f ".git"     # worktree marker present
cat .git 2>/dev/null | grep gitdir   # confirm it's a worktree link, not main
```
If `pwd` ends with `corelink-server` WITHOUT the worktree segment, STOP
immediately. Do not edit anything. Report a worktree leak.

### 0.2 Charter constraints (Rust where applicable)
- `#![forbid(unsafe_code)]` at every new crate root
- `#[non_exhaustive]` on every new public enum + struct
- No `unwrap()` / `expect()` / `panic!` / `todo!()` / `unimplemented!()`
  outside `#[cfg(test)]`
- No `tokio` import in `src/` (only `#[cfg(test)]`)
- No `prop_assert!(matches!(..., Variant { .. }))` anti-pattern
- Audit emit ordering: `lookup → emit_audit → mutate_state` always
- Secrets never logged (grep your code for `tracing::|log::|println!|dbg!`
  and verify any secret-adjacent line redacts)

### 0.3 TypeScript/Next charter (where applicable)
- No `// @ts-ignore`, no `as any`, no `as unknown as`
- No `--no-verify`, no `eslint-disable`
- No `console.log` of any `process.env` value

### 0.4 GHA charter (where applicable)
- Every `uses:` SHA-pinned to 40-char commit hash (FF-HR-005)
- No floating tags (`@v4`, `@main`, `@latest`, `@stable`)

### 0.5 Test bar
- `proptest!` configured via `proptest_cases(N)` helper (NOT
  `ProptestConfig::with_cases(N)` directly)
- Adversarial test names prefixed `adversarial_*` / `negative_*` /
  `replay_*` / `tampered_*` / `prop_assert_*`
- Real-network / live-API tests gated behind `#[ignore]` AND a
  `*_TEST_KEY` env var

### 0.6 Commit + push policy
- Agent commits its own work BEFORE reporting SEAL (orchestrator policy)
- Commit message includes a `Co-Authored-By:` trailer
- DO NOT use `--no-verify`
- DO NOT use `git push --force` to any branch except your worktree branch
- DO NOT touch `main` directly

### 0.7 SEAL report contract (every agent returns this shape)
```
result: <one-line summary>
DoD: 1/✅ 2/✅ ... N/✅  (or ❌ with reason)
files changed: <absolute paths>
commit SHA: <full SHA>
external-repo changes: <if any, repo + SHA>
blockers: <list or NONE>
```

---

## §1 Contexts × WPs (15 total)

```
CONTEXT 1 — Wave 32 prod-deploy advance (2 agents)
  WP-1.1  Phase B  worker-shim audit + harden + adversarial review
  WP-1.2  Phase C  idempotent CF provisioning script + dry-run

CONTEXT 2 — CLI feature expansion (1 agent)
  WP-2.1  corelink-cli: cargo-init + npm-init + docker-init commands

CONTEXT 3 — Comparison pages remaining (3 agents)
  WP-3.1  vs bazel-remote+S3        (1500-2500 words)
  WP-3.2  vs sccache+S3              (1500-2500 words)
  WP-3.3  vs Turborepo Remote Cache  (1500-2500 words)

CONTEXT 4 — Educational content cadence (2 agents)
  WP-4.1  Blog #2 — BLAKE3 internals + why we picked it
  WP-4.2  Blog #3 — Audit-chain re-derivation walkthrough

CONTEXT 5 — Engineering quality (2 agents)
  WP-5.1  validate_specs.py baseline-to-zero sweep
  WP-5.2  cargo-mutants kill-rate baseline on core crates

CONTEXT 6 — Observability + SDK (2 agents)
  WP-6.1  Grafana dashboards JSON for prod SLOs
  WP-6.2  Python SDK MVP from OpenAPI (sdks/python/)

CONTEXT 7 — Production-readiness hardening (3 agents)
  WP-7.1  Synthetic monitoring probes (BetterStack scripted)
  WP-7.2  SBOM + license audit refresh post-Phase-1
  WP-7.3  Quickstart docs refresh aligned to corelink-cli MVP
```

### Conflict matrix

```
                       worker/  scripts/  apps/docs/ apps/docs/  specs/  crates/  sdks/  dashboards/ marketing/ external-repo
                                          compare    blog                                            
WP-1.1  worker shim    █                                          ░                                                          
WP-1.2  CF provision           █                                                                                              
WP-2.1  CLI commands                                                                                              corelink-cli
WP-3.1  vs bazel-rem                       ░vs-bazel..              .         .                                                
WP-3.2  vs sccache                         ░vs-sccache              .         .                                                
WP-3.3  vs Turborepo                       ░vs-turbo                .         .                                                
WP-4.1  Blog BLAKE3                                  ░2026-..       .                                                          
WP-4.2  Blog audit                                   ░2026-..       .                                                          
WP-5.1  validate_specs                                              ▓                                                          
WP-5.2  cargo-mutants                                                       ░                                                  
WP-6.1  Grafana JSON                                                                              ▓                            
WP-6.2  Python SDK                                                                       ░                                     
WP-7.1  Synthetic mon          .                                                                                              
WP-7.2  SBOM/license                                                                                                          
WP-7.3  Quickstart                                                ░ tutorial                                                  

█ = primary edit zone   ░ = WP-scoped subdir   ▓ = broad-scoped (read-mostly, narrow writes)
```

**Hot zones (multi-WP) and how they're safe:**

- `apps/docs/src/pages/compare/` — WP-3.1/3.2/3.3 each ONLY touch
  `vs-<their-name>.mdx`. No shared file.
- `apps/docs/blog/` — WP-4.1/4.2 each touch their dated MDX file.
  Both MIGHT add tags to `tags.yml` — guidance: each agent appends
  ONLY new tag rows, never edits existing rows. Orchestrator unions
  cleanly during merge.
- `specs/` — WP-5.1 has broad scope BUT only updates audit
  doc-status / frontmatter. Does not edit any code spec content.
- `worker/` — only WP-1.1 owns. WP-1.2 only touches `wrangler.toml` and
  `scripts/`.

---

## §2 Per-WP contracts

Each contract is self-contained — an agent dispatched with that contract
in its prompt MUST be able to execute without any external reference
(per `feedback_prompts_mastigados` memory).

---

### WP-1.1 — Phase B worker-shim audit + harden + adversarial review

**CONTEXT:** Wave 32 Phase B per spec at
`specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md` §4 Phase B.
Worker shim partially landed (~2755 LOC in `worker/src/`, `worker/tests/`)
during the Phase 0 rush. We need an adversarial review to seal it.

**ROLE:** Audit `worker/` against the Wave 32 Phase B spec; harden any
gaps; raise coverage to ≥70% on shim code; produce SEAL audit doc with
adversarial-review verdict.

**SCOPE (may modify):**
- `worker/src/{index.ts,durable_object.ts,rollout_controller.ts}`
- `worker/tests/*.ts`
- `worker/tsconfig.json`, `worker/package.json`, `worker/vitest.config.mts`
- `wrangler.toml` `main = "worker/src/index.ts"` line only (uncomment if
  still commented; do NOT touch other config)
- `specs/_audits/2026-05-27-w32-phaseB-worker-shim-seal.md` (NEW)

**NON-SCOPE:**
- `apps/` (any subdir)
- `crates/` (any subdir)
- Real CF infra (Phase C territory)
- Wrangler secret rotation

**INPUTS (pre-computed):**
- Current worker/ inventory: `index.ts` + `durable_object.ts` +
  `rollout_controller.ts` exist; tests at 947 LOC across 3 files.
- Spec §Phase B gates (verbatim):
  1. `cd worker && pnpm test` ≥70% coverage; all green
  2. `wrangler dev --local` boots; `curl http://localhost:8787/health` → 200
  3. `cargo build -p corelink-server` builds clean in release profile
  4. `validate_specs.py` + `validate_references.py` green
- Per spec §7 hard pause trigger: if CF Containers beta surfaces a
  blocker (DO ↔ container IPC, billing, Rust binary fails), HALT.

**DOD:**
1. `cd worker && pnpm test --coverage` shows ≥70% line coverage on `src/`
2. `cd worker && pnpm test` exits 0 (every test green)
3. `cd worker && pnpm exec tsc --noEmit` exits 0
4. `wrangler dev --local --port 8787` boots; `curl localhost:8787/health` → 200
5. `python3 scripts/validate_specs.py` exits 0
6. SEAL audit doc lists: file inventory, test count, coverage %, gaps
   found + fixed, residual risks (≥3 entries with severity)
7. Adversarial-review section: 5+ "what if" attacks attempted (replay,
   tenant boundary, malformed gRPC frame, DO storage corruption, race
   on container start); each closed with code reference or accepted
   risk
8. Single commit on worktree

**CONSTRAINTS:** §0.1, §0.3, §0.5, §0.6 from master.

**ACCEPTANCE GATES:**
```bash
cd worker
pnpm install --frozen-lockfile=false
pnpm test --coverage 2>&1 | tail -15
pnpm exec tsc --noEmit 2>&1 | tail -3
# wrangler dev cannot run in agent env (no CF login); skip with note in SEAL
cd ..
python3 scripts/validate_specs.py 2>&1 | tail -3
```

**COMMIT MSG TEMPLATE:**
```
seal(w32-phaseB): worker-shim audit + harden + adversarial review

Wave 32 Phase B per spec sealed/2026-05-22-wave32-prod-deploy-spec.md §B.

Coverage: <N>% on worker/src/ (target ≥70).
Adversarial review: 5/5 attacks closed (replay, tenant boundary,
malformed gRPC frame, DO storage corruption, container-start race).
Gaps fixed: <count, summary in audit doc>.

Audit doc: specs/_audits/2026-05-27-w32-phaseB-worker-shim-seal.md.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

### WP-1.2 — Phase C idempotent CF provisioning script + dry-run

**CONTEXT:** Wave 32 Phase C — provision real CF resources (1 D1, 5 KV,
6 R2) and replace `wrangler.toml` PLACEHOLDER strings with real IDs.
`scripts/provision-cf-corelink-prod.sh` already exists as scaffolding;
harden it to idempotent / re-runnable.

**ROLE:** Take the provisioning script to production-ready state with
DRY-RUN mode + idempotent name-match-then-PATCH semantics + a teardown
counterpart. Do NOT execute against real CF account (Owner-only).

**SCOPE:**
- `scripts/provision-cf-corelink-prod.sh` (harden)
- `scripts/teardown-cf-corelink-prod.sh` (verify exists / harden)
- `scripts/lib/cf-api.sh` (extract common API helpers if useful)
- `specs/_audits/2026-05-27-w32-phaseC-provisioning-seal.md` (NEW)

**NON-SCOPE:**
- Any real `curl` to api.cloudflare.com (script must support `--dry-run`
  default; live mode is Owner-only)
- `wrangler.toml` placeholder substitution (deferred to D-day; script
  PREPARES the substitution but doesn't apply it)

**INPUTS (pre-computed):**
- Resources to provision (per Wave 32 §Phase C):
  - 1× D1: `corelink-prod-d1`
  - 5× KV: `corelink-prod-jwks-kv`, `corelink-prod-cache-kv`,
    `corelink-prod-rate-limit-kv`, `corelink-prod-session-kv`,
    `corelink-prod-pilot-signup-kv`
  - 6× R2: `corelink-cas-prod`, `corelink-ac-sam`, `corelink-ac-iad`,
    `corelink-ac-lhr`, `corelink-ac-nrt`, `corelink-ac-syd`
- Existing CF API token must scope: `Account:D1:Edit`, `Account:KV:Edit`,
  `Account:R2:Edit`. Script verifies via `wrangler whoami` first.
- Reference idempotent pattern: existing `scripts/statuspage-bootstrap.sh`
  (Phase A) uses name-match-then-PATCH; mirror that.

**DOD:**
1. `bash scripts/provision-cf-corelink-prod.sh --dry-run` exits 0, prints
   the 12 resources it WOULD provision, no API calls made
2. `bash scripts/provision-cf-corelink-prod.sh --dry-run --validate-token`
   exits 0 IF a token is present in env, OR exits 1 with helpful error IF
   token absent
3. Script handles re-run idempotency: a resource that already exists is
   recognized by name, IDs are echoed (not re-created)
4. Teardown script symmetric: `--dry-run` prints what it WOULD delete
5. Both scripts pass `shellcheck` clean
6. Audit doc documents: token-scope checklist, dry-run output capture,
   D-day execution checklist, rollback ladder
7. Single commit on worktree

**CONSTRAINTS:** §0.1, §0.4 if any .yml touched, §0.6.

**ACCEPTANCE GATES:**
```bash
shellcheck scripts/provision-cf-corelink-prod.sh scripts/teardown-cf-corelink-prod.sh 2>&1 | tail
bash scripts/provision-cf-corelink-prod.sh --dry-run 2>&1 | head -30
bash scripts/teardown-cf-corelink-prod.sh --dry-run 2>&1 | head -30
```

**COMMIT MSG TEMPLATE:**
```
seal(w32-phaseC): idempotent CF provisioning script + dry-run

Wave 32 Phase C provisioning hardened to dry-run-default with
idempotent name-match-then-PATCH semantics. Token-scope precheck
added; teardown counterpart symmetric.

12 resources catalogued (1 D1 + 5 KV + 6 R2). Live execution remains
Owner-only.

Audit doc: specs/_audits/2026-05-27-w32-phaseC-provisioning-seal.md.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

### WP-2.1 — corelink-cli: cargo-init + npm-init + docker-init commands

**CONTEXT:** External repo `humangr-labs/corelink-cli @ 71d7f6a9` ships
MVP with `ping` + `bazel-init`. Phase 1.3 of ROADMAP names
`buck2-init` / `cargo-init` as stubs. Expand to cargo + npm + docker
(OCI) wedges to cover the polyglot positioning per competitive audit §2.

**ROLE:** Add 3 commands to the CLI: `cargo-init`, `npm-init`,
`docker-init`. Each detects its build-system marker file, appends 3-5
idempotent lines of cache configuration, and confirms with a sample
run instruction.

**SCOPE (external repo only):**
- Clone `humangr-labs/corelink-cli` to `/tmp/corelink-cli-wp21-<task-id>/`
- Add `src/commands/cargo_init.rs`
- Add `src/commands/npm_init.rs`
- Add `src/commands/docker_init.rs`
- Update `src/main.rs` clap registration (3 new subcommands)
- Update `src/commands/mod.rs` exports
- Add tests in `tests/`
- Update `README.md` usage section
- Push to `main` of external repo
- In MONOREPO worktree: SEAL audit at
  `specs/_audits/2026-05-27-cli-polyglot-commands-seal.md`

**NON-SCOPE:**
- Any monorepo source change beyond the SEAL audit
- Real network calls in tests (use tempdir fixtures)

**INPUTS (pre-computed, per build-system):**

| Cmd | Detect file | Append target | Lines to write |
|---|---|---|---|
| cargo-init | `Cargo.toml` (project root) | `.cargo/config.toml` | `[build]` `incremental = false` (cache requires) ; `[remote-cache]` `endpoint = "<arg>"` ; `[remote-cache]` `token = "<arg>"` |
| npm-init | `package.json` | `.npmrc` + `package.json` "scripts" section | `cache=~/.npm/corelink-cache` ; pre-add a `corelink:warm` script that primes the cache |
| docker-init | `Dockerfile` | new `.docker/buildx-cache.json` + `.gitignore` line | `{"endpoint": "<arg>", "token": "<arg>", "layer-cache": true}` and BuildKit `--cache-from=type=remote` flag |

Each command is idempotent: detecting an already-applied marker line
results in a noisy log + exit 0, NO file modification.

**DOD:**
1. `cargo check` exits 0
2. `cargo build --release` exits 0
3. `cargo test` exits 0 (≥3 tests per command: detect-success,
   detect-miss, already-applied-noop)
4. `./target/release/corelink cargo-init --help` shows the new command
5. `./target/release/corelink npm-init --help` shows the new command
6. `./target/release/corelink docker-init --help` shows the new command
7. README usage section updated with examples
8. All GHA `uses:` SHA-pinned (verify with grep)
9. Pushed to external repo `main` branch
10. SEAL audit committed in monorepo worktree

**CONSTRAINTS:** §0.1, §0.2, §0.5, §0.6.

**ACCEPTANCE GATES (run in cloned cli dir):**
```bash
cd /tmp/corelink-cli-wp21-<task-id>
cargo check 2>&1 | tail -3
cargo build --release 2>&1 | tail -3
cargo test 2>&1 | tail -10
./target/release/corelink --help | grep -E "cargo-init|npm-init|docker-init"
git log -1 --oneline
git push origin main 2>&1 | tail -3
```

**COMMIT MSG TEMPLATE (monorepo worktree, audit doc):**
```
docs(specs): SEAL audit for corelink-cli polyglot commands

ROADMAP Phase 1.3 expansion: cargo-init + npm-init + docker-init
added to corelink-cli. Each command detects its build-system marker,
appends idempotent cache config (Cargo.toml, .npmrc, .docker/),
exit-0 if already applied.

External repo: humangr-labs/corelink-cli @ <new-SHA>.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

### WP-3.1 — Comparison page: CoreLink vs bazel-remote + S3

**CONTEXT:** Per ROADMAP §6 Phase 4.6, 6 comparison pages are scheduled
(3 done: vs-buildbuddy, vs-engflow, vs-nx-cloud). This WP delivers the
4th: vs `bazel-remote` (the open-source Bazel REAPI cache) running on
self-hosted S3.

**ROLE:** Write a single MDX page at
`apps/docs/src/pages/compare/vs-bazel-remote-s3.mdx`. 1500-2500 words.
Honest. Cites the bazel-remote GitHub project + S3 cost calculators.
Linked from existing comparison index.

**SCOPE:**
- `apps/docs/src/pages/compare/vs-bazel-remote-s3.mdx` (NEW)
- `apps/docs/src/pages/compare/index.mdx` (link to new page; check if
  index file exists first)
- `apps/docs/i18n/pt-BR/.../vs-bazel-remote-s3.mdx` (translated stub OK)
- `apps/docs/i18n/es-419/.../vs-bazel-remote-s3.mdx`
- `apps/docs/i18n/de/.../vs-bazel-remote-s3.mdx`

**NON-SCOPE:**
- Any other comparison page
- Any docs config

**STRUCTURE (mandatory sections, in order):**
1. **TL;DR** — 3 bullets honest summary
2. **What is bazel-remote** — 1 paragraph
3. **Where bazel-remote wins** — at least 3 wins with brief explanation
4. **Where CoreLink wins** — at least 4 wins with charter-cited evidence
   (BYOK, verifiable audit chain, multi-package not just Bazel, no ops
   overhead)
5. **Cost comparison** — concrete numbers: bazel-remote+S3 at three
   scale points (10 GB / 500 GB / 5 TB) vs CoreLink Free/Pro/Enterprise
6. **When to use which** — decision tree (3 questions, end nodes)
7. **Migration path** — step-by-step from bazel-remote to CoreLink
   (idempotent, both running side-by-side)
8. **Sources** — bibliography with verifiable URLs

**TONE:** Honest. Acknowledge bazel-remote is excellent for its niche.
Do not mislead. Cite charter sources for every CoreLink claim.

**DOD:**
1. Word count 1500-2500 (`wc -w` on the rendered text excluding code blocks)
2. `pnpm build` in `apps/docs/` exits 0
3. 4 locales present (translated stubs acceptable for pt-BR/es-419/de
   with "Coming soon — see English version" + canonical link)
4. Page links from comparison index (or footer compare-list if no
   dedicated index)
5. All external URLs return HTTP 200 (verify with curl in your acceptance run)
6. SEAL audit at `specs/_audits/2026-05-27-compare-bazel-remote-s3-seal.md`
7. Single commit on worktree

**CONSTRAINTS:** §0.1, §0.6. Honesty bar from charter.

**ACCEPTANCE GATES:**
```bash
cd apps/docs && pnpm build 2>&1 | tail -5
wc -w src/pages/compare/vs-bazel-remote-s3.mdx
grep -oE "https://[^ )]+" src/pages/compare/vs-bazel-remote-s3.mdx | sort -u | head -20 | \
  while read u; do printf "%-80s " "$u"; curl -sI --max-time 8 "$u" | head -1; done
```

**COMMIT MSG TEMPLATE:**
```
content(compare): CoreLink vs bazel-remote + S3 (Phase 4.6/6)

ROADMAP Phase 4.6 comparison page #4. 1500-2500w honest writeup with
3 bazel-remote wins, 4 CoreLink wins, 3-scale cost comparison, 3-q
decision tree, migration path, full source bibliography.

4 locales (pt-BR/es-419/de stub + canonical EN link until human-review).

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

### WP-3.2 — Comparison page: CoreLink vs sccache + S3

Same contract as WP-3.1 with substitutions:

- Page path: `apps/docs/src/pages/compare/vs-sccache-s3.mdx`
- Subject: `sccache` (Mozilla's compiler-cache, Rust/C++/Swift)
- Wins for sccache: free, simple, well-known
- Wins for CoreLink: multi-language (not just compiler caches), audit
  log, BYOK, no S3 ops, REAPI compatibility (Bazel/Buck2)
- Cost section: sccache+S3 self-host at 3 scales vs CoreLink
- Migration: parallel-run pattern (sccache local fallback + CoreLink
  cloud primary)

Audit doc: `specs/_audits/2026-05-27-compare-sccache-s3-seal.md`.

---

### WP-3.3 — Comparison page: CoreLink vs Turborepo Remote Cache

Same contract as WP-3.1 with substitutions:

- Page path: `apps/docs/src/pages/compare/vs-turborepo.mdx`
- Subject: `Turborepo Remote Cache` (Vercel-owned, JS/TS monorepo cache)
- Wins for Turborepo: free with Vercel, auto-on, npm-native
- Wins for CoreLink: not Vercel-coupled, multi-language, audit log,
  BYOK, residency honesty
- Cost section: Vercel Pro pricing vs CoreLink at equivalent scale
- Migration: drop-in REAPI shim for Turborepo workspaces

Audit doc: `specs/_audits/2026-05-27-compare-turborepo-seal.md`.

> **Note vs ROADMAP:** competitive audit §3.4 explicitly names Turborepo
> as a "lose segment". This page is written for COMPLETENESS but with the
> stance: "if you're already on Vercel, Turborepo wins. If you're not, or
> you need multi-language, here's the case for CoreLink." Honest framing,
> not adversarial.

---

### WP-4.1 — Blog post #2: BLAKE3 internals + why we picked it

**CONTEXT:** Phase 4.7 educational content cadence. Blog #1 (CF Workers
architecture) is live. Blog #2 goes deep on the hash function choice.

**ROLE:** Write 3000-5000 word technical post for the Docusaurus blog.
Topic: BLAKE3 hashing internals + comparison vs SHA-256 + Blake2 +
xxHash for the CAS-cache use case. End with "why CoreLink picked BLAKE3"
section grounded in `crates/corelink-hash/`.

**SCOPE:**
- `apps/docs/blog/2026-05-28-why-blake3.mdx` (NEW)
- `apps/docs/blog/tags.yml` (APPEND ONLY — new tags `cryptography`,
  `blake3`; do NOT edit existing rows)
- `apps/docs/blog/authors.yml` (existing `gustavo` row works — do not
  edit)

**NON-SCOPE:**
- Any other blog post
- Docs config
- Hash-crate source

**STRUCTURE (mandatory):**
1. **Hook** — concrete CAS-cache problem statement
2. **What is a content-addressable hash** — 200 words, accessible
3. **The candidates** — SHA-256, BLAKE2, BLAKE3, xxHash table
4. **BLAKE3 internals** — Merkle tree structure, SIMD path, derive-key
   mode (with diagrams — ASCII art OK, no external images needed)
5. **Benchmarks** — real numbers cited from BLAKE3 paper + reproducible
   command (mention `cargo bench` in `corelink-hash`)
6. **Why CoreLink picked BLAKE3** — 4-5 bullets grounded in CAS use
   case (parallel hashing of large blobs, derive-key for tenant
   namespacing, no length-extension concerns)
7. **What we'd do differently** — at least 2 honest "if we started over"
   thoughts
8. **References** — BLAKE3 paper + repo + relevant crate file paths
   (`crates/corelink-hash/src/digest.rs:42` style)

**DOD:**
1. Word count 3000-5000 (`wc -w`)
2. `cd apps/docs && pnpm build` exits 0 (blog renders)
3. Post HTML present in `apps/docs/build/blog/why-blake3/index.html` (or
   equivalent locale paths)
4. Tags `cryptography` + `blake3` added to `tags.yml` (no existing edits)
5. SEAL audit at `specs/_audits/2026-05-27-blog-2-blake3-seal.md`
6. Single commit on worktree

**CONSTRAINTS:** §0.1, §0.6. Honesty bar (no inflated claims).

**ACCEPTANCE GATES:**
```bash
wc -w apps/docs/blog/2026-05-28-why-blake3.mdx
cd apps/docs && pnpm build 2>&1 | tail -5
test -f build/blog/why-blake3/index.html || ls build/blog/ | head
```

**COMMIT MSG TEMPLATE:**
```
content(blog): blog #2 — why BLAKE3 (Phase 4.7 cadence)

3000-5000w deep-dive on BLAKE3 internals (Merkle tree, SIMD, derive-key)
vs SHA-256/BLAKE2/xxHash for CAS workloads. Grounded in
crates/corelink-hash/ implementation. Honest "what we'd do differently"
section included.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

### WP-4.2 — Blog post #3: Audit-chain re-derivation walkthrough

**CONTEXT:** Same Phase 4.7 cadence. Blog #3 covers the
RFC-6962-inspired audit chain that lets customers re-derive the audit
log to prove tamper-resistance.

**ROLE:** 3000-5000w technical post walking through the audit chain
end-to-end: emit-side construction, storage shape, re-derivation
procedure, what attacks it resists, what attacks it doesn't.

**SCOPE:**
- `apps/docs/blog/2026-05-29-audit-chain-walkthrough.mdx` (NEW)
- `apps/docs/blog/tags.yml` (APPEND new tags `audit`, `merkle`,
  `rfc-6962`)

**STRUCTURE:**
1. **The compliance problem** — "auditor asked how I prove the cache
   wasn't tampered with" hook (matches ICP audit §3.1)
2. **What is RFC 6962 (Certificate Transparency)** — adapted summary
3. **CoreLink's audit chain design** — diagram (ASCII) + crate refs
   (`crates/corelink-audit-chain/`)
4. **The re-derivation procedure** — step-by-step bash + Rust snippets
   that a customer / auditor can actually run (independent of our SaaS)
5. **What attacks this resists** — tampering, reordering, replay,
   selective deletion
6. **What attacks this does NOT resist** — total-loss, time-of-check
   races, insider operator with both signing keys
7. **Performance + cost** — Merkle proof size + audit-storage overhead
8. **References** — RFC 6962, our INV-AUDIT-* invariants, code paths

**DOD:** Identical to WP-4.1 with audit-chain substitutions.

Audit doc: `specs/_audits/2026-05-27-blog-3-audit-chain-seal.md`.

---

### WP-5.1 — `validate_specs.py` baseline-to-zero sweep

**CONTEXT:** Per memory + INDEX §7, `python3 scripts/validate_specs.py`
must NEVER regress; baseline currently at 0 failures per Wave 35
closure. This WP audits + fixes any new failures that crept in during
Phase 1 churn and raises the validator's strictness one notch if
possible.

**ROLE:** Run validator; for every failure, fix the spec doc (not the
validator); attempt one strictness uplift (e.g., enforce `references:`
exists on every audit doc); commit.

**SCOPE:**
- Any `specs/**/*.md` failing validation (frontmatter / cross-ref fixes
  only — NEVER alter substantive content)
- `scripts/validate_specs.py` ONE strictness uplift (additive, must
  ship green)
- `specs/_audits/2026-05-27-validate-specs-baseline-seal.md` (NEW)

**NON-SCOPE:**
- Any non-`specs/` directory
- Substantive content changes to specs (you fix frontmatter/refs ONLY)

**INPUTS:**
- Baseline as of `36a7a926`: 0 failures (per INDEX §7).
- Common failure shapes: missing `audit_status`, broken `[[name]]` link,
  superseded-by pointing to nonexistent file.

**DOD:**
1. `python3 scripts/validate_specs.py` exits 0
2. `python3 scripts/validate_references.py` exits 0
3. Strictness uplift documented in audit doc (one new rule, justified)
4. SEAL audit committed
5. Single commit on worktree

**CONSTRAINTS:** §0.1, §0.6. No substantive content changes.

**ACCEPTANCE GATES:**
```bash
python3 scripts/validate_specs.py 2>&1 | tail -5
python3 scripts/validate_references.py 2>&1 | tail -5
```

---

### WP-5.2 — `cargo-mutants` kill-rate baseline on core crates

**CONTEXT:** Mutation testing baseline was last refreshed at sealed
`ea2d9d2`. Phase 1 added crates + churn. This WP runs cargo-mutants
on the 3 most critical crates and produces a kill-rate report.

**ROLE:** Run mutation testing on:
1. `crates/corelink-hash` (cryptographic primitives)
2. `crates/corelink-audit-chain` (audit log integrity)
3. `crates/corelink-byok` (BYOK boundary)

Produce kill-rate table; identify surviving mutants; classify each
(real gap → file follow-up | equivalent mutant → annotate skip).

**SCOPE:**
- `mutants.out/` (cargo-mutants output dir — gitignored, ephemeral)
- `specs/_audits/2026-05-27-mutation-testing-baseline-seal.md` (NEW)
- Optional: `.cargo/mutants.toml` for ignore patterns IF needed

**NON-SCOPE:**
- Any test-code changes (just report; follow-ups are separate WPs)
- Any non-core crate

**DOD:**
1. `cargo mutants -p corelink-hash --check` exits 0 (just the check)
2. `cargo mutants -p corelink-hash --timeout 60 -j2` completes (≥30 min OK)
3. Same for `corelink-audit-chain` and `corelink-byok`
4. SEAL audit reports kill-rate per crate, surviving-mutant list
   classified, recommended follow-up WPs (with severity)
5. Single commit on worktree

**CONSTRAINTS:** §0.1, §0.6.

**ACCEPTANCE GATES:**
```bash
cargo mutants -p corelink-hash --check 2>&1 | tail -3
# Full runs may take 30-90 min total; agent reports progress, doesn't time out itself
```

---

### WP-6.1 — Grafana dashboards JSON for prod SLOs

**CONTEXT:** Existing `dashboards/grafana/` has 5 dashboards
(DASH-{AC,CAS,COST,DEDUP,EXEC}.json). Phase 1 added new emit points
(Sentry, Plausible, BetterStack, newsletter). Refresh dashboards to
cover the Phase 1 observability surface + add 2 new SLO dashboards.

**ROLE:** Add 2 new dashboards:
1. `DASH-SLO-API.json` — API request SLO (p50/p95/p99 latency, error
   rate by endpoint, per-tenant breakdown — top 10 talkers)
2. `DASH-SLO-AUDIT.json` — audit-chain SLO (emit latency, chain
   integrity verification cadence, storage growth rate)

Update existing dashboards with any missing Phase 1 metrics.

**SCOPE:**
- `dashboards/grafana/DASH-SLO-API.json` (NEW)
- `dashboards/grafana/DASH-SLO-AUDIT.json` (NEW)
- Existing `DASH-*.json` files (additive panel updates only)
- `dashboards/README.md` (regen + provisioning instructions update)
- `specs/_audits/2026-05-27-grafana-slo-dashboards-seal.md` (NEW)

**NON-SCOPE:**
- Any non-dashboard file
- Grafana version upgrade

**INPUTS:**
- Existing dashboards use grafonnet-style JSON; mirror that style
- Metric names should match what `crates/corelink-telemetry` emits
  (grep `crates/corelink-telemetry/src/` for `metrics::counter!`,
  `metrics::histogram!`)
- Existing dashboard `DASH-EXEC.json` is the closest sibling — copy
  its variable + panel structure

**DOD:**
1. Both new dashboards valid JSON (`jq . <file> > /dev/null` succeeds)
2. Both dashboards import successfully into Grafana (verify with
   `grafana-tool validate` if available, or `jq` schema sanity)
3. Metric names match emitter-side actual names (grep verify)
4. README updated with import instructions for the 2 new dashboards
5. SEAL audit committed
6. Single commit on worktree

**CONSTRAINTS:** §0.1, §0.6.

---

### WP-6.2 — Python SDK MVP from OpenAPI

**CONTEXT:** OpenAPI spec exists at
`apps/docs/static/openapi-corelink-v1.yaml`. P1 customers will want a
Python client (data-science teams in regulated orgs hit CAS from
Python notebooks).

**ROLE:** Generate a minimal Python SDK from the OpenAPI spec into
`sdks/python/` (new top-level dir). MVP scope: 3 operations (ping,
cas-put, cas-get); auth via Bearer token; pytest suite.

**SCOPE:**
- `sdks/python/` (NEW dir)
- `sdks/python/pyproject.toml`, `sdks/python/README.md`,
  `sdks/python/LICENSE` (Apache-2.0)
- `sdks/python/corelink/__init__.py`, `corelink/client.py`,
  `corelink/exceptions.py`
- `sdks/python/tests/test_client.py` (≥5 tests with `httpx` mock)
- `specs/_audits/2026-05-27-python-sdk-mvp-seal.md` (NEW)

**NON-SCOPE:**
- Publishing to PyPI (deferred)
- A separate `humangr-labs/corelink-py` repo (deferred — start
  in-monorepo)
- More than 3 operations
- Async support (sync-only MVP)

**INPUTS:**
- OpenAPI spec at `apps/docs/static/openapi-corelink-v1.yaml`
- Python version: 3.11+
- Use `httpx` for transport (sync), `pydantic` for response models
- Match Rust SDK shape (`crates/corelink-client-verify/`) where useful

**DOD:**
1. `cd sdks/python && python -m pytest -v` exits 0
2. `cd sdks/python && python -m mypy corelink/` exits 0
3. `cd sdks/python && python -m ruff check corelink/` exits 0
4. README has quickstart, install (`pip install -e .`), usage, license
5. License Apache-2.0
6. SEAL audit at the path above
7. Single commit on worktree

**CONSTRAINTS:** §0.1, §0.6. Python: type-hints required; mypy clean.

**ACCEPTANCE GATES:**
```bash
cd sdks/python
python -m pip install -e ".[dev]" 2>&1 | tail -3
python -m pytest -v 2>&1 | tail -10
python -m mypy corelink/ 2>&1 | tail -3
python -m ruff check corelink/ 2>&1 | tail -3
```

---

### WP-7.1 — Synthetic monitoring probes (BetterStack scripted)

**CONTEXT:** Phase 1 brought BetterStack live. Phase 4.8 (bug-bash +
retention) implies proactive uptime detection. This WP scripts
synthetic probes that BetterStack will run from external POPs.

**ROLE:** Define 5 synthetic probes + commit the probe definitions as
YAML manifests + a runbook to apply them via BetterStack API.

**SCOPE:**
- `monitoring/synthetic/probes.yml` (NEW dir + file)
- `monitoring/synthetic/README.md`
- `scripts/apply-betterstack-probes.sh`
- `specs/_audits/2026-05-27-synthetic-monitoring-seal.md` (NEW)

**NON-SCOPE:**
- Any live API call to BetterStack (dry-run only)
- Modification of BetterStack page itself

**INPUTS:**
- Probe targets (per Wave 32 §G DNS inventory):
  1. `https://corelink-api.humangr.com/health` — 30s interval, expect 200
  2. `https://corelink-app.humangr.com` — 60s, expect 200 + Clerk-managed
  3. `https://corelink-docs.humangr.com` — 60s, expect 200
  4. `https://corelink-signup.humangr.com` — 60s, expect 200
  5. `https://corelink-get.humangr.com` — 60s, expect 200 + `text/plain`
- Each probe defines: URL, method, interval, timeout, status assertion,
  body assertion (optional), regions to probe from (US-east, EU, AP)

**DOD:**
1. `probes.yml` valid YAML (`yq` or `python -c "import yaml; yaml.safe_load(open(...))"` clean)
2. Apply script `--dry-run` lists all 5 probes and the API calls it
   would make, NO real calls
3. README documents creation/update/delete via the script
4. SEAL audit committed
5. Single commit on worktree

**CONSTRAINTS:** §0.1, §0.4 (any GHA), §0.6.

---

### WP-7.2 — SBOM + license audit refresh post-Phase-1

**CONTEXT:** Phase 1 added new deps (Sentry SDK, Clerk SDK, Resend,
PostHog client, BetterStack). FF-HR-005 charter requires license
allowlist + SBOM artifact. Refresh both.

**ROLE:** Re-generate SBOM (CycloneDX) for the full workspace + admin-ui
+ docs + workers; audit licenses against allowlist (Apache-2.0, MIT,
BSD-2/3, ISC, MPL-2.0); flag any non-allowed.

**SCOPE:**
- `.sbom/cyclonedx-rust.json` (NEW or refresh)
- `.sbom/cyclonedx-npm.json` (NEW or refresh)
- `scripts/generate-sbom.sh` (harden / re-author)
- `specs/_compliance/license-allowlist.md` (extend with new deps)
- `specs/_audits/2026-05-27-sbom-license-audit-seal.md` (NEW)

**NON-SCOPE:**
- Removing any existing dep (just flag, don't remove)
- Rust crate-level Cargo.toml edits

**INPUTS:**
- Allowlist (from charter): Apache-2.0, MIT, BSD-2-Clause, BSD-3-Clause,
  ISC, MPL-2.0, CC0-1.0
- Tools: `cargo cyclonedx` (or `cargo sbom`), `npx @cyclonedx/cyclonedx-npm`
- Existing SBOM dir state: verify with `ls .sbom/`

**DOD:**
1. Rust SBOM generated, JSON valid
2. NPM SBOM generated (separate file per app: admin-ui, docs)
3. License audit table in SEAL doc: total deps, allowed count, flagged
   count (≤ 2 acceptable; if >2 flagged, escalate via audit doc)
4. Generation script `--dry-run` works; live mode produces fresh files
5. SEAL audit committed
6. Single commit on worktree

**CONSTRAINTS:** §0.1, §0.6.

---

### WP-7.3 — Quickstart docs refresh aligned to corelink-cli MVP

**CONTEXT:** corelink-cli MVP is live at `71d7f6a9` with `ping` +
`bazel-init`. Docs quickstart references the older onboarding flow.
Refresh quickstart to match what an HN visitor actually does:
sign-up → install one-liner → ping → bazel-init → first cache hit.

**ROLE:** Refresh `apps/docs/docs/tutorial/quickstart.mdx` (or whatever
path the current quickstart lives at — find via grep) to walk a new
user from zero to first-cache-hit in ≤ 10 minutes (TTFV target per
ROADMAP §4).

**SCOPE:**
- `apps/docs/docs/**/quickstart*.mdx` (locate via find/grep first)
- 4 locale variants under `apps/docs/i18n/<locale>/.../`
- `specs/_audits/2026-05-27-quickstart-refresh-seal.md` (NEW)

**NON-SCOPE:**
- Any non-quickstart doc
- The blog
- CLI source

**STRUCTURE:**
1. **Before you start** — prerequisites (Bazel 7+, browser for signup)
2. **Step 1** — sign up at https://corelink-app.humangr.com
3. **Step 2** — copy your PAT from `/welcome`
4. **Step 3** — `curl -fsSL https://corelink-get.humangr.com | sh -s --
   --token=ct_xxx --region=ord`
5. **Step 4** — `corelink ping` — should print latency + 200 OK
6. **Step 5** — cd into your Bazel repo, `corelink bazel-init`
7. **Step 6** — `bazel build //... && bazel build //...` — second
   build = mostly cache HITs
8. **Troubleshooting** — common failure modes (regional mismatch,
   token-not-set, .bazelrc duplicate-line)
9. **Next steps** — link to bazel-example repo, blog post #1, comparison
   pages

**DOD:**
1. `cd apps/docs && pnpm build` exits 0 (4 locales)
2. Quickstart rendered HTML lists all 6 steps + troubleshooting
3. All 4 locales updated (translated copy OR canonical-EN-link stub)
4. Every URL referenced in quickstart returns HTTP 200
5. SEAL audit committed
6. Single commit on worktree

**CONSTRAINTS:** §0.1, §0.6.

---

## §3 Dispatch order

All 15 are **safe to dispatch in a single parallel batch** (see conflict
matrix in §1). Each agent gets its own isolated worktree per master
constraint §0.

Orchestrator pattern per agent:
```
Agent({
  subagent_type: "general-purpose",
  description: "<WP-X.Y short>",
  model: "sonnet",
  isolation: "worktree",
  run_in_background: true,
  prompt: "<full contract from §2>"
})
```

## §4 Merge order (when SEALs arrive)

When agents complete (in unpredictable order), orchestrator merges
**by completion time**, NOT by WP ID. Each merge:
1. Runs L0-L7 tech-lead verdict
2. Resolves any conflicts via §1 conflict-matrix guidance
3. Pushes after every 3-5 merges (per techlead L10)

## §5 Hard pause triggers (orchestrator-bound)

Stop dispatching / stop merging if:
- ≥3 agents return identical blocker → systemic issue, investigate first
- Phase B (WP-1.1) flags a Wave 32 §7 hard pause trigger
- `validate_specs.py` regresses below 0 (NEVER allow)
- Disk free < 5 GB (target dir explosion — clean first)

## §6 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of 15-agent dispatch matrix.**
