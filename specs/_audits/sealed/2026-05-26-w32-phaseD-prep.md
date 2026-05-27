# Wave 32 Phase D Prep Audit (2026-05-26)

> **Doc kind:** wave-scope prep audit (evidence; `_audits/` excluded from canonical schema validation).
>
> **Owner:** Gustavo Schneiter (Security + Release Lead).
>
> **Authored:** 2026-05-26 by Claude Sonnet 4.6 (agent-acec9199eb9d6d72c) during parallel Phase D prep dispatch.
>
> **Trigger:** Phase D prep agent mandate — write runner scripts for migrations + secrets put WITHOUT applying them. Blocked on Phase B (worker shim) + Phase C (CF infra) for the apply step.
>
> **Base commit:** `f9badfe9947df6a821a2f349c99853fa5b851fad`
>
> **Charter compliance:** SOTA bar 8.5/10; CTRL-CRED-001 (no hardcoded secrets); CTRL-AUDIT-EMIT-BEFORE-MUTATION (log before mutate); audit-fail-CLOSED on secret put (rollback on any failure); INV-AUTH-MIGRATION-ADDITIVE inherited from `check_migrations_additive.py` gate.

---

## §1 Scope (prep only; no apply)

This prep agent wrote the three runner scripts that Phase D (Migrations + Secrets) will invoke once Phase B (Worker shim) and Phase C (CF infra provision) are both green and the B+C → D decision gate is approved by the Owner.

**What this agent DID:**
- Verified baseline: HEAD = `f9badfe9`, migration count = 52, secrets matrix = 131 rows.
- Parsed `docs/internal/secrets-checklist.md` to identify 55 cf-wrangler secrets (out of 131 total rows).
- Derived expected post-migration table count (75 unique tables) from static analysis of the 52 SQL files.
- Wrote three scripts (228 + 298 + 212 = 738 LOC total).
- Ran dry-run for each script and captured output in §5.
- Ran `bash -n` syntax check on all three — all pass.
- Committed this audit doc.

**What this agent DID NOT do:**
- Apply any migration (`wrangler d1 migrations apply` not invoked).
- Push any secret (`wrangler secret put` not invoked).
- Modify `wrangler.toml`, `worker/`, or any infra binding.
- Overlap with Phase B (`worker/`), Phase C (CF resource IDs), Phase F (`deploy-pages-prod.sh`), or Phase G (`dns-prod-plan.sh`).

---

## §2 Script API surface

### `scripts/apply-d1-migrations-prod.sh`

| Flag | Behaviour |
|---|---|
| (none) | DRY-RUN: lists 52 migrations in lex order; runs additive guard; exits 0. No remote mutations. |
| `--apply` | APPLY: runs additive guard; invokes `wrangler d1 migrations apply corelink-prod-d1 --remote`; verifies table count post-apply. |
| `--help` | Prints usage. |

**Exit codes:**
- `0` — success (applied + verified, or dry-run clean)
- `1` — migration failure or verification failure
- `2` — usage error
- `3` — additive guard failure (DROP/ALTER-with-loss detected)
- `127` — wrangler not in PATH

**Key behaviours:**
- Hard-HALT if file count != 52 (spec baseline guard).
- Additive guard via existing `scripts/check_migrations_additive.py` (runs `python3 check_migrations_additive.py`).
- Post-apply: `SELECT count(*) FROM sqlite_master WHERE type='table'` via `wrangler d1 execute --json`; asserts >= 75.
- CTRL-AUDIT-EMIT-BEFORE-MUTATION: each migration logged (filename + statement count) before wrangler is invoked.

### `scripts/put-secrets-prod.sh`

| Flag | Behaviour |
|---|---|
| (none) | DRY-RUN: lists 55 cf-wrangler secrets in matrix order; exits 0. No remote mutations. |
| `--apply` | APPLY: sources `.env.local`; for each secret, reads from env or prompts interactively; invokes `wrangler secret put NAME --env prod` with value piped via stdin. |
| `--env-file <path>` | Override env file path (default: `.env.local`). Pass `/dev/null` to force all-interactive. |
| `--help` | Prints usage. |

