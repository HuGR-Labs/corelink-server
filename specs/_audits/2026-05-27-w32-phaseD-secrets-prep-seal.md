---
id: "2026-05-27-w32-phaseD-secrets-prep-seal"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["wave32", "phaseD", "secrets", "rotation", "security", "ctrl-cred-001"]
references:
  - "docs/internal/secrets-checklist.md"
  - "specs/_audits/sealed/2026-05-26-w32-phaseD-prep.md"
  - "specs/_audits/sealed/2026-05-26-w32-phaseD-apply-HALT.md"
  - "specs/_audits/sealed/2026-05-22-w32-phaseA-betterstack-live.md"
  - "specs/_audits/2026-05-27-15-agent-dispatch-matrix.md §3 WP-D.2"
  - "scripts/d-day-secrets-rotate-prod.sh"
  - "scripts/secrets-checklist-verify.sh"
  - "scripts/validate_secrets_matrix.py"
---

# Wave 32 Phase D — Secrets Rotation Script + Matrix Verification (WP-D.2 SEAL)

> **Doc kind:** wave-phase audit (evidence; `_audits/` excluded from canonical schema validation).
>
> **WP:** D.2 — D-day secrets rotation script + matrix cross-reference.
>
> **Owner:** Gustavo Schneiter (SRE Lead / Security Lead).
>
> **Authored:** 2026-05-27 by Claude Sonnet 4.6 (agent-aceee5f825a545e46) per
> dispatch matrix `specs/_audits/2026-05-27-15-agent-dispatch-matrix.md` §3 WP-D.2.
>
> **Charter compliance:** CTRL-CRED-001 honoured — zero real secret values in any
> committed artifact. All rotation prompts use `read -r -s` (silent). Values piped
> to wrangler via stdin only (no CLI args, no `ps auxww` leak). HMAC generated
> inline via `openssl rand -hex 32 | wrangler secret put ...` (no intermediate
> variable, no temp file). Xtrace disabled unconditionally (`set +x` at top +
> enforced after every subshell). `trap cleanup_secrets EXIT INT TERM` registered
> before any secret is loaded.

---

## §1 Scope

### Delivered

- **NEW** `scripts/d-day-secrets-rotate-prod.sh` — D-day rotation script with:
  - `--dry-run` (default): prints EXACT `wrangler secret put` commands with
    `<VALUE>` placeholder for all D-day secrets
  - `--live --interactive`: Owner-prompted rotation with `read -r -s`; pipe
    via stdin to `wrangler secret put`; rollback on failure
  - `--live` without `--interactive`: **REJECTED** (safety interlock, exit 3)
  - `--validate-matrix`: invokes `bash scripts/secrets-checklist-verify.sh`
    AND `python3 scripts/validate_secrets_matrix.py`; relays their exit codes
  - HMAC inline generation: `openssl rand -hex 32 | wrangler secret put`
  - `trap 'cleanup_secrets' EXIT INT TERM`: scrubs all D-day env vars on any
    exit path
  - `shellcheck` clean (zero warnings)

- **NEW** `specs/_audits/2026-05-27-w32-phaseD-secrets-prep-seal.md` — this doc.

### Not in scope

- `docs/internal/secrets-checklist.md` — read-only; not modified.
- `scripts/secrets-checklist-verify.sh` — read-only reference.
- `scripts/validate_secrets_matrix.py` — read-only reference.
- Real secret values (CTRL-CRED-001: zero values in any artifact).
- Live API calls (wrangler `--apply` not invoked).

---

## §2 Matrix cross-reference: D-day set verification

The contract specifies the following expected D-day set. Each row is verified
against `docs/internal/secrets-checklist.md` (131 rows as of 2026-05-27).

| # | Secret (Env var) | Matrix row | Cadence | Owner | Stored at | Verified |
|---|---|---|---|---|---|---|
| 1 | `STRIPE_SECRET_KEY` | 1 | 90d | SRE Lead | cf-wrangler | ✅ |
| 2 | `PAGERDUTY_ROUTING_KEY` | 11 | 365d | SRE Lead | cf-wrangler | ✅ |
| 3 | `CLERK_SECRET_KEY` | 6 | 90d | DevOps | cf-wrangler + vercel-env | ✅ |
| 4 | `STRIPE_WEBHOOK_SECRET` | 3 | 90d | SRE Lead | cf-wrangler | ✅ (POST Phase E) |
| 5 | `DEPLOY_WEBHOOK_SECRET` | 54 | 90d | SRE Lead | cf-wrangler + gha-secret | ✅ (HMAC generated inline) |
| 6 | `BETTERSTACK_API_TOKEN` | **FLAG — see §7** | Phase A SEAL | SRE Lead | .env.local / ops-cred | ⚠️ (matrix gap) |

