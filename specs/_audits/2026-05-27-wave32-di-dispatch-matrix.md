---
id: "DISPATCH-MATRIX-WAVE32-DI-2026-05-27"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["dispatch", "wave-32", "prod-deploy", "phase-D-I", "parallel", "sota"]
references:
  - "specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md"
  - "specs/_audits/2026-05-27-w32-phaseB-worker-shim-seal.md"
  - "specs/_audits/2026-05-27-w32-phaseC-provisioning-seal.md"
  - "docs/internal/secrets-checklist.md"
  - "scripts/pre-cutover-weekly-verify.sh"
  - "scripts/ga-cutover-prod-dressrun.sh"
---

# Wave 32 Phase D-I — 10-agent dispatch matrix

> **Mandate (2026-05-27, user):** *"divida wave 32 em 10 wps independentes
> separados por contexto para que 10 agents possam trabalhar na wave 32
> em paralelo, cada um com sua worktree propria"* + *"nao esqueca de sua
> skill de techlead"*.
>
> **Status today (2026-05-27 evening):** Phases A + B + C SEALed (worker
> shim 70.14% coverage + 5/5 adversarial closed via WP-1.1;
> CF provisioning HIGH-2 fixed via WP-1.2). Phases D-I remain.

## §0 Critical reframe: Wave 32 D-I is NOT pure-agent territory

Per `2026-05-22-wave32-prod-deploy-spec.md` §5 Dispatch model:

```
Fase D (Migrations + secrets):     orchestrator-direct, irreversible, NO agent autonomy
Fase E (Container deploy):         orchestrator-direct with Owner go-ahead per sub-step
Fases F, G, H (Pages, DNS, smoke): parallel Sonnet agents
Fase I (Sign-off + tag):           orchestrator-direct (audit + tag)
```

**Owner-bound execution** (cannot be agent-dispatched):
- D-day: `wrangler d1 migrations apply --remote` (irreversible)
- D-day: `wrangler secret put` (real secret values, never in repo)
- E-day: `docker push` to CF Containers (paid plan; irreversible)
- I-day: `git tag` + DEBT register update (governance)

