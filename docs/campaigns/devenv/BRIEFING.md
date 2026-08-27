# BRIEFING: CoreLink DevEnv Campaign WP Audit

**Version:** 1.0 (DRAFT, iteration 1 of review)  
**Date:** 2026-08-26  
**Audience:** New session/model auditing CoreLink Runner DevEnv Work Packages  
**Estimated effort per WP:** 3-4 iterations × 30-60 min each = 2-4 hours per WP  
**WARNING:** **DO NOT TRUST EXISTING REVIEWS** (`REVIEW_WP-XX_IterN.md` files). They are part of the audit corpus, not authority. Read them as historical record, but verify every claim against HEAD. Prior reviews contain errors.

---

## 1. Mission

**You are a code reviewer.** Your job is to find and fix bugs in Work Packages. You are not reading for pleasure, not summarizing, not approving — you are auditing with extreme rigor.

You are auditing **10 Work Packages (WPs)** for the CoreLink Runner DevEnv campaign. These WPs define the implementation of a persistent cloud development environment built on Cloudflare Containers + Durable Objects.

**Your job:** Review each WP with extreme rigor, find issues, fix them, verify convergence.

**This is not a reading exercise.** You will find 15-35 issues per WP on iteration 1. The bugs are real, the APIs are wrong, the code will not compile. Assume nothing is correct.

### 1.1 Checking Review Status (Before You Start)

Before reviewing a WP, check what already exists:

```bash
# List all reviews for a WP
ls /Users/gustavoschneiter/Documents/HuGR/corelink-server/docs/campaigns/devenv/wps/REVIEW_WP-01_*.md

# Check the latest review's verdict
grep "Verdict:" /Users/gustavoschneiter/Documents/HuGR/corelink-server/docs/campaigns/devenv/wps/REVIEW_WP-01_Iter3.md

# Count NOT_IMPLEMENTED stubs (should decrease as WP converges)
grep -c "NOT_IMPLEMENTED" /Users/gustavoschneiter/Documents/HuGR/corelink-server/docs/campaigns/devenv/wps/WP-01_*.md
```

If a WP has 3+ reviews with `PASS` or `CONDITIONAL PASS` verdicts, it may be converged. But **DO NOT TRUST THE REVIEWS** — verify convergence yourself by running the review process again.

### 1.2 Managing Existing Reviews (Do NOT Delete)

**Keep all existing `REVIEW_*.md` files as part of the audit trail.** New iterations create new `REVIEW_IterN.md` files. Never delete prior reviews — they are the historical record of what was found, what was fixed, and what was missed.

---

## 2. Location & Inventory

### 2.1 Where Everything Lives

```
/Users/gustavoschneiter/Documents/HuGR/corelink-server/docs/campaigns/devenv/wps/
```

### 2.2 File Inventory (as of 2026-08-26)

| Type | Filename | Status |
|------|----------|--------|
| **WP Spec** | `WP-01_RunnerDevEnvDO_Skeleton.md` | ✅ Converged (4 iterations) |
| **WP Spec** | `WP-02_Dockerfile_runnner_devenv.md` | ⚠️ Conditional PASS (4 iterations) |
| **WP Spec** | `WP-03_Entrypoint_Supervisord.md` | ✅ PASS (3 iterations) |
| **WP Spec** | `WP-04_clw_Integration.md` | ⚠️ Conditional (3 iterations) |
| **WP Spec** | `WP-05_WebSocket_Proxy.md` | ⚠️ Conditional (3 iterations) |
| **WP Spec** | `WP-06_DO_Lifecycle.md` | ⚠️ Conditional (3 iterations) |
| **WP Spec** | `WP-07_Billing_Metering.md` | ⚠️ Conditional (3 iterations) |
| **WP Spec** | `WP-08_Worker_Ingress_Routes.md` | ⚠️ Conditional (3 iterations) |
| **WP Spec** | `WP-09_Dashboard_UI.md` | ⚠️ Conditional (3 iterations) |
| **WP Spec** | `WP-10_Dogfood_Testing_Docs.md` | ✅ PASS (4 iterations) |
| **Review** | `REVIEW_WP-XX_IterN.md` | One per WP per iteration |