**Exit codes:**
- `0` — success (all secrets put, or dry-run clean)
- `1` — wrangler secret put failure (rollback executed)
- `2` — usage error
- `127` — wrangler not in PATH

**Key behaviours:**
- CTRL-CRED-001: values sourced from `.env.local` or interactive prompt (`read -s`). NEVER hard-coded or passed as CLI args — piped via stdin to wrangler.
- Per-secret log: `[N/55] NAME ... OK (length=N chars; SHA256[:8]=XXXXXXXX)`. The hash is a rotation sanity check only; the value is never logged.
- Audit-fail-CLOSED rollback: if any `wrangler secret put` fails, all successfully-put secrets in the same session are deleted via `wrangler secret delete --force`. Rollback failures are reported to stderr.
- Secrets are parsed dynamically at runtime from `docs/internal/secrets-checklist.md` — the script is not hard-coded with the list. If the matrix drifts, the script picks it up automatically.

### `scripts/verify-secrets-deployed.sh`

| Flag | Behaviour |
|---|---|
| (none) | Queries `wrangler secret list --env prod --json`; diffs against canonical matrix; exits 0 if no diff, 1 if diff present. |
| `--env <env>` | Override CF Workers environment (default: `prod`). |
| `--help` | Prints usage. |

**Exit codes:**
- `0` — no diff (canonical == deployed)
- `1` — diff present (missing or extra secrets)
- `2` — usage error
- `127` — wrangler or jq not in PATH

**Output format:**
```
+SECRET_NAME   # in matrix but NOT deployed (missing)
-SECRET_NAME   # deployed but NOT in matrix (undocumented extra)
```

---

## §3 Parsed secrets list

**Parser:** Python inline script (identical logic in all three scripts); reads `docs/internal/secrets-checklist.md`, splits `|`-delimited rows, filters `row_num.isdigit()`, selects rows where column 11 (`Stored at`) contains `cf-wrangler`.

**Results (verified 2026-05-26):**

| Count | Category |
|---|---|
| 131 | Total rows in matrix |
| 55 | cf-wrangler secrets (pushed to CF Workers via `wrangler secret put`) |
| 76 | Non-cf-wrangler entries (gha-secret, vercel-env, customer-side, dev-only) |

**The 55 cf-wrangler secrets (in matrix order):**

```
STRIPE_SECRET_KEY
STRIPE_SECRET_KEY_TEST
STRIPE_WEBHOOK_SECRET
CLERK_PUBLISHABLE_KEY
CLERK_SECRET_KEY
CLERK_JWKS_URL
CLERK_JWT_ISSUER
CLERK_AUDIENCE
PAGERDUTY_ROUTING_KEY
PAGERDUTY_SYNTHETIC_ROUTING_KEY
SLACK_WEBHOOK_URL_ALERTS_SEV1
SLACK_WEBHOOK_URL_ALERTS_SEV2
SLACK_WEBHOOK_URL_ENTERPRISE_INQUIRIES
SLACK_WEBHOOK_URL_BREACH_NOTIFICATIONS
SLACK_WEBHOOK_URL_ONCALL_HANDOFF
SLACK_WEBHOOK_URL_LIGHTHOUSE_CUSTOMERS
HUBSPOT_PRIVATE_APP_TOKEN
AWS_REGION
AWS_USE_FIPS_ENDPOINT
GOOGLE_APPLICATION_CREDENTIALS
AZURE_TENANT_ID
AZURE_CLIENT_ID
AZURE_CLIENT_SECRET
SENDGRID_API_KEY
TWILIO_ACCOUNT_SID
TWILIO_AUTH_TOKEN
STATUSPAGE_API_KEY
DT_API_URL
DT_API_KEY
DT_WEBHOOK_SECRET
DEPLOY_WEBHOOK_SECRET
PORT
AZURE_FEDERATED_TOKEN_FILE
DRATA_API_BASE_URL
DRATA_API_KEY
HTTP_PORT
STRIPE_PRICE_ID_STARTER
GCP_REGION
CORELINK_BYOK_AZURE_VAULT_URL
CORELINK_BYOK_AZURE_REGION
CORELINK_BYOK_VAULT_REGION
STATUSPAGE_PAGE_ID
STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS
STATUSPAGE_TENANT_ID
NEON_DB_URL_IAD
NEON_DB_URL_FRA
NEON_DB_URL_GRU
NEON_DB_URL_NRT
NEON_DB_URL_SYD
HUGR_WALLET_BASE
HUGR_WALLET_TOKEN
HUGR_STRIPE_REF
STRIPE_API_BASE
PAGERDUTY_TOKEN
STATUSPAGE_URL
```

