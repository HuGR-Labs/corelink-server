# Wave 32 Phase H PREP — Smoke + Cutover + Rollback Scripts (2026-05-26)

> **Doc kind:** wave-scope PREP audit (evidence; `_audits/` excluded from canonical schema
> validation).
>
> **Owner:** Gustavo Schneiter (Security + Release Lead).
>
> **Authored:** 2026-05-26 by Claude Sonnet 4.6.
>
> **Trigger:** Phase H PREP — write smoke, cutover, and rollback scripts that Phase H APPLY
> will use after Phase E (container deploy) + Phase F (pages) + Phase G (DNS) land.
>
> **Branch:** assigned by worktree harness.
>
> **Baseline commit:** `9dcbd2a3e6c7ad8ce29b100c07f6886567bcb2e6` (post all Wave 32 Phase
> B+C+D-prep+E-prep+F-prep+G-prep merges).
>
> **Charter compliance:** SOTA bar; CTRL-CRED-001 (no credentials in any script — anonymous
> probes only; authed audit-chain check reads `CORELINK_SMOKE_TOKEN` from env and is skipped
> if absent); cutover-fail-CLOSED (post-cutover smoke failure auto-invokes rollback);
> zero gambiarra; no real cutover applied.

---

## §1 Scope

This PREP phase delivers three operator scripts and this audit doc. No production state is
mutated; Phase H APPLY (the actual cutover execution) requires Phase E complete first.

**Deliverables:**

| Artefact | Path | LOC | Purpose |
|---|---|---|---|
| Smoke script | `scripts/smoke-prod-corelink.sh` | 504 | End-to-end smoke test, all 5 check areas, 22 checks |
| Cutover checklist | `scripts/cutover-checklist-prod.sh` | 519 | Interactive go-live checklist, 15 items, signed markdown output |
| Rollback script | `scripts/rollback-prod-corelink.sh` | 407 | Fast rollback: Worker+DO, Container, DNS, post-rollback smoke |
| This audit | `specs/_audits/sealed/2026-05-26-w32-phaseH-prep.md` | — | PREP SEAL |

**Total script LOC:** 1,430

**What Phase H PREP does NOT do:**
- Execute any wrangler deploy, rollback, or promote commands.
- Create any DNS records.
- Modify any CF infrastructure.
- Apply any cutover (Phase H APPLY is blocked on Phase E completing).

**Parallel-safe with:** Phase B (worker shim), Phase C (CF provision), Phase D prep (migration
scripts), Phase E prep (container scripts), Phase F prep (pages scripts), Phase G prep (DNS
scripts). Zero file overlap confirmed: this prep touches only
`scripts/{smoke-prod-corelink,cutover-checklist-prod,rollback-prod-corelink}.sh` and this
audit doc.

---

## §2 Smoke check inventory

`scripts/smoke-prod-corelink.sh` covers 5 check areas, 22 individual checks:

### Area (a) — Worker + DO + Container (checks 1–4)

| # | Check | Expected |
|---|---|---|
| 1 | `GET /health` | HTTP 200 + `{"status":"ok"}` |
| 2 | `GET /_health` | HTTP 200 + `Content-Type: application/json` |
| 3 | `POST /v2/cas/upload` (REAPI v2) | HTTP 200 + digest `sha256:<hash>` in response |
| 4 | `GET /v2/cas/sha256:<digest>` | HTTP 200 + bytes match exactly |

**Rationale:** Checks 1–2 verify Worker health and Durable Object health endpoint. Checks 3–4
exercise the full REAPI CAS path: Worker receives upload → forwards to DO → container writes
blob to R2 → retrieval round-trips back. A failure here indicates Worker, DO, or Container is
not running or not reachable.

### Area (b) — Pages deploys (checks 5–8)

| # | Check | Expected |
|---|---|---|
| 5 | `GET https://docs.corelink.humangr.com` | HTTP 200 + HTML body |
| 6 | `GET https://app.corelink.humangr.com` | HTTP 200 + Clerk init HTML |
| 7 | TLS chain: `docs.corelink.humangr.com` | Cert subject covers `humangr.com` |
| 8 | TLS chain: `app.corelink.humangr.com` | Cert subject covers `humangr.com` |

**Rationale:** Checks 5–6 confirm both Phase F Pages deploys are live. Checks 7–8 verify
CF Universal SSL issued for `*.corelink.humangr.com` (automatic for proxied records).

### Area (c) — DNS resolution (checks 9–18)