### 2.3 Finding the WPs (Commands)

```bash
# List all WPs
ls /Users/gustavoschneiter/Documents/HuGR/corelink-server/docs/campaigns/devenv/wps/ | grep "^WP-"

# List all reviews
ls /Users/gustavoschneiter/Documents/HuGR/corelink-server/docs/campaigns/devenv/wps/ | grep "^REVIEW"

# Count
ls /Users/gustavoschneiter/Documents/HuGR/corelink-server/docs/campaigns/devenv/wps/ | wc -l
```

---

## 3. What a WP Contains (Template Structure)

Each WP follows this structure. **All sections MUST be present and substantive:**

| Section | Purpose | What to look for |
|---------|---------|------------------|
| Header (Status, Owner, Depends, Estimate, Priority) | Metadata | Wrong metadata, missing dependencies |
| 1. Objective | What this WP builds | Vague goals, missing scope boundaries |
| 2. Scope (In/Out) | Boundaries | Missing Out-of-Scope, scope creep |
| 3. Technical Specification | Exact code/config | **THIS IS WHERE BUGS LIVE** — wrong APIs, fabricated methods, impossible code |
| 4. Acceptance Criteria (DoD) | Verifiable outcomes | Vague criteria, unverifiable claims |
| 5. Invariants | Properties that must always hold | Invariants not enforced in code, wrong invariants |
| 6. Quality Standards (SOTA) | Excellence criteria | Standards claimed but not met |
| 7. Completeness Checklist | Implementation tasks | Missing tasks, vague items |
| 8. Self-Check Points (3+) | Agent self-evaluation | Self-checks that pass even when WP fails |
| 9. Risk Register | Known risks | Missing critical risks, impact misjudged |
| 10. Sign-Off | Approval table | Empty sign-off |

---

## 4. The WPs Reference Real Code — You Must Verify Against It

**The WPs are NOT standalone documents.** They reference real systems in the corelink monorepo. Every WP must be verified against the actual code it claims to use.

### 4.1 Code Repos to Verify Against

| Repo | Location | What it contains |
|------|----------|------------------|
| `corelink-server` | `/Users/gustavoschneiter/Documents/HuGR/corelink-server/` | This repo: Worker, Durable Objects, Rust data plane, D1, CAS/AC, admin-ui, runbooks, billing ingest |
| `corelink-runners` | `/Users/gustavoschneiter/Documents/HuGR/corelink-runners/` | **Cloudflare Container DO patterns**: `RunnerContainer`, `CheckHostContainer` (extends `@cloudflare/containers` Container class), `CloudflareEngine` (Rust), spawn-Worker (TS), fabric orchestration |
| `corelink-workspaces` | `/Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/` | **`clw` CLI binary** + all clw-* libraries: `clw-types` (frozen contract), `clw-snapshot`, `clw-hydrate`, `clw-run`, `clw-cli`, `clw-manifest`, `clw-chunk`, `clw-cache`, `clw-client`, `clw-conformance`, `clw-integration-tests` |

### 4.2 Critical Code Paths to Read

```bash
# 1. Cloudflare Container DO patterns (RUNNER, not DevEnv — but patterns apply)
ls /Users/gustavoschneiter/Documents/HuGR/corelink-runners/deploy/cloudflare/src/

# 2. clw CLI binary structure
ls /Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-cli/src/

# 3. clw-types (frozen contract)
cat /Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-types/src/lib.rs

# 4. Existing Worker routes (for WP-08)
ls /Users/gustavoschneiter/Documents/HuGR/corelink-server/worker/src/

# 5. Admin-ui (for WP-09)
ls /Users/gustavoschneiter/Documents/HuGR/corelink-server/apps/admin-ui/src/

# 6. Existing runbooks (for WP-10)
ls /Users/gustavoschneiter/Documents/HuGR/corelink-server/specs/_runbooks/ | head -20
```

### 4.3 Common Lies the WPs Tell

These are the API lies discovered in iteration 1. **Check for these specifically:**