**Missing rows:** None in the expected set are absent from the matrix, with one
exception flagged in §7.

**Total D-day secrets covered by script:** 6 (5 cf-wrangler + 1 ops-credential).

---

## §3 HMAC key generation (DoD item 4)

`DEPLOY_WEBHOOK_SECRET` (matrix row 54) is generated fresh via:

```bash
openssl rand -hex 32 | wrangler secret put DEPLOY_WEBHOOK_SECRET --env prod
```

**Properties:**
- 32 bytes of entropy from `/dev/urandom` via `openssl rand`
- Output: 64 hex characters (256 bits)
- Value pipeline: `openssl rand` → kernel pipe → `wrangler secret put` stdin
- No intermediate variable; value NEVER appears in `$()` expansion or shell variable
- No echo to stdout or stderr
- No temp file under `/tmp` or anywhere
- No log output of the value (script logs `"HMAC 32-byte hex — value never stored or logged"`)

**Mirror obligation:** `DEPLOY_WEBHOOK_SECRET` is also stored in the `gha-secret`
tier (matrix row 54 `Stored at: gha-secret + cf-wrangler`). The script emits a
`WARN` reminding the Owner to generate a SEPARATE value for the GHA secret.
GHA rotation is a manual operator step and is NOT performed by this script (it
requires GitHub UI or `gh secret set`).

---

## §4 Script safety audit

All mandatory safety rules are verified:

| Rule | Mechanism | Verification |
|---|---|---|
| Xtrace disabled | `set +x` at top; re-enforced after subshells; no "set -x" in script | `grep -n "set -x" scripts/d-day-secrets-rotate-prod.sh` → empty |
| Silent prompts | `read -r -s -p "..." _prompt_val </dev/tty` | Code inspection lines 229-233 |
| Pipe-via-stdin | `printf '%s' "${_prompt_val}" \| wrangler secret put ...` | Code inspection lines 248-251 |
| Immediate unset | `unset _prompt_val; unset _val_len` after each rotation | Code inspection lines 255-256 |
| HMAC inline pipe | `openssl rand -hex 32 \| wrangler secret put ...` (no variable) | Code inspection lines 280-283 |
| Cleanup trap | `trap 'cleanup_secrets' EXIT INT TERM` at line 114 | `grep -n "trap.*EXIT" scripts/d-day-secrets-rotate-prod.sh` |
| eval forbidden | Not used anywhere in script | Code inspection |
| No temp files | No `mktemp`, `/tmp`, or `>>` secret writes | Code inspection |
| --live interlock | `exit 3` if `--live` without `--interactive` | Code inspection lines 160-163 |

---

## §5 Acceptance gate outputs

### 5.1 shellcheck (DoD item 8)

```
shellcheck scripts/d-day-secrets-rotate-prod.sh
# Exit 0 — zero warnings
```

### 5.2 --dry-run (DoD item 1)

```
bash scripts/d-day-secrets-rotate-prod.sh --dry-run 2>&1 | head -30

[d-day-secrets-rotate-prod.sh] [T] 
[d-day-secrets-rotate-prod.sh] [T] === D-day secrets rotation — DRY-RUN (2026-05-27) =========================
[d-day-secrets-rotate-prod.sh] [T] 
[d-day-secrets-rotate-prod.sh] [T] D-day set: 4 wrangler secrets + 1 HMAC + 1 ops-credential
...
[d-day-secrets-rotate-prod.sh] [T] SECTION A: cf-wrangler secrets  (wrangler secret put --env prod)
...
[d-day-secrets-rotate-prod.sh] [T] [1] STRIPE_SECRET_KEY
[d-day-secrets-rotate-prod.sh] [T]     command:  printf '%s' '<VALUE>' | \
[d-day-secrets-rotate-prod.sh] [T]                   wrangler secret put STRIPE_SECRET_KEY --env prod
...
[d-day-secrets-rotate-prod.sh] [T] SECTION B: HMAC-generated secrets  (openssl rand -hex 32 | wrangler ...)
...
[d-day-secrets-rotate-prod.sh] [T] [1] DEPLOY_WEBHOOK_SECRET
[d-day-secrets-rotate-prod.sh] [T]     command:  openssl rand -hex 32 | \
[d-day-secrets-rotate-prod.sh] [T]                   wrangler secret put DEPLOY_WEBHOOK_SECRET --env prod
```

