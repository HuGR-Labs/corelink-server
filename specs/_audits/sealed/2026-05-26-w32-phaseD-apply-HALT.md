# Wave 32 Phase D Apply — HALT Audit (2026-05-26)

> **Doc kind:** wave-scope HALT audit — production apply blocked; operator unblock required.
>
> **Owner:** Gustavo Schneiter (Security + Release Lead).
>
> **Authored:** 2026-05-26 by Claude Sonnet 4.6 (agent-a439b54302191076f) during Phase D apply dispatch.
>
> **Trigger:** Phase D apply agent mandate — run `apply-d1-migrations-prod.sh --apply` + `put-secrets-prod.sh --apply` + `verify-secrets-deployed.sh` against live prod. **HALTED before any prod mutation** — multiple hard-pause triggers fired during baseline verification (Step 0).
>
> **Base commit:** `9dcbd2a3e6c7ad8ce29b100c07f6886567bcb2e6` (confirmed matches mandate)
>
> **Charter compliance:** CTRL-CRED-001 honoured — zero prod mutations attempted; zero secret values logged. Audit-fail-CLOSED: no session secrets to roll back (apply never started). Hard-pause protocol executed per charter §"Hard pause triggers — HALT if".

---

## §1 Scope

This agent was dispatched to execute the Phase D APPLY (real production state change):

- Step 1: `bash scripts/apply-d1-migrations-prod.sh --apply` → apply 52 D1 migrations to `corelink-prod-d1`
- Step 2: `bash scripts/put-secrets-prod.sh --apply` → push 55 cf-wrangler secrets via `wrangler secret put --env prod`
- Step 3: `bash scripts/verify-secrets-deployed.sh` → zero-diff verification
- Step 4: SEAL audit commit

**No production mutations were performed.** The agent halted during Step 0 (baseline verification) upon detecting three hard-pause trigger conditions. The mandate explicitly requires HALT + SEAL doc on any trigger; no workarounds are permitted.

---

## §2 Baseline verification results

### Check 1 — HEAD commit

```
$ git rev-parse HEAD
9dcbd2a3e6c7ad8ce29b100c07f6886567bcb2e6
```

**PASS.** Matches expected `9dcbd2a3`.

### Check 2 — Runner scripts exist

```
$ ls scripts/apply-d1-migrations-prod.sh scripts/put-secrets-prod.sh scripts/verify-secrets-deployed.sh
scripts/apply-d1-migrations-prod.sh
scripts/put-secrets-prod.sh
scripts/verify-secrets-deployed.sh
```

**PASS.** All three scripts present.

### Check 3 — `.env.local` key count

The mandate specifies: confirm `.env.local` at workspace root has `CF_API_TOKEN` + `CF_ACCOUNT_ID` + all 55 secret values.

**Two sub-issues found:**

**3a.** The workspace root `.env.local` (`/Users/gustavoschneiter/Documents/HuGR/corelink-server/.env.local`) has **8 uppercase-key lines** (well short of the required 55+):

```
CLOUDFLARE_ACCOUNT_ID
CLOUDFLARE_ZONE_ID_HUMANGR
CLOUDFLARE_API_TOKEN
BETTERSTACK_API_TOKEN
BETTERSTACK_PAGE_ID
PAGERDUTY_ROUTING_KEY
STRIPE_AUTH_MODE
STRIPE_SECRET_KEY
```