| Lie | Truth | Where the truth lives |
|-----|-------|----------------------|
| `this.ctx.container.exec()` exists | **Does not exist.** `@cloudflare/containers` has `containerFetch`, not `exec`. | `/Users/gustavoschneiter/Documents/HuGR/corelink-runners/node_modules/@cloudflare/containers/dist/lib/container.d.ts` |
| `Container.alarm()` re-arms to `Date.now() + interval` | **Re-arms to `Date.now()` immediately.** Use `this.schedule()` instead. | `/Users/gustavoschneiter/Documents/HuGR/corelink-runners/node_modules/@cloudflare/containers/dist/lib/container.js:1518` |
| `clw` has an `exec` subcommand | **Does not exist.** Subcommands: `init/snapshot/hydrate/status/run/ls/rm/prune/erase/doctor/auth/uninstall/completions` | `/Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-cli/src/main.rs:163-191` |
| `clw hydrate --json` returns `chunks_total` | **Only snapshot returns that field.** Hydrate returns `root/files/bytes_total/bytes_from_cache/bytes_downloaded`. | `/Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-hydrate/src/lib.rs` |
| `clw` exit code 2 = "not found" | **Exit 2 = ANY clw error** (auth, network, config, not-found). Use `clw ls --name X` to pre-check. | `/Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-cli/src/main.rs` |
| `super.fetch()` routes by port | **Routes to `defaultPort` only.** Use `this.ctx.container.getTcpPort(port).fetch()` for port-specific routing. | `/Users/gustavoschneiter/Documents/HuGR/corelink-runners/deploy/cloudflare/src/index.ts` |
| `addEventListener` works on hibernated WS | **Doesn't survive eviction.** Must use `webSocketMessage`/`Close`/`Error` RUNTIME handlers. | Cloudflare DO docs |
| `corelink-runners/crates/clw-cli/` exists | **It does NOT.** The `clw` binary is built from `corelink-workspaces/crates/clw-cli/`. The runner repo has `corelink-runner`, `corelink-cloud-engine`, `corelink-fabric-*` — NOT clw. | `ls /Users/gustavoschneiter/Documents/HuGR/corelink-runners/crates/` vs `ls /Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/` |
| `tauri`, `tldr`, `actix-web`, `axum` are in `corelink-server` | `axum` is. `tauri`, `tldr`, `actix-web` are not. | `/Users/gustavoschneiter/Documents/HuGR/corelink-server/crates/corelink-container/Cargo.toml` |
| `apps/admin-ui` uses shadcn `@/components/ui/button` | **Wrong.** Uses Linear kit `@/components/ui/linear/Button`. | `/Users/gustavoschneiter/Documents/HuGR/corelink-server/apps/admin-ui/src/components/ui/` |
| `useToast` has `.error()` method | **Signature is `toast({ title, tone })`.** No `.error()`. | `/Users/gustavoschneiter/Documents/HuGR/corelink-server/apps/admin-ui/src/hooks/use-toast.ts` |
| `@tanstack/react-query` is used | **Not in admin-ui.** Uses `useCustomerClient` + `CustomerClient` class. | `/Users/gustavoschneiter/Documents/HuGR/corelink-server/apps/admin-ui/package.json` |
| `lucide-react` icons | **Inline SVGs.** No lucide-react. | `/Users/gustavoschneiter/Documents/HuGR/corelink-server/apps/admin-ui/src/components/` |
| `cloudflare:workers` export of `Container` class | **Container comes from `@cloudflare/containers`, not `cloudflare:workers`.** | `/Users/gustavoschneiter/Documents/HuGR/corelink-runners/deploy/cloudflare/src/index.ts:15` |
| `apps/signup-worker/scripts/d1.py` exists | **Does not exist.** | `find apps/signup-worker -name "*.py"` |
| Runner tiers = Starter/Pro/Team/Scale/Max with 1/2/4/8/16 concurrent DevEnvs | **DO namespace is `idFromName(tenantId)` = 1 DO per tenant. "Pro=2 concurrent" is physically impossible.** | `/Users/gustavoschneiter/Documents/HuGR/corelink-runners/deploy/cloudflare/src/index.ts:395` |
| `migrations/d1/0090_runner_devenv_usage.sql` migration | **Migration 0090 is `0090_dsr_tickets.sql`.** Collision. | `/Users/gustavoschneiter/Documents/HuGR/corelink-server/migrations/d1/` |
| `corelink-workspaces` repo does NOT have clw-* crates | **WRONG — `corelink-workspaces` IS where clw-* crates live.** | `ls /Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/` |
| `d1_http` binding name for billing DB | **It's `CONFIG_DB` per `worker/src/index.ts:82`** | `/Users/gustavoschneiter/Documents/HuGR/corelink-server/worker/src/index.ts` |