**What 10 agents CAN safely do in parallel** (the user's mandate):
- **Author** every script, runbook, harness, smoke pack, audit doc
  skeleton that D-day / E-day / I-day will execute against
- **Validate** every input (migrations additive, secrets matrix
  alignment, Dockerfile correctness, DNS routes complete)
- **Dry-run** every API call with `--dry-run` default
- **Pre-stage** rollback ladders, canary plans, smoke gates

Owner's D-day work post-this-batch = run the pre-authored scripts +
review the pre-authored audit doc + sign + tag. ≤4-6h.

This matrix decomposes Wave 32 D-I into **10 conflict-disjoint
parallel-safe WPs** authored by 10 isolated Sonnet agents. The output is
**executable D-day** — the Owner runs scripts, reviews audits, signs.

---

## §1 Master constraints (inherited)

All `§0.1` pre-flight, `§0.2-§0.5` charter, `§0.6` commit policy,
`§0.7` SEAL report format from
`specs/_audits/2026-05-27-15-agent-dispatch-matrix.md` apply verbatim.

**Plus Wave 32-specific constraints:**

### 0.W1 Dry-run default
Every script an agent writes MUST default to `--dry-run`. Live execution
requires explicit `--live` (or `--apply`) flag. Owner runs `--live` on
D-day; agent never invokes live.

### 0.W2 Idempotency
Every script an agent writes MUST be idempotent (re-runnable). Use
name-match-then-PATCH pattern OR `if [[ -z "$(...)" ]]; then ...; fi`
guards. Reference: `scripts/statuspage-bootstrap.sh` (Phase A precedent).

### 0.W3 No live API calls from agents
Agents may inspect repo state freely. They MUST NOT make any HTTP
request to api.cloudflare.com, api.stripe.com, api.betterstack.com, etc.
Validation is repo-state-only.

### 0.W4 Hard pause triggers (Wave 32 §7)
1. CF Containers beta surfaces architectural blocker → HALT
2. Container image size > CF Containers limit → HALT, file ADR
3. Worker shim ↔ gRPC impedance → HALT, file ADR
4. Migrations not strictly additive → HALT, escalate to Owner

If ANY agent encounters one of these, it MUST stop + file an audit doc
noting the trigger + leave the repo clean.

---

## §2 WP catalog (10 WPs × 6 contexts)

```
CONTEXT D — Migrations + secrets D-day prep (2 agents)
  WP-D.1  Migration additive-safety harness + D-day apply script + audit
  WP-D.2  Secrets-checklist matrix verification + D-day rotation script

CONTEXT E — Container deploy prep (2 agents)
  WP-E.1  Dockerfile audit + image size + layer safety + scan harness
  WP-E.2  Container canary runbook (5%→25%→100%) + rollback ladder + promote script

CONTEXT F — Pages deploys (2 agents)
  WP-F.1  docs Pages deploy script (corelink-docs.humangr.com) + smoke
  WP-F.2  admin-ui Pages deploy script (corelink-app.humangr.com) + smoke

CONTEXT G — DNS production (2 agents)
  WP-G.1  DNS apply script + wrangler.toml route additions (app/docs/get)
  WP-G.2  DNS verify harness (dig + HTTPS + cert-chain assertion)

CONTEXT H — Smoke + cutover (1 agent)
  WP-H.1  Wave-32 smoke extension to pre-cutover-weekly + cutover gate pack

CONTEXT I — Sign-off + tag (1 agent)
  WP-I.1  Closure audit doc skeleton + DEBT register update plan + tag annotation
```

### Conflict matrix (verified disjoint)

```
                  wrangler.toml  Dockerfile  migrations/  scripts/        specs/_audits/  apps/docs/  apps/admin-ui/
                                            d1/                          
WP-D.1                          (read-only)             █new-D-script   █new-audit                 
WP-D.2                                                  █new-D-script   █new-audit                 
WP-E.1            (read-only)   ░audit                  █new-scan       █new-audit                 
WP-E.2                                                  █new-promote    █new-runbook
                                                                        + new-audit                
WP-F.1                                                  █new-F-docs     █new-audit      (read-only)
WP-F.2                                                  █new-F-app      █new-audit                 (read-only)
WP-G.1            █add-routes                           █new-G-apply    █new-audit                 
WP-G.2                                                  █new-G-verify   █new-audit                 
WP-H.1                                                  █new-wave32     █new-audit                 
                                                          extension                                
WP-I.1                                                                  █new-closure
                                                                        skeleton

█ = primary edit zone (unique file per WP)   ░ = small audit-only edits
```

**Hot zones (multi-WP) and safety:**

- `scripts/`: each WP creates UNIQUE files. Filenames pre-defined in
  per-WP contracts. Zero overlap.
- `specs/_audits/`: each WP creates `2026-05-27-w32-<wp-slug>-seal.md`
  (unique). Zero overlap.
- `wrangler.toml`: ONLY WP-G.1 touches (adds 3 route blocks for
  app/docs/get). No other agent edits this file.
- `Dockerfile`: WP-E.1 may add labels/comments (minor). No other agent
  edits.

---

## §3 Per-WP contracts (10)

### WP-D.1 — Migration additive-safety harness + D-day apply script

**CONTEXT:** Wave 32 §Phase D requires `wrangler d1 migrations apply
corelink-prod-d1 --remote` against the 52 D1 migrations. Spec invariant:
migrations are STRICTLY ADDITIVE (no DROP/RENAME/ALTER-DROP). Agent
audits the 52 migrations + authors the D-day apply script.

**SCOPE (may modify):**
- NEW: `scripts/d-day-migrations-apply-prod.sh`
- NEW: `scripts/d-day-migrations-additive-audit.sh`
  (extends/wraps existing `scripts/check_migrations_additive.py` if it
  exists — verify first)
- NEW: `specs/_audits/2026-05-27-w32-phaseD-migrations-prep-seal.md`

**NON-SCOPE:**
- ANY migration file under `migrations/d1/` (no edits — only audit)
- ANY live CF API call
- Phase E/F/G/H/I work

**INPUTS (pre-computed by orchestrator):**
- 52 migrations at `migrations/d1/` from `0001_blob_meta.sql` to
  numbered tail. Exact count verified: `52`.
- Existing additive-checker: `scripts/check_migrations_additive.py`
  (check existence first; if absent, author one).
- Spec verbatim (`2026-05-22-wave32-prod-deploy-spec.md:149`):
  ```
  Apply 52 D1 migrations to corelink-prod-d1 via
    `wrangler d1 migrations apply corelink-prod-d1 --remote`
  ```
- Spec invariant: additive-only (no rollback path — instead, delete
  + re-provision D1 from Phase C if catastrophe).

**DOD:**
1. Audit reports for ALL 52 migrations: classification per-row (ADD
   TABLE | ADD COLUMN | ADD INDEX | ADD VIEW | OTHER) with line cite
   per migration file
2. Zero migrations classified `OTHER` (or each `OTHER` carries an
   inline justification + Owner-acknowledgement note in the audit doc)
3. `scripts/d-day-migrations-apply-prod.sh --dry-run` exits 0 + prints
   the full migration sequence + the EXACT wrangler command Owner will run
4. `scripts/d-day-migrations-apply-prod.sh --help` shows: `--dry-run`,
   `--live`, `--validate-token`, `--apply` flags
5. `shellcheck scripts/d-day-migrations-apply-prod.sh` exits 0
6. Audit doc cites every migration file with classification + expected
   row count after apply
7. Single commit on worktree

**ACCEPTANCE GATES:**
```bash
shellcheck scripts/d-day-migrations-apply-prod.sh scripts/d-day-migrations-additive-audit.sh
bash scripts/d-day-migrations-apply-prod.sh --dry-run 2>&1 | head -30
bash scripts/d-day-migrations-additive-audit.sh 2>&1 | tail -10
```

**COMMIT MSG TEMPLATE:**
```
prep(w32-phaseD): D-day migration apply script + 52-migration additive audit

Wave 32 Phase D pre-stage. 52 D1 migrations classified strictly additive.

- d-day-migrations-apply-prod.sh: --dry-run default; --live requires
  CF API token + explicit flag; sequential apply with halt-on-error.
- d-day-migrations-additive-audit.sh: scans all 52 for non-additive
  patterns (DROP TABLE/COLUMN, ALTER ... DROP, RENAME); exits 1 on hit.
- Per-migration classification in audit doc with line citations.

Owner D-day path: review audit → run --live → confirm post-state.

Audit: specs/_audits/2026-05-27-w32-phaseD-migrations-prep-seal.md.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

### WP-D.2 — Secrets-checklist matrix verification + D-day rotation script

**CONTEXT:** Wave 32 §Phase D requires `wrangler secret put` for
`STRIPE_SECRET_KEY`, `PAGERDUTY_ROUTING_KEY`, `BETTERSTACK_API_TOKEN`,
`STRIPE_WEBHOOK_SECRET` (after Phase E creates the endpoint), and HMAC
keys generated fresh via `openssl rand -hex 32`. Agent verifies the
matrix at `docs/internal/secrets-checklist.md` is consistent with code
consumers and authors the D-day rotation script.

**SCOPE:**
- NEW: `scripts/d-day-secrets-rotate-prod.sh`
- NEW: `specs/_audits/2026-05-27-w32-phaseD-secrets-prep-seal.md`
- READ-ONLY: `docs/internal/secrets-checklist.md` (257 LOC),
  `scripts/secrets-checklist-verify.sh` (existing verifier),
  `scripts/validate_secrets_matrix.py` (existing daily-cron)

**NON-SCOPE:**
- Real secret values (NEVER)
- Modification of the matrix itself (only verification + tooling)
- Live API calls

**INPUTS (pre-computed):**
- Secrets-checklist matrix exists with these critical rows for D-day:
  - `STRIPE_SECRET_KEY` (90d rotation, SRE Lead)
  - `STRIPE_WEBHOOK_SECRET` (90d, SRE Lead — created post-Phase-E)
  - `CLERK_SECRET_KEY` (90d, DevOps)
  - `PAGERDUTY_ROUTING_KEY` (per matrix)
  - `BETTERSTACK_API_TOKEN` (per Phase A SEAL)
  - HMAC keys via `openssl rand -hex 32` (per Phase D spec)
- Existing verifier: `scripts/secrets-checklist-verify.sh` is wired to
  `.github/workflows/cf-deploy-prod.yml` as deploy gate
- Existing daily-cron: `scripts/validate_secrets_matrix.py` produces
  `secrets-drift-report.json` (90d retention for SOC 2 CC6.1)

**DOD:**
1. Script `--dry-run` mode prints the EXACT 6+ `wrangler secret put`
   commands Owner will execute (placeholder for value; never real)
2. Script handles HMAC-generation safely (`openssl rand -hex 32`
   inline, piped, never written to disk)
3. Script `--validate-matrix` mode re-runs both existing verifiers
4. Audit doc has Owner D-day checklist (numbered steps with expected
   output per step) — Owner copy-pastes the commands one-by-one
5. Cross-references the existing matrix row-by-row; flags any missing
   row a Phase D-day will need
6. `shellcheck` clean
7. Single commit on worktree

**ACCEPTANCE GATES:**
```bash
shellcheck scripts/d-day-secrets-rotate-prod.sh
bash scripts/d-day-secrets-rotate-prod.sh --dry-run 2>&1 | head -30
bash scripts/d-day-secrets-rotate-prod.sh --validate-matrix 2>&1 | tail -10
```

---

### WP-E.1 — Dockerfile audit + image size + layer safety + scan harness

**CONTEXT:** Wave 32 §Phase E requires `docker build` + push to
Cloudflare Containers. Dockerfile (91 LOC) is Wave-33-Stage-2-hardened
(uses workspace + builds `corelink-server` specifically). Agent audits
Dockerfile for safety (no secrets baked, multi-stage minimal, no shell
trickery) + authors a pre-push scan harness.

**SCOPE:**
- READ-ONLY (audit only): `Dockerfile`, `.dockerignore` (if exists)
- NEW: `scripts/e-day-container-pre-push-scan.sh`
- NEW: `specs/_audits/2026-05-27-w32-phaseE-container-audit-seal.md`
- MAY add `.dockerignore` if missing OR add LABELs to Dockerfile
  (additive only, no logic change)

**NON-SCOPE:**
- Container image push (Owner-bound)
- Cloudflare Containers config beyond what's in `[[containers]]` block
  of `wrangler.toml`
- Rust source changes

**INPUTS (pre-computed):**
- Dockerfile: 91 LOC, multi-stage (`FROM rust:1.91-slim-bookworm AS builder`),
  workspace-aware, builds `corelink-server` package
- Header comment cites Wave-33 Stage 2.B.2 fix of pre-existing
  broken-since-`50c92f40` bug
- CF Containers beta limits (research current): image size cap typically
  ~512 MB; layer count cap; binary entrypoint required
- Spec gate (§Phase E):
  - `docker run` smoke locally before push
  - 5% canary first; verify 10 minutes; ramp to 100%
  - Rollback via `wrangler rollback`

**DOD:**
1. Pre-push scan script checks: image size (warn >256MB, error >512MB);
   layer count (warn >50, error >100); zero shell scripts in final
   image (read-only check via `docker image history`); zero secrets
   detected via `grep -i 'PRIVATE KEY\|BEGIN.*KEY\|password=\|token='`
2. Script `--dry-run` runs the local `docker build` and reports size/
   layers WITHOUT pushing
3. `.dockerignore` exists OR audit doc explains why it's not needed
4. Audit doc cites each Dockerfile section with rationale + lists 3
   hardening opportunities (LOC + suggested change) — these are NOT
   applied; orchestrator/Owner decides
5. `shellcheck` clean
6. Single commit

**ACCEPTANCE GATES:**
```bash
shellcheck scripts/e-day-container-pre-push-scan.sh
docker --version 2>&1 | head -1  # report availability
bash scripts/e-day-container-pre-push-scan.sh --dry-run 2>&1 | head -20
```

---

### WP-E.2 — Container canary runbook (5%→25%→100%) + rollback ladder + promote script

**CONTEXT:** Wave 32 §Phase E spec mandates gradual deploy: 5% canary
first; verify clean for 10 min; ramp to 100%. Workers Paid plan ≥$5/mo
required. Author the canary runbook + promotion automation.

**SCOPE:**
- NEW: `specs/_runbooks/RB-W32-CONTAINER-CANARY.md`
- NEW: `scripts/e-day-container-canary-promote.sh`
- NEW: `specs/_audits/2026-05-27-w32-phaseE-canary-prep-seal.md`

**NON-SCOPE:**
- Actual deployment
- Dockerfile changes (WP-E.1 owns)

**INPUTS:**
- Canary stages: 5% → wait 10 min observing metrics → 25% → 10 min → 100%
- Observability sources to check during canary:
  - CF Workers analytics: error rate, p95 latency
  - Grafana DASH-SLO-API + DASH-SLO-AUDIT (landed today via WP-6.1)
  - BetterStack synthetic probes (landed today via WP-7.1)
  - Sentry error rate (no-op until DSN set; deferred-aware)
- Rollback ladder:
  - At 5%: `wrangler rollback` (instant; previous deploy revives)
  - At 25%: same
  - At 100%: same (DO state survives `wrangler rollback`; if DO state
    corrupted, `wrangler delete --force` purges — DATA LOSS warning)
- Promote criteria (each stage):
  - Error rate within +0.5pp of baseline
  - p95 latency within +20ms of baseline
  - Zero CRITICAL Sentry errors
  - Zero BetterStack probe failures

**DOD:**
1. Runbook is one-pass scannable, includes:
   - Pre-deploy checklist (Phase D done, Phase F+G ready, etc.)
   - Per-stage: command to promote, what to monitor for 10 min,
     PASS/FAIL criteria, rollback command
   - Post-promote sign-off checklist
2. Promote script `--dry-run` lists every stage + the wrangler command
   for each (Owner reviews + invokes manually OR uses `--apply` to
   sequence them with built-in 10-min waits)
3. Runbook cites Grafana dashboard URLs + BetterStack page ID +
   Sentry org (links the existing observability surfaces)
4. Audit doc lists the 4 PASS/FAIL criteria with verbose data-source
   reference per criterion
5. `shellcheck` clean
6. Single commit

---

### WP-F.1 — docs Pages deploy script (corelink-docs.humangr.com) + smoke

**CONTEXT:** Wave 32 §Phase F deploys `apps/docs/build` to CF Pages
project `corelink-docs` at custom domain `corelink-docs.humangr.com`
(flat-name pattern adapted from spec's `docs.corelink.humangr.com` per
Wave 32 Phase G adaptation already landed). The docs site already has
EN locale building green (per Drift-B agent's outcome expected); fix
non-EN locales is Drift-B's territory (parallel agent).

**SCOPE:**
- NEW: `scripts/f-day-deploy-pages-docs.sh`
- NEW: `scripts/f-day-smoke-docs.sh`
- NEW: `specs/_audits/2026-05-27-w32-phaseF1-docs-pages-prep-seal.md`

**NON-SCOPE:**
- The docs build itself (`apps/docs/`)
- Non-docs Pages (WP-F.2 owns admin-ui)
- DNS apply (WP-G.1 owns)

**INPUTS:**
- Pages project name: `corelink-docs`
- Custom domain target: `corelink-docs.humangr.com`
- Build command: `cd apps/docs && pnpm build` (currently EN-only safe;
  full 4-locale gated by Drift-B fix)
- Deploy command:
  `wrangler pages deploy apps/docs/build --project-name corelink-docs --env prod`
- Smoke targets (post-deploy):
  - `https://corelink-docs.humangr.com/` → 200 + Docusaurus index
  - `https://corelink-docs.humangr.com/blog` → 200 + blog index (post-WP-4.1/4.2)
  - `https://corelink-docs.humangr.com/compare/vs-buildbuddy` → 200
  - `https://corelink-docs.humangr.com/legal/sub-processors` → 200
- BetterStack probe target already defined (WP-7.1) — cross-reference

**DOD:**
1. Deploy script `--dry-run` prints the wrangler command + build hash
   that would deploy
2. Smoke script verifies all 4 critical URLs return 200 + content
   assertion (HTML present, no 5xx body)
3. Scripts handle `EN-only` fallback gracefully (if Drift-B fix lands
   first, script auto-detects 4 locales; else falls back to EN)
4. `shellcheck` clean
5. Single commit

---

### WP-F.2 — admin-ui Pages deploy script (corelink-app.humangr.com) + smoke

**CONTEXT:** Wave 32 §Phase F deploys `apps/admin-ui/dist` (or `.next`
for Next.js) to CF Pages project `corelink-admin-ui` at
`corelink-app.humangr.com` (flat-name adapted). Phase 0.J fix landed
(Clerk SDK nodejs runtime).

**SCOPE:**
- NEW: `scripts/f-day-deploy-pages-admin.sh`
- NEW: `scripts/f-day-smoke-admin.sh`
- NEW: `specs/_audits/2026-05-27-w32-phaseF2-admin-pages-prep-seal.md`

**NON-SCOPE:**
- admin-ui source
- WP-F.1 docs work
- DNS

**INPUTS:**
- Pages project name: `corelink-admin-ui`
- Custom domain: `corelink-app.humangr.com`
- Build command: `cd apps/admin-ui && pnpm build`
- Deploy:
  `wrangler pages deploy apps/admin-ui/.next --project-name corelink-admin-ui --env prod`
- Smoke targets:
  - `https://corelink-app.humangr.com/` → 200 (admin-ui shell)
  - `https://corelink-app.humangr.com/sign-up` → 200 (Clerk widget renders)
  - `https://corelink-app.humangr.com/sign-in` → 200
  - `https://corelink-app.humangr.com/en/welcome` → 200 (post-signup
    SSE pane — gated on actual signup; smoke checks page exists)
- Required Pages env secrets (pre-D-day):
  CLERK_PUBLISHABLE_KEY, CLERK_SECRET_KEY, STRIPE_SECRET_KEY,
  RESEND_API_KEY, RESEND_NEWSLETTER_AUDIENCE_ID (post-D-day),
  SENTRY_DSN (deferred — no-op until set)

**DOD:**
1. Deploy script `--dry-run` lists exact wrangler command + build hash
2. Smoke script tests 4 URLs with assertions
3. Pre-deploy checklist enumerates required secrets (cross-ref WP-D.2)
4. `shellcheck` clean
5. Single commit

---

### WP-G.1 — DNS apply script + wrangler.toml route additions (app/docs/get)

**CONTEXT:** Wave 32 §Phase G adds CNAME records for all subdomains.
Currently `wrangler.toml [env.prod.routes]` declares only `api`,
`signup`, `admin` (lines 240/244/248). Missing: `app`, `docs`, `get`
(despite being target hostnames in Phase F + WP-7.1 + WP-7.3 work).

**SCOPE:**
- `wrangler.toml` (ADD 3 `[[env.prod.routes]]` blocks)
- NEW: `scripts/g-day-dns-apply-prod.sh`
- NEW: `specs/_audits/2026-05-27-w32-phaseG1-dns-apply-prep-seal.md`

**NON-SCOPE:**
- Any wrangler.toml change beyond the 3 route additions
- Real DNS API calls (Owner D-day only)

**INPUTS:**
- Existing route declarations (verified by orchestrator):
  ```toml
  [[env.prod.routes]]
  pattern = "corelink-api.humangr.com/*"
  zone_name = "humangr.com"

  [[env.prod.routes]]
  pattern = "corelink-signup.humangr.com/*"
  zone_name = "humangr.com"

  [[env.prod.routes]]
  pattern = "corelink-admin.humangr.com/*"
  zone_name = "humangr.com"
  ```
- New routes to add (3):
  ```toml
  [[env.prod.routes]]
  pattern = "corelink-app.humangr.com/*"
  zone_name = "humangr.com"

  [[env.prod.routes]]
  pattern = "corelink-docs.humangr.com/*"
  zone_name = "humangr.com"

  [[env.prod.routes]]
  pattern = "corelink-get.humangr.com/*"
  zone_name = "humangr.com"
  ```
- Total subdomains post-add: 6 flat-name (api/app/docs/signup/admin/get)
  + `status.corelink.humangr.com` (Phase A SEALed, separate path)
- CF API: `POST /zones/{zone_id}/dns_records` with type=CNAME (proxied
  orange-cloud for Worker-backed; DNS-only for status)
- Spec §G says all proxied EXCEPT `status` — Phase A SEAL doc shows
  status was DNS-only via CNAME to `hugrl.betteruptime.com`

**DOD:**
1. `wrangler.toml` has 6 `[[env.prod.routes]]` blocks (3 added)
2. DNS apply script `--dry-run` lists all 6 CNAMEs it would create
   with full JSON body per record
3. Script handles re-run idempotency (lookup-by-name then PATCH)
4. `shellcheck` clean
5. Audit doc tabulates: subdomain → CF Worker route → proxied flag →
   TLS cert path (Universal SSL Free covers `*.humangr.com`)
6. Single commit

**ACCEPTANCE GATES:**
```bash
grep -c "corelink-.*\.humangr\.com/\*" wrangler.toml  # expect 6
shellcheck scripts/g-day-dns-apply-prod.sh
bash scripts/g-day-dns-apply-prod.sh --dry-run 2>&1 | head -30
```

---

### WP-G.2 — DNS verify harness (dig + HTTPS + cert-chain assertion)

**CONTEXT:** Wave 32 §Phase G post-apply verification: each subdomain
must resolve to CF IPs + serve valid HTTPS cert from chain. Cross-checks
with BetterStack probes (WP-7.1) at the synthetic-monitoring layer.

**SCOPE:**
- NEW: `scripts/g-day-dns-verify-prod.sh`
- NEW: `specs/_audits/2026-05-27-w32-phaseG2-dns-verify-prep-seal.md`

**NON-SCOPE:**
- DNS apply (WP-G.1)
- Any wrangler.toml change

**INPUTS:**
- 6 flat-name subdomains: api, app, docs, signup, admin, get
- Plus status.corelink.humangr.com (existing, BetterStack-routed)
- Per-host verification:
  - `dig +short <host>` returns CF IPs (104.x or 172.67.x range)
  - `curl -sI https://<host>` returns HTTPS/2 200 or 30x
  - Cert chain via `openssl s_client -connect <host>:443 -servername <host>`
    chains to a CF-issued cert (Cloudflare Inc ECC CA-3 or similar)
- Universal SSL Free covers `*.humangr.com` — 3-level subdomains
  (e.g., `*.corelink.humangr.com`) NOT covered (Wave 32 Phase H lesson)

**DOD:**
1. Script tests all 7 hosts (6 flat + status) with 3 checks each
2. Script `--mode=quick` runs only dig (no TLS handshake — fast); default
   mode runs all 3 checks
3. Exit 0 only if all 7 × 3 = 21 checks pass; exit 1 with summary table
   if any fail
4. `shellcheck` clean
5. Audit doc tabulates expected output per host (sample dig + curl
   output for documentation)
6. Single commit

---

### WP-H.1 — Wave-32 smoke extension to pre-cutover-weekly + cutover gate pack

**CONTEXT:** Existing `scripts/pre-cutover-weekly-verify.sh` (602 LOC)
covers 8 DEFER items from pre-Wave-32 state. Wave 32 deploy adds new
surfaces (worker shim live, container, DNS, Pages). Extend (don't edit)
the smoke pack to cover the Wave-32 additions.

**SCOPE:**
- NEW: `scripts/pre-cutover-wave32-extension.sh` (sourced/called by the
  weekly verifier OR runnable standalone)
- NEW: `scripts/h-day-cutover-gate-pack.sh` (post-deploy smoke +
  cutover gate)
- NEW: `specs/_audits/2026-05-27-w32-phaseH-smoke-prep-seal.md`

**NON-SCOPE:**
- Editing `pre-cutover-weekly-verify.sh` directly (additive separate
  file only)
- Editing `ga-cutover-prod-dressrun.sh` (existing 743 LOC; cite/reuse only)

**INPUTS:**
- Existing smoke harness covers 8 DEFER items (LFPDPPP, FW roles,
  pentest, AWS Artifact, statuspage, pilot signups, retest, owner sign-off)
- Wave-32 additions to smoke:
  - Worker shim live (`/health` returns 200 from `corelink-api.humangr.com`)
  - Container running (DO instantiation + container start → grpc :50051)
  - 7 subdomains resolve + serve HTTPS (cross-ref WP-G.2)
  - 6 Pages projects deployed (cross-ref WP-F.1/F.2)
  - Migrations applied (count tables; cross-ref WP-D.1)
  - Secrets bound (cross-ref WP-D.2)
  - BetterStack probes green (cross-ref WP-7.1)
  - Sentry receives test event (deferred — no-op until DSN set; smoke
    script flags as "deferred" not error)
- `scripts/ga-cutover-prod-dressrun.sh` (743 LOC) is the dressrun
  baseline — cite it, don't re-write it

**DOD:**
1. Extension script tests 8 Wave-32 surfaces with PASS/FAIL per row
2. Cutover gate pack collects: weekly digest + extension + dressrun
   exit codes; produces a single GREEN/YELLOW/RED verdict
3. Verdict thresholds:
   - GREEN: all 8 PASS, dressrun exit 0
   - YELLOW: 1-2 PASS-WITH-NOTES (e.g., Sentry deferred); proceed with
     Owner ack
   - RED: any FAIL; halt cutover
4. `shellcheck` clean
5. Audit doc tabulates each of the 8 Wave-32 surfaces with verification
   command + expected output
6. Single commit

---

### WP-I.1 — Closure audit doc skeleton + DEBT register update + tag annotation

**CONTEXT:** Wave 32 §Phase I — comprehensive closure audit at
`specs/_audits/2026-05-22-w32-closure.md`. Pre-author the skeleton so
Owner only fills the D-day actuals + signs. Also stages the DEBT
register update + tag annotation template.

**SCOPE:**
- NEW: `specs/_audits/2026-05-22-w32-closure.md` (skeleton with
  `[FILL-D-DAY]` placeholders Owner replaces)
- NEW: `specs/_audits/2026-05-27-w32-phaseI-closure-prep-seal.md`
- READ-ONLY: existing DEBT register (find path via
  `find specs -iname "DEBT-REGISTER*" 2>/dev/null | head`)

**NON-SCOPE:**
- Editing DEBT register directly (only prepare the update text in audit
  doc); Owner applies on I-day
- `git tag` (Owner-bound; cannot agent-dispatch)
- Any Wave 32 phase-specific work other than closure

**INPUTS:**
- Spec §Phase I deliverables:
  - Comprehensive closure audit at `specs/_audits/2026-05-22-w32-closure.md`
  - DEBT-016 (Statuspage) flipped to CLOSED
  - DEBT-027 (Pilot signups) status uplifted to `signup-infra-deployed`
  - New top-of-doc entry in DEBT register:
    `> **2026-05-22 update (v1.5.0):** Wave 32 prod deploy SEALed;
    status/api/app/docs/signup/admin live on *.corelink.humangr.com`
  - `git tag corelink-prod-deploy-v1` + tag annotation
- Memory update: `corelink_prod_deploy_sealed_20260527.md`
- L9 risk-assessment 7-questions from techlead skill §L9 (mandatory in
  tag annotation)

**DOD:**
1. Closure skeleton has sections:
   - Wave 32 summary (Phase A-I status)
   - Deployment topology achieved (6 flat-name subdomains + status)
   - L9 7-question risk register (placeholder for Owner answers)
   - DEBT updates (3 rows pre-drafted)
   - Sign-off table (Owner signature placeholder)
2. Phase I prep audit doc lists every `[FILL-D-DAY]` placeholder Owner
   must complete
3. Tag annotation template includes:
   - Full Wave 32 phase chain (A → I)
   - L9 risk-assessment 7 answers (placeholders)
   - Pointers to per-phase SEAL audit docs
   - Cumulative LOC + test count delta
4. validate_specs.py green (placeholder text must not break schema)
5. Single commit

**ACCEPTANCE GATES:**
```bash
python3 scripts/validate_specs.py 2>&1 | tail -3
grep -c "FILL-D-DAY" specs/_audits/2026-05-22-w32-closure.md  # expect ≥10
```

---

## §4 Dispatch model

All 10 in a SINGLE parallel batch. Each gets `isolation: "worktree"`,
`model: "sonnet"`, `run_in_background: true`.

**Per skill §0.6 cap:** Owner explicitly authorized 10 (mandate above).
Above default 6-cap. No further override needed.

## §5 Merge order

Per techlead skill: merge by ARRIVAL time. Each merge runs L0-L7
verdict. Conflict matrix §1 guarantees no two agents share a primary
file zone.

The ONLY shared-edit file is `wrangler.toml` (WP-G.1 only). No conflict
because no other agent edits it.

## §6 Hard pause triggers (orchestrator-bound)

Stop dispatching / merging if:
- ≥3 agents return identical blocker (systemic — investigate)
- ANY agent triggers Wave 32 §7 hard pause (CF Containers blocker,
  image size, gRPC impedance, non-additive migration)
- `validate_specs.py` regresses below 459 (NEVER allow)
- Disk free < 5 GB

## §7 What's left AFTER this batch (Owner D-day work)

- **D-day (Owner-direct, ~2-3h):**
  - Run `scripts/d-day-migrations-additive-audit.sh` (read-only verify)
  - Run `scripts/d-day-migrations-apply-prod.sh --live` (apply 52)
  - Run `scripts/d-day-secrets-rotate-prod.sh --live --interactive`
    (paste each secret value when prompted)
- **E-day (Owner-direct, ~3-5h):**
  - Run `scripts/e-day-container-pre-push-scan.sh` (size+layer check)
  - `docker push` to CF Containers
  - Follow `RB-W32-CONTAINER-CANARY.md` (5%→25%→100%)
- **F-day (Owner-direct, ~1-2h):**
  - Run `scripts/f-day-deploy-pages-docs.sh --live`
  - Run `scripts/f-day-deploy-pages-admin.sh --live`
  - Smoke both via `f-day-smoke-*.sh`
- **G-day (Owner-direct, ~1h):**
  - Run `scripts/g-day-dns-apply-prod.sh --live`
  - Run `scripts/g-day-dns-verify-prod.sh`
- **H-day (Owner-direct, ~2-3h):**
  - Run `scripts/h-day-cutover-gate-pack.sh`
  - Resolve any YELLOW/RED before proceeding
- **I-day (Owner-direct, ~1-2h):**
  - Fill `[FILL-D-DAY]` placeholders in closure audit
  - Apply DEBT register updates
  - `git tag corelink-prod-deploy-v1 -m "<annotation>"`
  - `git push origin corelink-prod-deploy-v1`
  - Update memory `corelink_prod_deploy_sealed_20260527.md`

**Total Owner-direct D-I work post-batch:** ~10-16h (across multiple days).

## §8 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of Wave 32 Phase D-I dispatch matrix — PENDING Owner go before dispatch.**