Missing from `.env.local` (47 of 55 cf-wrangler secrets absent — partial list):
`STRIPE_SECRET_KEY_TEST`, `STRIPE_WEBHOOK_SECRET`, `CLERK_PUBLISHABLE_KEY`, `CLERK_SECRET_KEY`, `CLERK_JWKS_URL`, `CLERK_JWT_ISSUER`, `CLERK_AUDIENCE`, `PAGERDUTY_SYNTHETIC_ROUTING_KEY`, `SLACK_WEBHOOK_URL_ALERTS_SEV1`, `SLACK_WEBHOOK_URL_ALERTS_SEV2`, `SLACK_WEBHOOK_URL_ENTERPRISE_INQUIRIES`, `SLACK_WEBHOOK_URL_BREACH_NOTIFICATIONS`, `SLACK_WEBHOOK_URL_ONCALL_HANDOFF`, `SLACK_WEBHOOK_URL_LIGHTHOUSE_CUSTOMERS`, `HUBSPOT_PRIVATE_APP_TOKEN`, `AWS_REGION`, `AWS_USE_FIPS_ENDPOINT`, `GOOGLE_APPLICATION_CREDENTIALS`, `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `SENDGRID_API_KEY`, `TWILIO_ACCOUNT_SID`, `TWILIO_AUTH_TOKEN`, `STATUSPAGE_API_KEY`, `DT_API_URL`, `DT_API_KEY`, `DT_WEBHOOK_SECRET`, `DEPLOY_WEBHOOK_SECRET`, `PORT`, `AZURE_FEDERATED_TOKEN_FILE`, `DRATA_API_BASE_URL`, `DRATA_API_KEY`, `HTTP_PORT`, `STRIPE_PRICE_ID_STARTER`, `GCP_REGION`, `CORELINK_BYOK_AZURE_VAULT_URL`, `CORELINK_BYOK_AZURE_REGION`, `CORELINK_BYOK_VAULT_REGION`, `STATUSPAGE_PAGE_ID`, `STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS`, `STATUSPAGE_TENANT_ID`, `NEON_DB_URL_IAD`, `NEON_DB_URL_FRA`, `NEON_DB_URL_GRU`, `NEON_DB_URL_NRT`, `NEON_DB_URL_SYD`, `HUGR_WALLET_BASE`, `HUGR_WALLET_TOKEN`, `HUGR_STRIPE_REF`, `STRIPE_API_BASE`, `PAGERDUTY_TOKEN`, `STATUSPAGE_URL`.

**3b.** The scripts resolve `.env.local` relative to `REPO_ROOT` (`$(cd "$(dirname "$0")/.." && pwd)` = worktree root). **No `.env.local` exists at the worktree root** (`/Users/gustavoschneiter/Documents/HuGR/corelink-server/.claude/worktrees/agent-a439b54302191076f/.env.local`). The workspace root file would not be found by the scripts.

**TRIGGER 2 FIRED: `.env.local` missing at worktree root; workspace root `.env.local` is incomplete (8/55 secrets).**

### Check 4 — `wrangler --version`

```
$ wrangler --version
wrangler: command not found
```

`wrangler` is **not installed globally or on the system PATH**.

`npx wrangler --version` resolves to **v3.114.17** via npx. The mandate requires ≥ 4.x. v3.114.17 is a 3.x release.

**TRIGGER 1 FIRED: `wrangler` CLI not on PATH AND version is 3.x, not ≥4.x as required.**

### Check 5 — `wrangler d1 list`

```
$ npx wrangler d1 list
✘ [ERROR] Processing wrangler.toml configuration:
    - "containers" should be an object, but got an array