---

## 5. The Review Methodology (SOTA)

### 5.1 Per-Iteration Process

For each WP, execute these steps per iteration:

```
1. READ:  Read the WP file end-to-end
2. READ:  Read the most recent REVIEW_WP-XX_IterN.md to understand prior findings
3. READ:  Read the actual code paths the WP claims to use (see §4)
4. VERIFY: Check each prior iteration's fixes were applied correctly (spot-check 5-10)
5. FIND:  Look for new issues using the patterns in §6
6. DOCUMENT: Write REVIEW_WP-XX_IterN.md (see §7 for template)
7. FIX:   Apply ALL BLOCKING + HIGH fixes directly to the WP file
8. REPORT: Output a terse summary
```

### 5.2 Convergence Rule

A WP is **CONVERGED** when:
- **Iter N:** 0 new BLOCKING AND 0 new HIGH issues
- **Iter N+1:** 0 new BLOCKING AND 0 new HIGH issues (2 consecutive clean iterations = PASS)
- **Expected convergence:** 3-4 iterations

**Clean iteration** = 0 BLOCKING + 0 HIGH. MEDIUM issues are allowed and do NOT block convergence (they are tracked but not blocking).

If iter 1 finds > 35 issues, expect 4-5 iterations. If iter 3 still finds > 3 BLOCKING, something is structurally wrong — investigate the cross-WP dependencies.

### 5.3 Issue Classification

| Severity | Definition | Action |
|----------|-------------|--------|
| **BLOCKING** | Code won't compile, API doesn't exist, logic is broken, security hole | **MUST fix in current iteration** |
| **HIGH** | Design flaw, missing error handling, race condition, will fail in prod | **MUST fix in current iteration** |
| **MEDIUM** | Inconsistency, style, missing test, edge case | Fix if time permits, else track |

### 5.4 What "Fix" Means

**DO** edit the WP file directly with the Edit tool. The WP is the source of truth.

**DO NOT** write a separate "fix proposal" document. The WP must be updated.

**DO NOT** ask the user for approval. Just fix and document.

---

## 6. What to Look For (Pattern Library)

### 6.1 Code-API Mismatch (Most Common)

The WP claims an API exists. **It doesn't.** The WPs were written from imagination, not from reading the SDK.

**How to check:** For every API call in the WP code blocks, verify it exists in the actual SDK or repo. Use `grep` and `find` liberally.

```bash
# Example: verify `Container.exec` exists
grep -r "exec" /Users/gustavoschneiter/Documents/HuGR/corelink-runners/deploy/cloudflare/src/

# Example: verify `clw exec` subcommand exists  
grep -A 5 "fn exec" /Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-cli/src/main.rs

# Example: verify shadcn path
ls /Users/gustavoschneiter/Documents/HuGR/corelink-server/apps/admin-ui/src/components/ui/
```

### 6.2 Self-Referential / Recursive Calls

The WPs define RPC methods that accidentally call themselves instead of parent class methods.

**Pattern:** Look for any RPC method that calls `this.methodName()` when a parent class also has that method. The fix is `super.methodName()`.

**Example from WP-01 iter 1:**
```typescript
async start(payload) {
  await this.start({...});  // RECURSIVE! Should be super.start({...})
}
```

### 6.3 State Machine Bypasses

The WPs mutate `this.state` directly, bypassing the `transitionState()` validator.

**Pattern:** Search for `this.state = {` or `this.state.workspaceName =`. All state mutations must go through `transitionState()`.

### 6.4 Misuse of Container DO API

The WPs invent Container DO methods that don't exist (`this.container.stop()`, `this.container.exec()`).

**Pattern:** For every `this.container.*` call, check if the Container class has that method. The Container class has: `start`, `stop`, `destroy`, `monitor`, `signal`, `getTcpPort`, `fetch`, `renewActivityTimeout`. It does NOT have `exec` (it has `getTcpPort` for HTTP/WS, but no arbitrary command exec).