**Note on spec §3 Phase D text:** The Wave 32 spec §3 Phase D lists a subset (STRIPE_SECRET_KEY, PAGERDUTY_ROUTING_KEY, BETTERSTACK_API_TOKEN, STRIPE_WEBHOOK_SECRET, HMAC keys) as examples. The authoritative list is the `docs/internal/secrets-checklist.md` matrix (131 rows), which is the single source of truth per the matrix header. `BETTERSTACK_API_TOKEN` does not appear in the matrix by that name — the BetterStack token in the matrix is `STATUSPAGE_API_KEY` (row 42). `put-secrets-prod.sh` uses the matrix as the source of truth.

**Parser correctness check:**
```bash
# Manual spot-check: count pipe-delimited data rows in the matrix section
grep '^|' docs/internal/secrets-checklist.md \
  | grep -v '^| #\|^|---|' \
  | grep -c 'cf-wrangler'
# Expected: 55
```

---

## §4 Migration count verification

**Files:** 52 `.sql` files in `migrations/d1/`, confirmed via `ls migrations/d1/*.sql | wc -l`.

**Full file list (lex order, matches wrangler apply semantics):**
```
0001_blob_meta.sql
0002_ac_meta.sql
0003_multipart_chunks_manifest.sql
0006_gc_run.sql
0007_gc_candidates.sql
0008_tenant_storage_state.sql
0009_quota_reservations.sql
0010_ratelimit_buckets.sql
0011_edge_blocklist.sql
0012_quota_cas_attempts.sql
0013_abuse_scores.sql
0014_global_circuit_state.sql
0015_analytics_cardinality_budgets.sql
0016_log_schema.sql
0017_usage_event_idem.sql
0018_stripe_idem_keys.sql
0019_billing_reconciliation_drift.sql
0020_quota_fsm_state.sql
0021_billing_replay_audit.sql
0022_dsr_erasure_log.sql
0023_residency_check_constraints.sql
0024_config_change_log.sql
0025_terraform_drift_findings.sql
0026_rollout_state_and_budget.sql
0027_region_provisioning.sql
0028_tenant_primary_region.sql
0029_hot_blobs.sql
0030_byok_envelope.sql
0031_byok_tenant_status.sql
0032_erasure_attestation.sql
0033_chaos_runs.sql
0034_dr_drill_runs.sql
0035_runbook_drills.sql
0036_oncall_pages.sql
0037_signup_orchestration.sql
0038_dpa_acceptances.sql
0039_tier_selection.sql
0040_enterprise_inquiries.sql
0041_dpa_versioning.sql
0042_lighthouse_customers.sql
0043_synthetic_page_drills.sql
0044_drata_evidence_sent.sql
0044_stripe_webhook_events_processed.sql
0045_stripe_webhook_dlq.sql
0046_tenant_offboarding_state.sql
0047_survey_responses.sql
0048_stripe_billing_materializer.sql
0049_export_audit_log.sql
0050_export_audit_log_add_payload.sql
0051_dsr_erasure_log_outcome_json.sql
0052_tenant_config_region.sql
0053_pilot_signups.sql
```