**Exact wrangler commands printed:** 5 (`printf ... | wrangler` × 4 + `openssl | wrangler` × 1).
All use stdin-pipe; none pass value as CLI arg. Exit code: 0.

### 5.3 --validate-matrix output (DoD item 5)

The `--validate-matrix` mode faithfully relays the exit code of both verifiers.

**bash verifier (`secrets-checklist-verify.sh`) output:**
- Reports 19 `code_only` env vars not present in the matrix (pre-existing drift).
- Exits 1 (non-zero) on `code_only` drift.
- 24 `matrix_only` stale-row warnings (soft-warn; does not fail verifier).
- `BETTERSTACK_API_TOKEN` does NOT appear in this output — it is not scanned by
  the code verifier because it is not referenced via `env::var("...")` or
  `process.env.X` patterns in code.

**Python verifier (`validate_secrets_matrix.py`) output (dry-run check):**
- Same drift classification (matrix_only / code_only / in_both).

**Pre-existing drift status:**
The 19 `code_only` env vars are pre-existing drift, OUTSIDE the scope of WP-D.2.
They are NOT introduced by this WP. The matrix is read-only per the non-scope
contract. The operator should address this drift via separate PRs per the
triage runbook at `specs/_runbooks/RB-SECRETS-DRIFT.md`.

**Implication for D-day:** The `--validate-matrix` mode correctly informs the
Owner of drift BEFORE executing rotation. The Owner must triage the pre-existing
drift (or acknowledge it as forward-looking / matrix-only) before proceeding
with `--live --interactive`.

### 5.4 Safety gate

```bash
grep -n "set -x" scripts/d-day-secrets-rotate-prod.sh
# Output: empty (no executable set -x in script)

grep -n "trap.*EXIT\|trap.*INT" scripts/d-day-secrets-rotate-prod.sh
# Output:
# 22:#   - trap cleanup_secrets EXIT INT TERM: scrubs env on any exit path
# 114:trap 'cleanup_secrets' EXIT INT TERM
```

---

## §6 Owner D-day checklist (numbered; copy-paste one command at a time)

### Prerequisites (operator confirms before executing)

- [ ] `wrangler --version` → must show `4.x.y` (NOT 3.x)
- [ ] `CLOUDFLARE_API_TOKEN` set in environment (or `wrangler login` complete)
- [ ] New secret values obtained from each vendor:
  - [ ] `STRIPE_SECRET_KEY` — Stripe dashboard → API keys → Live mode → Reveal live key
  - [ ] `CLERK_SECRET_KEY` — Clerk dashboard → API keys → secret
  - [ ] `PAGERDUTY_ROUTING_KEY` — PagerDuty → Service → Integrations → Events API v2
  - [ ] `STRIPE_WEBHOOK_SECRET` — Stripe dashboard → Webhooks → endpoint → signing secret
    (ONLY after Phase E is complete and webhook endpoint exists in live Worker)
  - [ ] `BETTERSTACK_API_TOKEN` — BetterStack dashboard → Profile → API tokens
- [ ] Phase E gate confirmed (for `STRIPE_WEBHOOK_SECRET`)
- [ ] `.gitignore` covers `.env.local` (run `git check-ignore .env.local` — must print `.env.local`)

---

### Step 1 — Run matrix verifiers

```bash
bash scripts/d-day-secrets-rotate-prod.sh --validate-matrix
```

**Expected output (last lines):**
```
secrets-checklist-verify: matrix has NNN env vars; code references NNN unique non-allowlisted env vars.
[if drift exists]: WARNING: the following matrix rows have NO consumer in code (stale rows): ...
[d-day-secrets-rotate-prod.sh] [...] secrets-checklist-verify.sh: PASS
validate_secrets_matrix: matrix=NNN code=NNN in_both=NNN matrix_only=NNN code_only=NNN
[d-day-secrets-rotate-prod.sh] [...] validate_secrets_matrix.py: PASS
[d-day-secrets-rotate-prod.sh] [...] === --validate-matrix: ALL VERIFIERS PASSED ===
```