### 6.5 Missing Cross-WP Coordination

The WPs assume other WPs have implemented things they haven't.

**Pattern:** Every WP that references another WP must check that the referenced contract is actually there. Look for:
- "Implementation in WP-XX" → check WP-XX actually has the API the caller expects
- `STATIC_ENV_VARS.CLW_REF_DOMAIN` → check WP-01 actually exposes this
- `getTcpPort(port).fetch()` → check WP-05/06 actually use this pattern

### 6.6 CLI Command Fabrications

The WPs invent clw flags or subcommands that don't exist.

**Pattern:** Every `clw snapshot ...`, `clw hydrate ...`, `clw run ...` command must match the actual `clw-cli` binary. Read `/Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-cli/src/main.rs` to verify.

### 6.7 DoD / Invariant / Quality Standard Gaps

The WPs state DoD criteria, invariants, and quality standards, but the code doesn't enforce them.

**Pattern:** For every DoD item, verify the code actually implements it. For every invariant, verify the code actually maintains it. For every quality standard, verify the code meets it.

### 6.8 HTTP/WebSocket Lifecycle Bugs

The WPs have fundamental misunderstanding of WebSocket hibernation, HTTP upgrade, and connection lifecycle.

**Pattern:** 
- WebSocket hibernation: must use `webSocketMessage`/`Close`/`Error` RUNTIME handlers, not `addEventListener`
- HTTP upgrade: must include `Upgrade: websocket` and `Connection: Upgrade` headers
- Connection cleanup: must handle BOTH client and server close events

### 6.9 Billing/Quoting Math Errors

The WPs have vCPU-second calculation errors, wrong markup percentages, and physically impossible concurrency limits.

**Pattern:** vCPU-seconds = wall_seconds × vCPU_count. Markup is a percentage. Concurrency = number of DOs, not a tier config.

### 6.10 Process / Shell Script Bugs

The entrypoint.sh and supervisord.conf have quoting errors, wrong signal handlers, and unsafe patterns.

**Pattern:**
- `set -euo pipefail` at the top
- `trap` on EXIT, TERM, INT for cleanup
- Quote all variables: `"$var"`, never `$var`
- Supervisord: `autorestart=true`, `startsecs` for startup delay, `priority` for ordering

---

## 7. Review Document Template (Per Iteration)

Each review must be at `/Users/gustavoschneiter/Documents/HuGR/corelink-server/docs/campaigns/devenv/wps/REVIEW_WP-XX_IterN.md`.

```markdown
# WP-XX REVIEW — Iteration N (DESCRIPTION)

**Reviewer:** [Your name/role]
**Date:** [Date]
**Previous Verdicts:** [Iter 1-2 verdicts]
**New Verdict:** ❌/⚠️/✅ [Verdict]

---

## Summary (Terse)

- Issues found: X BLOCKING / Y HIGH / Z MEDIUM
- Files modified: [list]
- Cross-WP needs: [list]
- Convergence: [Y/N]

---

## 🔴 BLOCKING Issues

### B[N]: [Title]
- **Location:** [File:line]
- **Problem:** [What's wrong]
- **Fix:** [What to do]

[Repeat for each BLOCKING]

---

## 🟠 HIGH Issues

### H[N]: [Title]
- **Location:** [File:line]
- **Problem:** [What's wrong]
- **Fix:** [What to do]

[Repeat for each HIGH]

---

## 🟡 MEDIUM Issues

### M[N]: [Title]
- **Location:** [File:line]
- **Problem:** [What's wrong]
- **Fix:** [What to do]

[Repeat for each MEDIUM]

---

## DoD / Invariants / Quality Standards Verification

[Brief table of what passed/failed]

---

## Cross-WP Coordination Needs

- [WP-YY]: [What needs to change in WP-YY]

---

## Verdict

[One paragraph: PASS / CONDITIONAL PASS / FAIL with reasoning]
```

---

## 8. Per-Iteration Reporting (Output Format)

After each iteration, output a TERSE report:

```
**Iter N Report:**
- Issues found: X BLOCKING / Y HIGH / Z MEDIUM
- Files modified: [list of files]
- Verdict: ❌/⚠️/✅
- Cross-WP needs: [list]
- Convergence: [Y/N, expected new issues in next iter]
```

Do NOT write essays. Do NOT repeat the full review. The review file has the detail.

---

## 9. Anti-Patterns (Lessons from Prior Iterations)

These mistakes were made and should NOT be repeated:

1. **Trusting citations without HEAD verification.** WP-10 iter 1 cited `apps/signup-worker/scripts/d1.py` which doesn't exist. WP-10 iter 1 cited `seedTenantEntitlements` (a non-existent function) for `runners_entitlement` seeding. **Always `grep` or `ls` to verify before citing.**

2. **Fixing one bug introducing another.** WP-01 iter 2 introduced B13 (recursive `this.stop()`) while fixing B1/B2/B3. **When fixing, re-read the surrounding code.**

3. **Stopping at "looks good to me".** WP-01 needed 4 iterations to converge. WP-02 needed 4. **Always run at least 3 iterations unless the first two were 100% clean.**

4. **Marking PASS after fixing without re-verifying.** The iter 2 reviews caught 20+ regressions in WP-05 alone. **Every fix must be re-verified in the next iteration.**

5. **Ignoring the `as any` cast.** `as any` is a type system escape hatch. Every `as any` is a potential bug hiding. **Find and fix every `as any`.**

6. **Trusting "verified by code review".** None of the WPs had actual code review verification. They were written from imagination. **The only verification is `grep` against HEAD.**

7. **Skipping the cross-WP coordination check.** Every WP references other WPs. If WP-XX says "see WP-YY for X", check WP-YY actually has X.

8. **Not reading the actual repo structure.** The `clw-*` crates are in `corelink-workspaces/crates/`, not `corelink-runners`. The admin-ui uses Linear kit, not shadcn. The billing DB is `CONFIG_DB`, not `D1_HTTP`. **Read the repo before you read the WP.**

9. **"This is a doc, not code, so minor errors don't matter."** FALSE. The WPs will be implemented verbatim. Every wrong API call, every wrong flag, every wrong path becomes a bug. **Treat the WP as production code.**

10. **Batching edits and losing track.** When you find 25 issues, fix them in order, but re-read the WP after the 10th edit to make sure the earlier fixes are still consistent. **The WPs are large files (500-1000 lines). Edits can interact.**

---

## 10. Tools & Commands

### 10.1 Essential grep Patterns

```bash
# Find all API calls in code blocks only (not narrative text)
# Extract code blocks first, then grep
awk '/^```typescript/,/^```$/' WP-XX_*.md | grep -n "this\."

# Find all `clw` command invocations (more specific)
grep -nE "clw (snapshot|hydrate|run|ls|rm|prune|erase|status|init|doctor|auth)" WP-XX_*.md

# Find all `as any` (type system escape)
grep -n "as any" WP-XX_*.md

# Find all NOT_IMPLEMENTED (which should decrease as WPs converge)
grep -nc "NOT_IMPLEMENTED" WP-XX_*.md
```

### 10.2 Cross-Reference Checks

```bash
# Check if a cited file exists
ls /path/to/cited/file

# Check if a cited function exists
grep -n "fn functionName" /path/to/source.rs

# Check if a cited migration exists
ls /Users/gustavoschneiter/Documents/HuGR/corelink-server/migrations/d1/

# Check if a cited env var is in wrangler.toml
grep "ENV_VAR" /Users/gustavoschneiter/Documents/HuGR/corelink-server/worker/wrangler.toml
```

### 10.3 Verify Actual SDK/API

```bash
# Cloudflare Container DO API
find /Users/gustavoschneiter/Documents/HuGR/corelink-runners -name "node_modules" -prune -o -name "*.ts" -print | xargs grep -l "extends Container" 2>/dev/null

# clw CLI subcommands
grep "subcommand" /Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-cli/src/main.rs

# clw flags
grep "long =" /Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-cli/src/main.rs
```

---

## 11. Execution Plan

### 11.1 Recommended Order

Audit in this order (dependencies first):