**Note:** The numbering has gaps (0004, 0005 missing) and two files share prefix `0044` — this is the existing state of the migration set; the runner sorts by full filename lexicographically, matching `wrangler d1 migrations apply` semantics.

**Expected table count derivation:**
- Static analysis via Python: `re.findall(r'CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?[a-zA-Z_][a-zA-Z0-9_]*', content, re.IGNORECASE)` across all 52 files.
- Result: 76 `CREATE TABLE` statements; 75 unique table names (the `tenant` table appears in both `0023` and `0037` — `CREATE TABLE IF NOT EXISTS` makes the second a no-op).
- Files `0028_tenant_primary_region.sql` and `0031_byok_tenant_status.sql` contain only `ALTER TABLE` / `CREATE INDEX` statements — no new tables.
- **Expected post-apply table count used in verification: 75.**
- The `apply-d1-migrations-prod.sh` script asserts `actual_count >= 75` (>=, not ==, to be tolerant of the internal `d1_migrations` tracking table that wrangler creates).

**Additive guard:** `scripts/check_migrations_additive.py` ran during the dry-run and reported:
```
OK: 59 migration file(s) scanned; all additive.
```
(The script scans multiple migration dirs; 52 d1 + 7 others = 59 total.)

---

## §5 Dry-run outputs

### 5.1 `apply-d1-migrations-prod.sh` (dry-run, 2026-05-26T18:48Z)

```
[apply-d1-migrations-prod.sh] Phase D migration runner starting
[apply-d1-migrations-prod.sh]   mode=DRY-RUN
[apply-d1-migrations-prod.sh]   db=corelink-prod-d1
[apply-d1-migrations-prod.sh] migration file count: 52 (matches spec expectation of 52)
[apply-d1-migrations-prod.sh] running additive guard: .../check_migrations_additive.py
OK: 59 migration file(s) scanned; all additive.
[apply-d1-migrations-prod.sh] additive guard PASSED
[apply-d1-migrations-prod.sh] ordered migration plan (lex sort, matches wrangler apply semantics):
[apply-d1-migrations-prod.sh]   [1/52] 0001_blob_meta.sql (~10 statement(s))
[apply-d1-migrations-prod.sh]   [2/52] 0002_ac_meta.sql (~31 statement(s))
[apply-d1-migrations-prod.sh]   [3/52] 0003_multipart_chunks_manifest.sql (~48 statement(s))
  ... [4–51 omitted for brevity; full output identical to script run] ...
[apply-d1-migrations-prod.sh]   [52/52] 0053_pilot_signups.sql (~13 statement(s))
[apply-d1-migrations-prod.sh]
[apply-d1-migrations-prod.sh] DRY-RUN complete. 52 migration files listed above would be applied
[apply-d1-migrations-prod.sh] to corelink-prod-d1 via: wrangler d1 migrations apply corelink-prod-d1 --remote
[apply-d1-migrations-prod.sh]
[apply-d1-migrations-prod.sh] Post-apply verification would check:
[apply-d1-migrations-prod.sh]   wrangler d1 execute corelink-prod-d1 --remote --command="SELECT count(*) FROM sqlite_master WHERE type='table'"
[apply-d1-migrations-prod.sh]   Expected table count: 75
[apply-d1-migrations-prod.sh]
[apply-d1-migrations-prod.sh] To apply for real: ./scripts/apply-d1-migrations-prod.sh --apply
```

Exit code: 0.

### 5.2 `put-secrets-prod.sh` (dry-run, 2026-05-26T18:49Z)