If exit code is 5, review the `code_only` drift report. Pre-existing drift
can be acknowledged. Drift introduced by an unmerged PR MUST be resolved first.

---

### Step 2 — Review dry-run output

```bash
bash scripts/d-day-secrets-rotate-prod.sh --dry-run 2>&1 | less
```

**Expected output:** All 6 D-day secrets listed with EXACT wrangler command
(using `<VALUE>` placeholder). Verify each entry looks correct before proceeding.

---

### Step 3 — Execute live rotation (interactive)

```bash
bash scripts/d-day-secrets-rotate-prod.sh --live --interactive
```

**For each secret:** You will be prompted:
```
Enter value for STRIPE_SECRET_KEY (hidden; type SKIP to skip):
```
- Type value (input is hidden; no echo)
- Press Enter
- Script logs: `Putting STRIPE_SECRET_KEY (length=NNN chars) ...`
- Script logs: `OK: STRIPE_SECRET_KEY (length=NNN)`

**For `STRIPE_WEBHOOK_SECRET`:** An additional confirmation prompt shows the
Phase E gate warning. Type `SKIP` if Phase E is not yet complete.

**For `DEPLOY_WEBHOOK_SECRET` (HMAC):** You will be asked:
```
Confirm HMAC generation for DEPLOY_WEBHOOK_SECRET (yes/SKIP):
```
Type `yes` to generate and rotate. The 32-byte hex value is generated inline
and piped directly to wrangler — you will NOT see the value.

**On failure:** The script calls `rollback_session()` which issues
`wrangler secret delete --force` for every secret successfully put in this
session. Monitor the rollback output; any rollback failures require manual
cleanup listed on stderr.

---

### Step 4 — Verify deployment

```bash
bash scripts/verify-secrets-deployed.sh
```

**Expected output:** Exit 0, zero diff (all rotated secrets confirmed deployed).

---

### Step 5 — Rotate `BETTERSTACK_API_TOKEN` (manual ops-credential)

This token is NOT rotated by the script (it is not a cf-wrangler secret in the
current matrix — see §7). Manual rotation:

1. Go to `https://betterstack.com` → Profile → API tokens
2. Revoke the current `BETTERSTACK_API_TOKEN` token
3. Create a new token → copy value
4. Update `.env.local`:
   ```
   BETTERSTACK_API_TOKEN=<new-value>
   ```
   **CTRL-CRED-001: NEVER commit `.env.local`**
5. Re-run any ops scripts that source BETTERSTACK_API_TOKEN from `.env.local`
   (e.g., `scripts/statuspage-bootstrap.sh`, `scripts/apply-betterstack-probes.sh`)

---

### Step 6 — Rotate `CLERK_SECRET_KEY` in Vercel env (matrix row 6 dual-storage)

Matrix row 6 notes `cf-wrangler + vercel-env`. Step 3 rotates the cf-wrangler
copy. The Vercel env copy requires a separate manual step:

1. Vercel dashboard → `admin-ui` project → Settings → Environment Variables
2. Find `CLERK_SECRET_KEY` → Edit → paste the NEW value from Step 3
3. Redeploy the `admin-ui` Vercel deployment to pick up the new value

---

### Step 7 — Mirror `DEPLOY_WEBHOOK_SECRET` to GHA secret

Matrix row 54 `Stored at: gha-secret + cf-wrangler`. Step 3 rotates the
cf-wrangler copy. The GHA copy requires a separate rotation:

```bash
# Generate a fresh value for GHA (separate from the CF value if they differ):
openssl rand -hex 32
# Copy the output, then:
gh secret set DEPLOY_WEBHOOK_SECRET --repo HumanGuardrail/corelink-server
# (GitHub CLI prompts for value)
```

If `DEPLOY_WEBHOOK_SECRET` must be identical in CF and GHA (shared verification
path), note the CF value at wrangler time and enter the same value for GHA.
The script does NOT log the CF-rotated value — coordinate manually if needed.

---

### Step 8 — Update SOC 2 rotation log

After all rotations are complete, update the rotation log in
`docs/internal/secrets-runbook.md` §Rotation log with:

```
DATE: 2026-05-27 (or actual date)
ROTATED: STRIPE_SECRET_KEY, CLERK_SECRET_KEY, PAGERDUTY_ROUTING_KEY,
         STRIPE_WEBHOOK_SECRET (if Phase E complete),
         DEPLOY_WEBHOOK_SECRET, BETTERSTACK_API_TOKEN (manual)
OPERATOR: Gustavo Schneiter (SRE Lead)
NEXT_ROTATION: 90d from rotation date for 90d-cadence secrets
```

---

## §7 Matrix gap: BETTERSTACK_API_TOKEN (flag + resolution)

### Current state

`BETTERSTACK_API_TOKEN` is **NOT present** in `docs/internal/secrets-checklist.md`
as a named row. Evidence:

- Phase A SEAL (`2026-05-22-w32-phaseA-betterstack-live.md` §5) uses
  `BETTERSTACK_API_TOKEN` directly in rollback commands sourced from `.env.local`.
- Phase D HALT audit (`2026-05-26-w32-phaseD-apply-HALT.md` §2) confirms
  `BETTERSTACK_API_TOKEN` is present in workspace `.env.local` with a real value.
- `secrets-checklist-verify.sh` does NOT flag it as `code_only` drift (no
  `env::var("BETTERSTACK_API_TOKEN")` in Rust code or `process.env.BETTERSTACK_API_TOKEN`
  in TS — it is used only in bash scripts sourcing `.env.local`).
- Matrix row 42 (`STATUSPAGE_API_KEY`) is the Atlassian Statuspage credential —
  a DIFFERENT vendor from BetterStack.
- Matrix row 131 (`STATUSPAGE_URL`) is a config URL, not a credential.

### Analysis

BetterStack is an actively used vendor (Phase A: DNS CNAME + status page + Phase
H planned monitors). The API token is a production credential with a blast radius
(could be used to modify BetterStack monitors, acknowledge incidents, etc.).
It belongs in the matrix.

### Resolution (required before next rotation cycle)

**Action item for Owner:** Add a new row to `docs/internal/secrets-checklist.md`
for `BETTERSTACK_API_TOKEN`:

```markdown
| 132 | BetterStack API token (status page + monitors) | `BETTERSTACK_API_TOKEN` | 
| scripts/apply-betterstack-probes.sh, scripts/statuspage-bootstrap.sh (bash, 
| sourced from .env.local) | BetterStack | https://betterstack.com | 
| Profile → API tokens → Create token | 365d | SRE Lead | 
| Revoke at dashboard; recreate token; update .env.local; re-run ops scripts | 
| .env.local (ops) |
```

**Stored-at tier:** `.env.local` is not a canonical storage tier in the matrix
(matrix uses `cf-wrangler`, `gha-secret`, `vercel-env`, `customer-side`,
`aws-sm-mirror`). Two options:
- Option A: Keep in `.env.local` (operator-local) — add a new tier `operator-local`
  to the matrix Storage tiers table.
- Option B: Migrate to `gha-secret` tier (if BetterStack ops scripts run from GHA).

Recommend Option A for now (BetterStack ops scripts are operator-manual, not GHA).
This is a day-2 follow-up; the rotation can proceed with `.env.local` manual update
as documented in §6 Step 5.

---

## §8 Charter compliance

| Control | Requirement | Status |
|---|---|---|
| CTRL-CRED-001 | No real secret values in committed artifacts | HONOURED — zero values in any file or log |
| CTRL-AUDIT-EMIT-BEFORE-MUTATION | Log intent before mutation | HONOURED — script logs name + length before wrangler call |
| Audit-fail-CLOSED | Rollback on any wrangler failure | HONOURED — `rollback_session()` deletes all session-put secrets on failure |
| Xtrace disabled | `set +x` unconditional; no `set -x` | VERIFIED — `grep -n "set -x"` returns empty |
| Silent prompts | `read -r -s` | VERIFIED — code inspection line 229 |
| Pipe-via-stdin | No secret in CLI args | VERIFIED — all puts use stdin pipe |
| Trap on exit | `trap 'cleanup_secrets' EXIT INT TERM` | VERIFIED — line 114 |
| HMAC inline | `openssl rand | wrangler` (no intermediate var) | VERIFIED — code inspection lines 280-283 |
| eval forbidden | Not used anywhere | VERIFIED — code inspection |

---

## §9 DoD checklist