Driven by Phase G plan table (§3 of `2026-05-26-w32-phaseG-prep.md`), 9 plan rows + 1 NO-OP:

| # | Name | Expected target |
|---|---|---|
| 9 | `api.corelink.humangr.com` | resolves toward `corelink-prod.gustavoschneiter.workers.dev` |
| 10 | `app.corelink.humangr.com` | resolves toward `corelink-admin-ui.pages.dev` |
| 11 | `docs.corelink.humangr.com` | resolves toward `corelink-docs.pages.dev` |
| 12 | `signup.corelink.humangr.com` | resolves toward workers.dev |
| 13 | `admin.corelink.humangr.com` | resolves toward workers.dev |
| 14 | `acme-dev.corelink.humangr.com` | resolves toward workers.dev |
| 15 | `staging.corelink.humangr.com` | resolves toward `corelink-staging.gustavoschneiter.workers.dev` |
| 16 | `sandbox.corelink.humangr.com` | resolves toward workers.dev |
| 17 | `go.corelink.humangr.com` | resolves toward workers.dev |
| 18 | `status.corelink.humangr.com` | resolves toward `hugrl.betteruptime.com` (DNS-only, Phase A) |

**Acceptance rule:** Proxied (CF orange-cloud) records resolve to CF anycast IPs, not the
CNAME target directly. The smoke script accepts any non-NXDOMAIN resolution for orange-cloud
records; NXDOMAIN is a FAIL. For `status` (DNS-only), the result must contain `betteruptime`.

### Area (d) — Audit chain end-to-end (checks 19–20)

| # | Check | Expected |
|---|---|---|
| 19 | `POST /v1/audit/probe` with `Authorization: Bearer $CORELINK_SMOKE_TOKEN` | HTTP 200 + `request_id` in response |
| 20 | `GET /v1/audit/chain?request_id=<id>` after 2s wait | HTTP 200 + row present + `chain_hash` is valid hex ≥ 16 chars |

**If `CORELINK_SMOKE_TOKEN` is absent:** checks 19–20 are SKIPPED with WARN (not FAIL).
CTRL-CRED-001: the token is read from environment only, never hard-coded.

### Area (e) — Status page (checks 21–22)

| # | Check | Expected |
|---|---|---|
| 21 | `GET https://status.humangr.com` | HTTP 200 |
| 22 | `GET https://status.corelink.humangr.com` | HTTP 200 (Phase A custom domain) |

**Total: 22 checks.** Exit code = number of failures (0 = all green).

---

## §3 Cutover checklist items

`scripts/cutover-checklist-prod.sh` runs 15 explicit items in order. Each item:
1. Describes the check and command(s) to verify.
2. Accepts operator y/n confirmation.
3. Appends a signed markdown row to the output checklist file.

### Pre-cutover items (8)