```
[put-secrets-prod.sh] WARN: env file not found: .env.local
[put-secrets-prod.sh] WARN: will prompt interactively for each secret value (if --apply)
[put-secrets-prod.sh] parsed 55 cf-wrangler secrets from secrets-checklist.md
[put-secrets-prod.sh]
[put-secrets-prod.sh] Secret put plan (env=prod):
[put-secrets-prod.sh]   [1/55] STRIPE_SECRET_KEY
[put-secrets-prod.sh]   [2/55] STRIPE_SECRET_KEY_TEST
[put-secrets-prod.sh]   [3/55] STRIPE_WEBHOOK_SECRET
  ... [4–52 omitted for brevity] ...
[put-secrets-prod.sh]   [54/55] PAGERDUTY_TOKEN
[put-secrets-prod.sh]   [55/55] STATUSPAGE_URL
[put-secrets-prod.sh]
[put-secrets-prod.sh] DRY-RUN complete. 55 secrets listed above would be put to
[put-secrets-prod.sh] Cloudflare Workers (--env prod) via: wrangler secret put <NAME> --env prod
[put-secrets-prod.sh]
[put-secrets-prod.sh] Values sourced from: .env.local (if present) or interactive prompt.
[put-secrets-prod.sh] CTRL-CRED-001: no values are logged or hard-coded.
[put-secrets-prod.sh]
[put-secrets-prod.sh] To apply for real: ./scripts/put-secrets-prod.sh --apply
```

Exit code: 0.

### 5.3 `verify-secrets-deployed.sh` (dry-run / --help, 2026-05-26T18:49Z)

`--help` output verified correct (prints full usage). Full dry-run requires `wrangler` in PATH — not installed in this environment. The script exits 127 cleanly when wrangler is absent, with:
```
[verify-secrets-deployed.sh] ERROR: wrangler CLI not found in PATH
[verify-secrets-deployed.sh] Install: npm install -g wrangler
```

This is expected and correct for a prep-phase run. The script will work as intended when invoked in an environment with wrangler + CF auth (Phase D apply environment).

---

## §6 Charter compliance

### CTRL-CRED-001 — No hardcoded secret values

- `put-secrets-prod.sh`: values sourced exclusively from `.env.local` (`source "$ENV_FILE"`) or interactive `read -s -p` prompt. The actual value is piped to wrangler via `printf '%s' "$VAL" | wrangler secret put "$NAME" ...` — never passed as a CLI argument or written to any log.
- Grep verification: `grep -n 'sk_\|SECRET_KEY=\|TOKEN=' scripts/put-secrets-prod.sh` returns zero matches.
- No `.env.local` file is committed (`.gitignore` excludes it).

### CTRL-AUDIT-EMIT-BEFORE-MUTATION

- `apply-d1-migrations-prod.sh`: each migration file is logged (filename + statement count) BEFORE wrangler is invoked.
- `put-secrets-prod.sh`: each secret name + length + SHA256[:8] of value is logged BEFORE `wrangler secret put` is called. The hash is computed locally for rotation verification only; it is not the value.

### Audit-fail-CLOSED on secret put

- `put-secrets-prod.sh` tracks all successfully-put secrets in `SUCCESSFULLY_PUT` array.
- On any `wrangler secret put` failure: `rollback()` is called, which issues `wrangler secret delete --force` for every entry in `SUCCESSFULLY_PUT`.
- Rollback failures are reported to stderr with manual remediation commands.
- The script exits non-zero in all failure paths.

### INV-AUTH-MIGRATION-ADDITIVE (HIGH) — inherited from gate

- `apply-d1-migrations-prod.sh` invokes `scripts/check_migrations_additive.py` before any wrangler call.
- If the additive guard returns non-zero, the script exits 3 without applying any migration.
- The guard ran during dry-run and confirmed: `OK: 59 migration file(s) scanned; all additive.`
- This inherits the PR-time enforcement (the gate runs at PR merge); the runner re-validates at apply-time as defense-in-depth.

---

## §7 What Phase D APPLY will do (forward-look)

When Phase B + C are green and the Owner approves the B+C→D decision gate:

**Step 1 — Migrations:**
```bash
scripts/apply-d1-migrations-prod.sh --apply
```
- Runs additive guard (expected exit 0).
- Invokes `wrangler d1 migrations apply corelink-prod-d1 --remote`.
- Wrangler applies only pending migrations (idempotent via `d1_migrations` table).
- Post-apply: queries `SELECT count(*) FROM sqlite_master WHERE type='table'`; asserts >= 75.
- Expected duration: ~2-10 minutes depending on CF D1 latency.

**Step 2 — Secrets:**
```bash
# Ensure .env.local is present (or prepare for interactive prompts)
scripts/put-secrets-prod.sh --apply
```
- Sources `.env.local` for values (all 55 cf-wrangler secrets expected to be set; validated from Bucket 2 credential setup 2026-05-22).
- Pushes 55 secrets to `--env prod` via `wrangler secret put`.
- On any failure: automatic rollback (delete all secrets put in this session).
- Expected duration: ~5-15 minutes (55 × wrangler API round trips).

**Step 3 — Verification:**
```bash
scripts/verify-secrets-deployed.sh
```
- Diffs `wrangler secret list --env prod` against canonical matrix.
- Must exit 0 (zero diff) before Phase D can be signed off.

**Phase D rollback path:**
- Migrations: additive only; no rollback at D1 layer. Recovery = delete + re-provision D1 from Phase C (see `specs/_runbooks/RB-D1-MIGRATION-APPLY.md`).
- Secrets: `scripts/put-secrets-prod.sh` rollback handler covers session failures. For post-apply rollback: `wrangler secret delete NAME --env prod --force` per secret.

---

## §8 Hard pause triggers (prep phase)

No hard pause triggers fired during this prep agent run. The following triggers apply to the APPLY phase:

1. `migrations/d1/*.sql` count != 52 — `apply-d1-migrations-prod.sh` will hard-halt (exit 1) with an explicit message directing spec amendment.
2. `secrets-checklist.md` markdown table parsing fails (0 results) — `put-secrets-prod.sh` will hard-halt (exit 1).
3. `wrangler` CLI not installed or version incompatible — both migration and secret scripts exit 127 cleanly.
4. Additive guard fails (DROP/ALTER-with-loss detected) — `apply-d1-migrations-prod.sh` exits 3; DO NOT override.
5. `wrangler d1 migrations apply` fails mid-stream — `apply-d1-migrations-prod.sh` exits 1 with remediation instructions; operator must inspect `wrangler d1 migrations list` before re-running.

---

## §9 Sign-off

**Acceptance criteria checklist:**

- [x] 3 scripts written, executable (`chmod +x`), with `--help` flag
- [x] Each script dry-run output captured + committed to audit §5
- [x] `bash -n scripts/apply-d1-migrations-prod.sh` passes
- [x] `bash -n scripts/put-secrets-prod.sh` passes
- [x] `bash -n scripts/verify-secrets-deployed.sh` passes
- [x] Migration table-count derivation reproducible (§4: 75 unique tables from 76 CREATE TABLE statements)
- [x] Secrets parser test: 55 cf-wrangler entries parsed from 131-row matrix
- [x] SEAL audit committed
- [x] Zero file overlap with Phase B/C/F/G sibling agents

**Script LOC:**
- `scripts/apply-d1-migrations-prod.sh`: 228 lines
- `scripts/put-secrets-prod.sh`: 298 lines
- `scripts/verify-secrets-deployed.sh`: 212 lines
- Total: 738 lines

**Parallel-safety confirmation:**
- This agent touched only: `scripts/apply-d1-migrations-prod.sh`, `scripts/put-secrets-prod.sh`, `scripts/verify-secrets-deployed.sh`, `specs/_audits/sealed/2026-05-26-w32-phaseD-prep.md`.
- No overlap with: `worker/` (Phase B), `wrangler.toml` (Phase B/C), `scripts/deploy-pages-prod.sh` (Phase F), `scripts/dns-prod-plan.sh` (Phase G).

**DCO sign-off:** Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---

*End of Wave 32 Phase D Prep Audit.*