1. **WP-01** (RunnerDevEnvDO Skeleton) — Foundational class
2. **WP-02** (Dockerfile) — Image build
3. **WP-03** (Entrypoint/Supervisord) — Container runtime
4. **WP-04** (clw Integration) — Uses WP-01, WP-02, WP-03
5. **WP-05** (WebSocket Proxy) — Uses WP-01
6. **WP-06** (DO Lifecycle) — Uses WP-01, WP-04, WP-05
7. **WP-07** (Billing) — Uses WP-04, WP-06
8. **WP-08** (Worker Ingress) — Uses WP-01, WP-07
9. **WP-09** (Dashboard UI) — Uses WP-08
10. **WP-10** (Dogfood/Docs) — Uses all

### 11.2 Per-WP Time Budget

- **Iter 1:** 60-90 min (read WP, verify APIs, find issues, document, fix)
- **Iter 2:** 30-45 min (regression check + missed issues)
- **Iter 3:** 15-30 min (convergence check)
- **Iter 4 (if needed):** 15-20 min (final cleanup)

**Total per WP:** 2-3 hours
**Total for all 10 WPs:** 20-30 hours

### 11.3 Parallel Execution

You can run multiple WP reviews in parallel using sub-agents. Spawn one per WP. Each sub-agent gets this briefing + a specific WP to audit.

---

## 12. What "Done" Looks Like

When ALL of the following are true, the campaign is ready for implementation:

- [ ] All 10 WPs have a `REVIEW_WP-XX_Iter3.md` (or later) marked PASS or CONDITIONAL PASS
- [ ] Zero BLOCKING issues remain in any WP
- [ ] Zero HIGH issues remain in any WP
- [ ] All cross-WP coordination needs are documented
- [ ] All WPs compile (mentally) when read together
- [ ] All DoD items are implementable as stated
- [ ] All invariants are enforced in code
- [ ] All quality standards are met

---

## 13. Emergency Protocols

### 13.1 If You Find a Bug That Affects Multiple WPs

1. **STOP** and document the bug in the current WP's review
2. **List** all affected WPs
3. **Fix** the current WP
4. **Flag** the cross-WP issue for the next WP's iteration
5. **Do NOT** try to fix all WPs simultaneously — iterate one at a time

### 13.2 If You Find a Bug in the Actual Code (Not the WP)

The WPs describe intended behavior. If the actual code in the repo contradicts the WP:

1. **Check which is correct** — read the Cloudflare docs, check the clw binary, verify the actual API
2. **Fix the WP** to match the actual correct behavior
3. **Do NOT** try to fix the actual code (that's a separate effort, post-audit)

### 13.3 If You're Stuck

If you can't determine if something is correct:

1. **Read the source:** `/Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-cli/src/main.rs` for the clw binary, and `/Users/gustavoschneiter/Documents/HuGR/corelink-runners/node_modules/@cloudflare/containers/dist/lib/container.d.ts` for the Container class.
2. **Read the test** (if it exists) — `/Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-cli/tests/` and `/Users/gustavoschneiter/Documents/HuGR/corelink-runners/crates/corelink-runner/tests/` have integration tests.
3. **Read the docs** (Cloudflare DO docs, clw-types frozen contract in `/Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-types/src/lib.rs`)
4. **Document** what you found and what you're uncertain about
5. **Do NOT** guess. The WPs are full of confident guesses that are wrong.

---

## 14. Appendix: Complete Issue Catalog (284 issues found across 31 iterations)

This is the historical record. Use it to know what kinds of issues to look for.

### 14.1 Most Common Issue Types (by frequency)

| Type | Count | Example |
|------|-------|---------|
| API doesn't exist (fabricated) | ~40 | `this.container.exec()`, `clw exec`, `@tanstack/react-query` |
| Recursive call (method calls itself) | ~5 | `this.start()` calling itself |
| State machine bypass | ~8 | Direct `this.state =` mutation |
| Cross-WP contract mismatch | ~25 | WP-04 references WP-01 field that doesn't exist |
| Wrong CLI flag | ~15 | `--chunks_total` on hydrate (doesn't exist) |
| Missing error handling | ~20 | Uncaught promise rejection, no timeout |
| Missing validation | ~15 | No input validation on RPC |
| Type system escape | ~12 | `as any`, `(this.state as any).x` |
| Misunderstood API lifecycle | ~18 | Hibernation, alarm, exec, health check |
| Math/logic errors | ~10 | vCPU calculation, quota limits, timestamps |
| File path / directory errors | ~15 | Non-existent directories, wrong paths |
| Naming convention drift | ~10 | snake_case vs camelCase, endpoint URLs |
| Citation drift | ~12 | Cited function/file that doesn't exist |
| Container exec API (phantom) | ~8 | `ctx.container.exec()` doesn't exist |
| Inconsistent constants | ~10 | `MAX_SOFT_STOPS_BEFORE_DESTROY` referenced but not defined |
| Off-by-one / boundary errors | ~8 | `if (status === "stopped" || "stopping")` (bug) |
| Missing tests | ~10 | No unit tests, no integration tests |
| Missing documentation | ~8 | No JSDoc, no inline comments |
| Security gaps | ~8 | No auth, no rate limiting, no input sanitization |
| Quoting / escaping (shell) | ~5 | Unquoted variables in entrypoint.sh |
| Signal handling | ~5 | Wrong trap order, missing SIGINT |
| Process ordering | ~5 | Chromium starts before xvfb |
| Environment variable contract | ~8 | WP-01 says var X, WP-03 says var Y |
| Exit code interpretation | ~5 | clw exit 2 = any error, not "not found" |
| Wrong port in supervisord | ~3 | code-server on port 8080 conflicts with noVNC |
| Missing volume mount | ~3 | /data/chrome not writable by coder |
| Missing health check | ~5 | No port readiness check |
| Missing graceful shutdown | ~5 | No SIGTERM handler |
| Resource leak | ~8 | Event listeners not removed, alarms not cancelled |
| Cache invalidation | ~3 | Stale state in DO after restart |
| Race condition | ~8 | Concurrent snapshot calls |
| Transaction boundary | ~5 | State persisted before operation completes |

### 14.2 Worst Single Bugs (Caught in Reviews)

1. **WP-01 B1:** `async start()` calls `this.start()` recursively → infinite loop at runtime
2. **WP-01 B2/B3:** `this.container.stop()` doesn't exist (Container class has `this.stop()` not `this.container.stop()`)
3. **WP-04/H6:** `this.ctx.container.exec()` doesn't exist → entire WP-04 and WP-06 built on phantom API
4. **WP-04/H4:** `clw hydrate --json` doesn't return `chunks_total` → WP silently returned zeros
5. **WP-04/B:** `clw` exit code 2 means "any error", not "not found" → WP treated all errors as first-run
6. **WP-05/B:** WebSocket hibernation message pipe was fundamentally broken → after DO eviction+resume, all messages dropped silently
7. **WP-06/B:** `Container.alarm()` re-arms to `Date.now()` immediately, making the 30s cadence impossible
8. **WP-07/B:** Migration number `0090_runner_devenv_usage.sql` collides with existing `0090_dsr_tickets.sql`
9. **WP-07/B:** `cache-tier ≠ runner-tier` conflation → wrong ownership, wrong pricing
10. **WP-08/B:** Parallel `itty-router` that trusted a client-supplied `x-corelink-tenant-id` header → tenancy escape (bypasses all auth)
11. **WP-08/B:** OpenAPI spec structurally invalid (`responses` at document root instead of `components.responses`)
12. **WP-09/B:** All code blocks imported `@tanstack/react-query`, `lucide-react`, `@/components/ui/button` — none of these exist in the repo
13. **WP-10/B:** `docker kill -9` is unexecutable in CF microVM → dogfood test would always fail
14. **WP-10/B:** `seedTenantEntitlements` does NOT seed `runners_entitlement` (function's own doc-comment says so)
15. **WP-10/B:** `apps/signup-worker/scripts/d1.py` does not exist
16. **WP-10/B:** Chaos math (5 + 8 ≠ 8) → GA-GATE-O10 broken

---

## 15. Final Notes

**This is a campaign that will take weeks to implement correctly.** The WPs are detailed but full of errors. Your job is to find those errors, fix them, and produce a set of WPs that an engineer can implement without further questions.

**Be rigorous. Be skeptical. Assume nothing. Verify everything.**

Good luck.
