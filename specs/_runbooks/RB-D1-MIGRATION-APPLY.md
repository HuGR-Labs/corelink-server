---
id: "RB-D1-MIGRATION-APPLY"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-14"
updated: "2026-05-27"
owner: "SRE Lead"
final_approver: "SRE Lead"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "r2-13", "d1", "migrations", "schema-change", "deploy"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-d1-migrations` was absorbed into `corelink-ops` via inline `mod migrations;` per SEAL `specs/_audits/sealed/2026-05-26-w35-p2-ops-absorption.md`. Canonical consumer path is now `corelink_ops::migrations::*`. The `cargo test -p corelink-d1-migrations` invocation still resolves for HISTORICAL log/grep reference but new automation should use `cargo test -p corelink-ops --test d1_migration_integration`.

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §4.

# RB-D1-MIGRATION-APPLY — D1 schema migration apply runbook

> **Status:** ACTIVE. Owned by SRE Lead + DBA. Covers the canonical
> staged rollout for any D1 schema migration landing under
> `migrations/d1/`.

## 1. Trigger

A PR that adds or modifies any file under `migrations/d1/` has merged
to `main` and the operator has been asked to apply it.

## 2. Pre-flight (mandatory)

### 2.1 CI gates

Before this runbook runs, the PR MUST have green:

- `additive-check` (INV-AUTH-MIGRATION-ADDITIVE)
- `d1-migration-validate` workflow:
  - `scripts/check_migrations_additive.py`
  - `scripts/d1-migration-verify.py --schema-only`
  - `cargo test -p corelink-d1-migrations --test d1_migration_integration`
  - Runner shell-script dry-run smoke

If any of those failed, do NOT proceed. Open a follow-up PR to fix.

### 2.2 D1 database backup → R2 snapshot

D1 does NOT support in-place rollback (see §5 below). The only
recoverable rollback path is **restore from an owner-provisioned R2
snapshot**. The root Worker configuration has no staging D1 binding, so
this runbook intentionally does not publish an implicit staging export,
bucket, or upload command. A separately provisioned external staging
system must provide its own reviewed target/configuration and backup
procedure before an operator runs it; that procedure is outside this
repository's root-worker contract.

For the shipped root Worker, production D1 backup and restore procedures
must name an explicitly provisioned production target and R2 bucket. Do
not substitute `--env staging`, a guessed database name, or a guessed
bucket: the runner rejects that root environment and a missing target must
fail closed.

### 2.3 Operator sanity

- Confirm Cloudflare auth: `wrangler whoami` shows the correct
  account.
- Confirm correct `wrangler.toml` is in scope (no stray local edits).
- Read the PR description for any migration-specific caveats (e.g.
  "the column default is NULL, intentional").

## 3. Apply procedure (dev → prod canary)

> **Repository boundary (B-070):** the root Worker configuration intentionally
> has no `[env.staging]`, and the production workflow has no staging deploy leg.
> This root-worker runbook intentionally provides no staging command. Any
> separately provisioned external staging system needs its own reviewed config
> and runbook; it is not a shipped staging → prod promotion flow.

### 3.1 Dev

```bash
scripts/d1-migration-runner.sh --env dev
# log: logs/d1-apply-dev-<ts>.log
```

Verify exit code 0. Inspect the log for any `wrangler list` diff that
shows unexpected pending migrations.

### 3.2 External staging (not a root-worker target)

This root-worker runbook does not apply migrations to staging. Do not pass
`--env staging` to `scripts/d1-migration-runner.sh`: the wrapper rejects it
because the root `wrangler.toml` has no such environment. An owner-provisioned
external staging system must supply and validate its own Wrangler configuration
and migration procedure before use; that work is outside B-070.

### 3.3 Production canary

Production requires the defense-in-depth opt-in flag:

```bash
scripts/d1-migration-runner.sh \
    --env prod \
    --i-understand-this-is-prod