```

`wrangler.toml` has `[[containers]]` (array of tables syntax) which is valid TOML for Cloudflare Containers but **not recognized by wrangler v3.x** (requires wrangler ≥ 4.x where the Containers feature was introduced). This schema error prevents all wrangler D1 commands from executing.

**TRIGGER 3 FIRED (contributed): cannot confirm D1 database exists because wrangler errors on wrangler.toml.**

Note: The `wrangler.toml` schema is likely correct for wrangler 4.x; this error is a consequence of Trigger 1 (wrong wrangler version via npx).

### Check 6 — Phase D prep spec review

`specs/_audits/2026-05-26-w32-phaseD-prep.md` read and understood. The runner contract is clear. The prep agent noted in §5.3: "Full dry-run requires `wrangler` in PATH — not installed in this environment. The script exits 127 cleanly when wrangler is absent." This confirms the baseline gap was known at prep time and the apply agent requires the wrangler installation precondition to be met by the operator.

---

## §3 Migration results

**Not executed.** HALT before any migration applied.

- `apply-d1-migrations-prod.sh --apply` was NOT run.
- 0/52 migrations applied.
- D1 table count: unknown (wrangler CLI unavailable).
- `check_migrations_additive.py` gate: NOT re-run (blocked by wrangler absence, though the Python script itself could run independently).

---

## §4 Secrets results

**Not executed.** HALT before any secret pushed.

- `put-secrets-prod.sh --apply` was NOT run.
- 0/55 secrets put.
- Rollback: N/A — no secrets were put in this session.
- CTRL-CRED-001: no secret values logged, echoed, or written anywhere.

---

## §5 Verify diff

**Not executed.** HALT before verification step.

- `verify-secrets-deployed.sh` was NOT run.

---

## §6 Charter compliance

### CTRL-CRED-001 — No secret values logged

**HONOURED.** This agent read key names from `.env.local` only via `grep '^[A-Z]' .env.local | cut -d= -f1`. No values were echoed, logged, or included in this document. Zero secret values in any committed artifact.

### Audit-fail-CLOSED — Rollback

**N/A.** No secrets were put during this session. The rollback mechanism in `put-secrets-prod.sh` (delete all session-put secrets on failure) was not invoked because apply never started.

### INV-AUTH-MIGRATION-ADDITIVE

**NOT RE-RUN** (blocked; wrangler required for full flow). However, the Phase D prep audit documents that the Python guard (`scripts/check_migrations_additive.py`) passed at prep time: `OK: 59 migration file(s) scanned; all additive.` The migration files have not changed since then (HEAD is the same `9dcbd2a3`).

### Hard-pause protocol

**EXECUTED.** Three triggers fired. Agent halted without attempting any production mutation. This HALT SEAL doc is committed per the charter mandate.

---

## §7 What Phase D APPLY will do next (when unblocked)

Once the operator resolves the blockers (§8 below), the apply agent should:

1. Re-run Step 0 baseline checks — confirm all pass green.
2. Run `bash scripts/apply-d1-migrations-prod.sh --apply` — capture to `target/phase-d-apply-migrations.log`.
3. Run `bash scripts/put-secrets-prod.sh --apply` — capture to `target/phase-d-apply-secrets.log` (values redacted; NAME + SHA256[:8] only).
4. Run `bash scripts/verify-secrets-deployed.sh` — capture to `target/phase-d-apply-verify.log`; assert exit 0.
5. Write `specs/_audits/2026-05-26-w32-phaseD-apply.md` (the green SEAL).
6. Commit with DCO + Co-Authored-By.

---

## §8 Hard-pause triggers fired + operator unblock path

### TRIGGER 1 — wrangler CLI not on PATH / version < 4.x

**Evidence:**
```
$ wrangler --version
wrangler: command not found

$ npx wrangler --version
3.114.17
```

**Required:** wrangler ≥ 4.x installed globally on PATH.

**Operator unblock:**
```bash
# Install wrangler 4.x globally
npm install -g wrangler@^4

# Verify
wrangler --version
# Expected: 4.x.y
```

### TRIGGER 2 — `.env.local` missing at worktree root / incomplete at workspace root

**Evidence:**

- Worktree root `.env.local`: does not exist (`/Users/gustavoschneiter/Documents/HuGR/corelink-server/.claude/worktrees/agent-a439b54302191076f/.env.local`).
- Workspace root `.env.local` (`/Users/gustavoschneiter/Documents/HuGR/corelink-server/.env.local`): exists but has only 8 of the required 55 cf-wrangler secret keys populated.

**The 47 missing keys** (cf-wrangler secrets from `docs/internal/secrets-checklist.md` not present in `.env.local`):
`STRIPE_SECRET_KEY_TEST`, `STRIPE_WEBHOOK_SECRET`, `CLERK_PUBLISHABLE_KEY`, `CLERK_SECRET_KEY`, `CLERK_JWKS_URL`, `CLERK_JWT_ISSUER`, `CLERK_AUDIENCE`, `PAGERDUTY_SYNTHETIC_ROUTING_KEY`, `SLACK_WEBHOOK_URL_ALERTS_SEV1`, `SLACK_WEBHOOK_URL_ALERTS_SEV2`, `SLACK_WEBHOOK_URL_ENTERPRISE_INQUIRIES`, `SLACK_WEBHOOK_URL_BREACH_NOTIFICATIONS`, `SLACK_WEBHOOK_URL_ONCALL_HANDOFF`, `SLACK_WEBHOOK_URL_LIGHTHOUSE_CUSTOMERS`, `HUBSPOT_PRIVATE_APP_TOKEN`, `AWS_REGION`, `AWS_USE_FIPS_ENDPOINT`, `GOOGLE_APPLICATION_CREDENTIALS`, `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `SENDGRID_API_KEY`, `TWILIO_ACCOUNT_SID`, `TWILIO_AUTH_TOKEN`, `STATUSPAGE_API_KEY`, `DT_API_URL`, `DT_API_KEY`, `DT_WEBHOOK_SECRET`, `DEPLOY_WEBHOOK_SECRET`, `PORT`, `AZURE_FEDERATED_TOKEN_FILE`, `DRATA_API_BASE_URL`, `DRATA_API_KEY`, `HTTP_PORT`, `STRIPE_PRICE_ID_STARTER`, `GCP_REGION`, `CORELINK_BYOK_AZURE_VAULT_URL`, `CORELINK_BYOK_AZURE_REGION`, `CORELINK_BYOK_VAULT_REGION`, `STATUSPAGE_PAGE_ID`, `STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS`, `STATUSPAGE_TENANT_ID`, `NEON_DB_URL_IAD`, `NEON_DB_URL_FRA`, `NEON_DB_URL_GRU`, `NEON_DB_URL_NRT`, `NEON_DB_URL_SYD`, `HUGR_WALLET_BASE`, `HUGR_WALLET_TOKEN`, `HUGR_STRIPE_REF`, `STRIPE_API_BASE`, `PAGERDUTY_TOKEN`, `STATUSPAGE_URL`.