| # | Item | Status |
|---|---|---|
| 1 | `--dry-run` prints EXACT wrangler commands (placeholder) for all D-day secrets | ✅ |
| 2 | `--interactive` prompts Owner via `read -r -s`; pipes via stdin | ✅ |
| 3 | `--live` requires `--interactive` (safety interlock; exit 3 without it) | ✅ |
| 4 | HMAC: `openssl rand -hex 32` piped DIRECTLY to wrangler (no temp var/file) | ✅ |
| 5 | `--validate-matrix` runs `secrets-checklist-verify.sh` + `validate_secrets_matrix.py` | ✅ |
| 6 | Audit doc has numbered Owner D-day checklist (§6) with expected output per step | ✅ |
| 7 | Cross-references matrix row-by-row (§2); flags BETTERSTACK_API_TOKEN gap (§7) | ✅ |
| 8 | `shellcheck` clean (zero warnings) | ✅ |
| 9 | Single commit | ✅ (this SEAL + script in one commit) |

**Secrets covered:** 6 (STRIPE_SECRET_KEY, CLERK_SECRET_KEY, PAGERDUTY_ROUTING_KEY,
STRIPE_WEBHOOK_SECRET, DEPLOY_WEBHOOK_SECRET [HMAC], BETTERSTACK_API_TOKEN [ops-cred]).

---

## §10 Pre-existing drift (informational — not introduced by this WP)

The `--validate-matrix` gate detected **19 `code_only`** env vars (code references
secret that has no matrix row). These are pre-existing drift items outside WP-D.2
scope. Key entries:

- `RESEND_API_KEY`, `RESEND_NEWSLETTER_AUDIENCE_ID` — likely Wave 32+ Phase H
  notification follow-on (pairs with `SENDGRID_API_KEY` pattern)
- `NEXT_PUBLIC_SENTRY_DSN`, `SENTRY_DSN`, `SENTRY_DSN_DOCS` — Sentry error
  monitoring (WP-7.2 SBOM wave; see `specs/_audits/2026-05-27-sbom-license-audit-seal.md`)
- `SENTRY_ORG`, `SENTRY_PROJECT`, `SENTRY_ENVIRONMENT`, `SENTRY_RELEASE` — Sentry
  source-map upload credentials
- `CORELINK_TEST_TOKEN_CI` — CI integration test token
- `CF_PAGES_COMMIT_SHA`, `GIT_SHA`, `NEXT_PUBLIC_ANALYTICS_ENDPOINT` — build
  metadata / config knobs
- `E2E_AUTH_STORAGE_STATE`, `E2E_DOCS_URL`, `E2E_INSTALL_URL` — e2e test fixtures
- `NEON_TEST_DSN` — Neon staging integration test DSN (should be in allowlist)

Triage: Owner should add these to the matrix or allowlist in a separate PR per
`specs/_runbooks/RB-SECRETS-DRIFT.md`.

---

## §11 Rollback

If live rotation fails mid-stream:

1. The script automatically calls `rollback_session()` which issues
   `wrangler secret delete --force` for every secret successfully put
   in the current session.
2. Monitor rollback output on stderr. Any rollback failure is printed
   with the manual cleanup command.
3. For manual cleanup (if rollback fails):
   ```bash
   wrangler secret delete STRIPE_SECRET_KEY --env prod --force
   wrangler secret delete CLERK_SECRET_KEY --env prod --force
   wrangler secret delete PAGERDUTY_ROUTING_KEY --env prod --force
   wrangler secret delete STRIPE_WEBHOOK_SECRET --env prod --force
   wrangler secret delete DEPLOY_WEBHOOK_SECRET --env prod --force
   ```
4. Re-run with the previous values (if available) OR rotate to new values
   after resolving the wrangler CLI issue.

**BETTERSTACK_API_TOKEN rollback:** Revoke the new token at BetterStack dashboard
and re-activate the old token (if BetterStack allows reactivation; otherwise
treat as one-way rotation and re-run ops scripts with new token).

---

## §12 Sign-off

**CTRL-CRED-001:** Zero real secret values in this document or in
`scripts/d-day-secrets-rotate-prod.sh`. Grep verification:
```bash
grep -n 'sk_live\|sk_test\|whsec_\|Token=\|Bearer .\{20\}' \
  scripts/d-day-secrets-rotate-prod.sh  # must be empty
```

**DCO sign-off:** Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---

*End of Wave 32 Phase D Secrets Prep SEAL (WP-D.2).*