```

After apply:

1. Re-run §3.2 verify against a prod snapshot.
2. Watch the canary dashboards for 30 min (error rate, p95
   latency, 5xx). Specific dashboards:
   - `corelink-d1-write-error-rate` (SLO budget: < 0.05%)
   - `corelink-blob-meta-insert-p95` (SLO budget: < 50ms)
3. If any SLO budget is breached → §5 rollback.

## 4. Idempotency

Re-running the apply script is safe:

- `wrangler d1 migrations apply` tracks applied migrations in a
  `d1_migrations` table — already-applied entries are no-ops.
- Every CoreLink migration uses
  `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS` /
  `ADD COLUMN IF NOT EXISTS`, so even forced re-apply is safe.
- The runner script is itself idempotent (no destructive side
  effects beyond appending to `logs/`).

## 5. Rollback

> **D1 does NOT support in-place rollback.** Per
> INV-AUTH-MIGRATION-ADDITIVE (HIGH), every migration is additive;
> there is no DROP. Rolling back requires database restore.

### 5.1 Decision tree

| Symptom                                  | Action                          |
|------------------------------------------|---------------------------------|
| New column not yet read by any code      | No rollback. Migration is dormant. |
| New column read but write path fails     | Hotfix the writer; do NOT touch D1. |
| Table-level corruption                   | §5.2 restore-from-snapshot.    |
| Trigger raising RAISE(ABORT) on writes   | Apply a 000X_disable_trigger migration that drops the trigger via the additive-allowed path. |

### 5.2 Restore from R2 snapshot

> SEV-2. Requires SRE Lead + DBA dual-approval before running.
> Causes a read-only window for the region during restore.

```bash
# 1. Stop writes to the D1 binding (kill switch via wrangler):
#    Set tenant.byok_status='degraded_read_only' globally (this is
#    overly broad but is the canonical write-block lever for D1).
#    See RB-BYOK-REVOCATION for the lever mechanics.

# 2. Restore the snapshot.
SNAPSHOT="$(aws s3 ls s3://corelink-d1-backup-prod/wnam/ \
    --endpoint-url "$R2_S3_ENDPOINT" | sort | tail -1 | awk '{print $4}')"
aws s3 cp "s3://corelink-d1-backup-prod/wnam/${SNAPSHOT}" \
    /tmp/restore.sqlite --endpoint-url "$R2_S3_ENDPOINT"

# 3. Re-import into D1 (this drops + recreates — see Cloudflare docs
#    on d1 import). Note: this requires a brief read-only window.
wrangler d1 execute corelink_d1_wnam --env prod \
    --file /tmp/restore.sqlite --remote

# 4. Re-enable writes.
# 5. Open a postmortem within 24h (RB-POSTMORTEM-PROCESS).
```

## 6. Observability

Every invocation of `scripts/d1-migration-runner.sh` appends to
`logs/d1-apply-<env>-<ts>.log`. Logs are retained 90d. Never log
migration SQL bodies (the runner strips them; the script only logs
filenames + wrangler exit codes).

The `d1_migrations` table inside each D1 binding is the canonical
source of truth for which migrations have been applied. Inspect with:

```bash
wrangler d1 execute corelink_d1_wnam --env prod \
    --command "SELECT * FROM d1_migrations ORDER BY id;" --remote
```

## 7. Owner + contacts

- **Primary owner:** SRE Lead (PagerDuty rotation `sre-tier-1`).
- **Co-owner:** DBA (PagerDuty rotation `dba-tier-1`).
- **Escalation:** Engineering Director on SEV-2 escalation.
- **Doc maintenance:** review every 6 months or after any D1 schema
  regression incident.

## 8. Cross-references

- `scripts/d1-migration-runner.sh` — canonical apply wrapper.
- `scripts/d1-migration-verify.py` — schema-drift detector.
- `crates/corelink-d1-migrations/` — Rust replay harness (in-memory).
- `.github/workflows/d1-migration-validate.yml` — PR gate.
- `specs/03_architecture/d1-schema-evolution.md` — per-migration
  registry.
- `specs/03_architecture/invariant_registry.md`
  INV-AUTH-MIGRATION-ADDITIVE.
- `specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md`.