**Operator unblock:**
```bash
# Option A: populate workspace root .env.local with all 55 cf-wrangler secrets
# (the scripts will find it via REPO_ROOT symlink if the worktree's REPO_ROOT
#  resolves to the workspace root — verify with:)
#   cd scripts && dirname "$(pwd)" && pwd
# If REPO_ROOT = worktree path, create .env.local IN THE WORKTREE:
cp /Users/gustavoschneiter/Documents/HuGR/corelink-server/.env.local \
   /Users/gustavoschneiter/Documents/HuGR/corelink-server/.claude/worktrees/agent-a439b54302191076f/.env.local
# Then add the 47 missing keys with real values.

# Option B: run put-secrets-prod.sh with --env-file pointing at a fully-populated file:
bash scripts/put-secrets-prod.sh --apply --env-file /path/to/complete.env.local
```

**IMPORTANT — CTRL-CRED-001:** The `.env.local` file with real values must NEVER be committed. Ensure `.gitignore` covers `*.env.local` before populating.

### TRIGGER 3 — wrangler d1 list schema error (contributed, resolves with Trigger 1 fix)

**Evidence:**
```
$ npx wrangler d1 list
✘ [ERROR] Processing wrangler.toml configuration:
    - "containers" should be an object, but got an array
```

`wrangler.toml` uses `[[containers]]` (TOML array-of-tables syntax) which requires wrangler ≥ 4.x. With wrangler 3.x, this is a schema error that blocks all wrangler commands.

**Operator unblock:** Resolved automatically by fixing Trigger 1 (install wrangler ≥ 4.x). No changes to `wrangler.toml` are needed or appropriate — the Containers feature syntax is correct for wrangler 4.x.

---

## §9 Rollback path

### Migrations
Not applicable — no migrations were applied. If future apply fails mid-stream:
- Additive-only policy means no automatic rollback at D1 layer.
- Recovery path: `wrangler d1 migrations list corelink-prod-d1 --remote` to see applied state; consult `specs/_runbooks/RB-D1-MIGRATION-APPLY.md`.
- Last resort: drop + re-provision D1 (see Phase C audit for provisioning steps).

### Secrets
Not applicable — no secrets were put. If future apply fails mid-stream:
- The `put-secrets-prod.sh` script automatically rolls back all session-put secrets via `wrangler secret delete --force`.
- For manual cleanup: `wrangler secret delete <NAME> --env prod --force` for each secret in the 55-name list.

---

## §10 Sign-off

**Acceptance criteria status:**

- [ ] 52/52 migrations applied — **BLOCKED (not attempted)**
- [ ] 55/55 secrets put + verified — **BLOCKED (not attempted)**
- [ ] All 3 logs captured — **BLOCKED (not attempted)**
- [x] HALT SEAL audit doc with redacted output — **THIS DOCUMENT**
- [ ] DCO + Co-Authored-By — **PENDING commit**

**Hard-pause triggers fired:** 3 (wrangler not on PATH / version < 4.x; `.env.local` incomplete; wrangler.toml schema error on v3.x)

**Operator unblock summary (in order):**
1. `npm install -g wrangler@^4` — fixes Triggers 1 + 3.
2. Populate `.env.local` at worktree root (or pass `--env-file`) with all 55 cf-wrangler secrets — fixes Trigger 2.
3. Re-verify: `wrangler d1 list | grep corelink-prod-d1` must return `d64742ea-e102-40b2-a844-ff02e3f94562`.
4. Re-dispatch Phase D apply agent.

**DCO sign-off:** Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---

*End of Wave 32 Phase D Apply HALT Audit.*