| # | Item | Blocking | Command |
|---|---|---|---|
| 1 | Smoke test green | yes | `bash scripts/smoke-prod-corelink.sh` → exit 0 |
| 2 | BetterStack status page reachable + subscribable | yes | `curl -sI https://status.corelink.humangr.com` + manual subscribe check (Phase A) |
| 3 | CF Container metrics — no crash loops (10-min baseline) | yes | CF Dashboard → Containers tab, 0 restarts in 10 min |
| 4 | Error budget unconsumed | yes | SLO dashboard: error rate < 0.1% last 1 hour |
| 5 | Phase D secrets deployed (55 cf-wrangler secrets) | yes | `bash scripts/verify-secrets-deployed.sh` → exit 0 |
| 6 | Phase B Worker shim unit tests passing | yes | `cd worker && pnpm test` → ≥ 70% coverage + all green |
| 7 | 5% canary stable >= 10 minutes | yes | CF Dashboard → Workers → corelink-prod → Traffic Splits |
| 8 | Adversarial review score >= 7.5/10 | yes | Independent Sonnet adversarial review (per spec §7 hard pause #8) |

### Cutover execution items (3)

| # | Item | Blocking | Command |
|---|---|---|---|
| 9 | Ramp canary 5% → 100% | yes (irreversible; rollback available) | `wrangler deployments promote <id> --env prod` |
| 10 | DNS cutover locked in (9 records resolving) | yes | `bash scripts/dns-prod-verify.sh --post-apply` |
| 11 | CF Pages custom domains responding | yes | `curl -sI https://docs.corelink.humangr.com` + app |

### Post-cutover items (3)

| # | Item | Blocking | Consequence if FAIL |
|---|---|---|---|
| 12 | Post-cutover smoke green | yes — auto-rollback triggered on FAIL | `rollback-prod-corelink.sh --auto` invoked |
| 13 | 30-minute metrics window clean | warn only | Logged as warning; operator discretion |
| 14 | Roll-forward version verified | warn only | Deployed vs HEAD mismatch logged |

### Sign-off (1)

| # | Item | Action |
|---|---|---|
| 15 | Operator types `CUTOVER` | Cutover is committed; checklist markdown is finalized |

**Checklist row count: 15.**

Output file: `specs/_audits/2026-05-26-w32-phaseH-cutover-checklist-<timestamp>.md`
(or operator-specified path via `--output`).

---

## §4 Rollback decision tree

`scripts/rollback-prod-corelink.sh` executes 4 steps; the decision tree at each step:

```
Rollback triggered
│
├─ Step 1: wrangler rollback --env prod
│   ├─ OK  → continue to Step 2
│   └─ ERR → HALT (fatal)
│            Manual: wrangler deployments list → promote previous ID
│            Do NOT proceed to Step 2 (Worker may be in unknown state)
│
├─ Step 2: wrangler containers rollback corelink-server:prod
│   ├─ OK  → continue to Step 3
│   └─ ERR → WARN + continue to Step 3
│            Worker is already rolled back (Step 1 succeeded)
│            Container state is degraded but not fatal — DNS rollback still proceeds
│
├─ Step 3: dns-prod-apply.sh --rollback <snapshot>
│   ├─ Snapshot present + apply OK → continue to Step 4
│   ├─ Snapshot missing            → ERR + manual deletion of 9 CNAMEs
│   │   Manual: CF Dashboard → DNS → delete api/app/docs/signup/admin/
│   │           acme-dev/staging/sandbox/go records
│   └─ Apply ERR                   → ERR + manual deletion
│
└─ Step 4: smoke-prod-corelink.sh (post-rollback)
    ├─ All green (exit 0)         → ROLLBACK COMPLETE (exit 0)
    ├─ DNS checks fail            → WARN — expected if Phase G APPLY had not yet run
    └─ Worker /health fails       → WARN + escalate to Owner
                                    System is in degraded state
```

### Trigger conditions for rollback

| Trigger | Who invokes | Mode |
|---|---|---|
| Post-cutover smoke fails (item 12 in cutover checklist) | `cutover-checklist-prod.sh` auto-invokes | `--auto` |
| Operator manually runs rollback at any point during cutover | Operator | Interactive (types `ROLLBACK`) |
| Ops on-call escalation (PagerDuty SEV-0/SEV-1) | Ops on-call | Interactive or `--auto` |

### What rolls back / what does NOT

| Component | Rolls back? | Notes |
|---|---|---|
| Worker (code) | YES | `wrangler rollback` reverts to previous Worker deployment |
| Durable Object (code) | YES | Same deploy package as Worker |
| DO storage (state) | NO | DO storage persists across rollback — by design |
| Container (image) | YES | `wrangler containers rollback` reverts image tag |
| DNS records (9 CNAMEs) | YES | Via Phase G snapshot + `dns-prod-apply.sh --rollback` |
| Phase A status page | NO | `status.corelink.humangr.com` is Phase A, not touched |
| D1 migrations | NO | Additive migrations cannot roll back at DB layer |
| CF Secrets | NO | Secrets remain; rollback does not delete them |
| R2 blobs (CAS store) | NO | Blob data persists; rollback does not delete blobs |

---

## §5 Dry-run output samples

### §5.1 `smoke-prod-corelink.sh --dry-run`

```
smoke-prod-corelink.sh — DRY-RUN (no network calls)
Date: 2026-05-26T19:08:30Z

Check inventory:

(a) Worker + DO + Container
  [1] GET  https://api.corelink.humangr.com/health             → expect 200 {"status":"ok"}
  [2] GET  https://api.corelink.humangr.com/_health            → expect 200 + content-type application/json
  [3] POST https://api.corelink.humangr.com/v2/cas/upload      → REAPI CAS blob upload (tiny blob)
  [4] GET  https://api.corelink.humangr.com/v2/cas/<digest>    → blob fetch, byte-for-byte match

(b) Pages deploys
  [5] GET  https://docs.corelink.humangr.com                    → expect 200 + HTML body
  [6] GET  https://app.corelink.humangr.com                     → expect 200 + Clerk init script tag
  [7] TLS  docs.corelink.humangr.com      → cert subject covers *.humangr.com
  [8] TLS  app.corelink.humangr.com       → cert subject covers *.humangr.com

(c) DNS resolution (Phase G plan — 9 rows + 1 NO-OP)
  [9]  dig api.corelink.humangr.com       → corelink-prod.gustavoschneiter.workers.dev
  [10] dig app.corelink.humangr.com       → corelink-admin-ui.pages.dev
  [11] dig docs.corelink.humangr.com      → corelink-docs.pages.dev
  [12] dig signup.corelink.humangr.com    → corelink-prod.gustavoschneiter.workers.dev
  [13] dig admin.corelink.humangr.com     → corelink-prod.gustavoschneiter.workers.dev
  [14] dig acme-dev.corelink.humangr.com  → corelink-prod.gustavoschneiter.workers.dev
  [15] dig staging.corelink.humangr.com   → corelink-staging.gustavoschneiter.workers.dev
  [16] dig sandbox.corelink.humangr.com   → corelink-prod.gustavoschneiter.workers.dev
  [17] dig go.corelink.humangr.com        → corelink-prod.gustavoschneiter.workers.dev
  [18] dig status.corelink.humangr.com    → hugrl.betteruptime.com (NO-OP — Phase A)

(d) Audit chain end-to-end
  [19] POST https://api.corelink.humangr.com/v1/audit/probe    → authed request (CORELINK_SMOKE_TOKEN)
  [20] wait 2s then GET  https://api.corelink.humangr.com/v1/audit/chain?request_id=<id>
       → row present + chain_hash valid (non-empty hex string)

(e) Status page
  [21] GET  https://status.humangr.com                 → expect 200
  [22] GET  https://status.corelink.humangr.com        → expect 200 (Phase A custom domain)

Total checks: 22
Exit code will equal number of failures.
```

### §5.2 `rollback-prod-corelink.sh --dry-run`

```
rollback-prod-corelink.sh — DRY-RUN

Rollback plan:

Step 1: Worker + DO rollback
  Command: wrangler rollback --env prod
  Effect:  Reverts corelink-prod Worker and CoreLinkServer DO to previous deployment.
  Note:    DO storage state survives rollback. Only code rolls back.
  Risk:    None for data. Workers.dev subdomain returns to previous Worker version.

Step 2: Container rollback
  Command: wrangler containers rollback corelink-server:prod
  Effect:  Reverts the CF Container image to the previous pushed version.
  Note:    Container must be re-started by DO after Worker rollback.

Step 3: DNS rollback (from Phase G snapshot)
  Snapshot: /tmp/dns-rollback-latest.json
  Command:  bash scripts/dns-prod-apply.sh --rollback /tmp/dns-rollback-latest.json
  Effect:   Deletes all 9 newly-created CNAME records from Phase G APPLY.
            status.corelink.humangr.com (Phase A) is NOT touched.
  Note:     Only runs if snapshot file exists.

Step 4: Post-rollback smoke verification
  Command: bash scripts/smoke-prod-corelink.sh
  Expected: Worker on workers.dev subdomain returns 200 (pre-DNS cutover baseline).
            DNS checks may fail if DNS was not yet applied — that is OK.

Decision tree:
  - Worker rollback fails     → HALT + manual action (wrangler deployments promote <id>)
  - Container rollback fails  → WARN + continue to DNS rollback (Worker already safe)
  - DNS rollback fails        → ERR + manual deletion of 9 CNAME records
  - Post-rollback smoke fails → WARN + escalate to Owner
```

---

## §6 Charter compliance

| Control | Check | Status |
|---|---|---|
| CTRL-CRED-001 | No credential values in any script or audit doc | PASS — `CORELINK_SMOKE_TOKEN` read from env only; absent = SKIP (not FAIL) |
| Cutover-fail-CLOSED | Post-cutover smoke failure triggers auto-rollback | PASS — item 12 in cutover checklist calls `rollback-prod-corelink.sh --auto` |
| CTRL-AUDIT-EMIT-BEFORE-MUTATION | Each rollback step logged before wrangler invocation | PASS — `log "CTRL-AUDIT-EMIT: about to invoke..."` before every destructive call |
| Public smoke targets only | All smoke HTTP/DNS checks use public endpoints | PASS — no internal VPC or private API calls |
| Default safe | Cutover defaults to interactive; rollback defaults to --dry-run for awareness | PASS — destructive steps require explicit confirmation |
| Idempotency | Re-running smoke is safe (read-only); cutover is idempotent at each step | PASS |
| Bash strict mode | `set -euo pipefail` in all three scripts | PASS |
| Hard pause triggers | All 3 triggers documented (§8) | PASS |
| Parallel-safety | This prep branch touches only 3 scripts + this audit doc | PASS |

---

## §7 What Phase H APPLY will do

Phase H APPLY executes after Phase E (Container deploy complete), Phase F (Pages live),
and Phase G (DNS live). Steps:

1. **Pre-cutover smoke:** `bash scripts/smoke-prod-corelink.sh`
   - All 22 checks must pass (exit 0) before cutover proceeds.

2. **Run existing dressrun scripts** (from prior waves):
   - `bash scripts/pre-cutover-weekly-verify.sh`
   - `bash scripts/ga-cutover-prod-dressrun.sh`
   - `bash scripts/statuspage-init-dressrun.sh` (against REAL BetterStack page)

3. **Cutover checklist:** `bash scripts/cutover-checklist-prod.sh`
   - Operator walks through 15 items interactively.
   - At item 9, executes `wrangler deployments promote <id> --env prod` (5% → 100%).
   - At item 12, re-runs smoke; any failure triggers auto-rollback.
   - At item 15, operator types `CUTOVER` to commit.
   - Output signed to `specs/_audits/2026-05-26-w32-phaseH-cutover-checklist-<ts>.md`.

4. **Refresh GA readiness doc:**
   - Update `specs/_audits/sealed/2026-05-16-ga-readiness-final.md`:
     replace all "engineering-CLOSED, deploy-pending" → "deploy-COMPLETE".

5. **Update `specs/_compliance/GA-GATE-CRITERIA.md`:**
   - Sign rows: status page, worker, container, pages, DNS, secrets.

6. **Phase H APPLY audit doc:**
   - `specs/_audits/2026-05-22-w32-phaseH-smoke-cutover.md`
   - Captures smoke output, cutover timeline, adversarial review score, and final gate state.

7. **Phase I** (after H):
   - Wave-32 closure audit + DEBT register update + `git tag corelink-prod-deploy-v1`.

---

## §8 Hard pause triggers

| # | Trigger | Check location | Action |
|---|---|---|---|
| 1 | `scripts/dns-prod-plan.sh` not present | Both smoke and cutover scripts check at startup | Exit 1 with HARD PAUSE TRIGGER 1 message |
| 2 | `wrangler` CLI not installed or not in PATH | Cutover and rollback check at startup; smoke warns only | Exit 1 (cutover/rollback); warn (smoke) |
| 3 | `specs/_audits/sealed/2026-05-26-w32-phaseG-prep.md` has fewer than 9 DNS plan rows | Cutover script: `grep -c "corelink.*humangr.com.*CNAME"` | Exit 1 with HARD PAUSE TRIGGER 3 message |

**Additional inherited triggers from wave-32 spec §7 (evaluated at Phase H APPLY time):**
- Post-cutover smoke exit code > 0 → auto-rollback (cutover-fail-CLOSED)
- Adversarial review score < 7.5/10 → cutover checklist item 8 abort

**Current state (2026-05-26):** All 3 hard pause triggers are clear. Phase G audit present,
9 plan rows confirmed, smoke and rollback scripts syntax-validated.

---

## §9 Sign-off

### Acceptance criteria

| Criterion | Status |
|---|---|
| `scripts/smoke-prod-corelink.sh` written + executable | DONE |
| `scripts/cutover-checklist-prod.sh` written + executable | DONE |
| `scripts/rollback-prod-corelink.sh` written + executable | DONE |
| All three: `bash -n` syntax check PASS | DONE (verified in prep run) |
| Smoke script `--dry-run` lists all 22 checks across 5 areas | DONE — §5.1 |
| Cutover checklist has 15 explicit items (requirement: 12–15) | DONE — §3 |
| Rollback decision tree documented | DONE — §4 |
| Cutover-fail-CLOSED: post-smoke failure triggers auto-rollback | DONE — item 12 + `--auto` flag |
| CTRL-CRED-001: no credentials in any script | DONE — §6 |
| Parallel-safety: zero file overlap with Phase B/C/D/E/F/G | DONE |
| Hard pause triggers documented | DONE — §8 |
| SEAL audit committed | DONE — this document |

### Script inventory

| Script | Path | LOC | Syntax |
|---|---|---|---|
| Smoke | `scripts/smoke-prod-corelink.sh` | 504 | PASS |
| Cutover checklist | `scripts/cutover-checklist-prod.sh` | 519 | PASS |
| Rollback | `scripts/rollback-prod-corelink.sh` | 407 | PASS |
| **Total** | | **1,430** | |

---

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---

*End of Wave 32 Phase H PREP audit.*
