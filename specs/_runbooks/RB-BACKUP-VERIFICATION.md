---
id: "RB-BACKUP-VERIFICATION"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "r6-prep", "backup-verification", "dr", "monthly-cadence"]
---

# RB-BACKUP-VERIFICATION — Monthly Backup Verification

> **Status:** ACTIVE. Owned by SRE Lead. Cadence: monthly, first Monday
> of each calendar month. Validates that `scripts/backup-daily.sh`
> artifacts are restorable end-to-end against an ephemeral staging
> instance.

## 1. Purpose

Backups that aren't periodically restored aren't backups. This runbook
exists so that the GA Engineering Gate (criterion C10 — DR drill
semestral) is reinforced by a **monthly** restore-and-smoke cycle that
catches silent corruption / key-rotation breakage / GPG recipient drift
**before** a real incident requires the restore path.

## 2. Trigger

- **Scheduled:** first Monday of the month, 09:00 UTC.
- **Ad-hoc:** any time after a backup script change merges
  (`scripts/backup-daily.sh` or `scripts/restore-from-snapshot.sh`).
- **Mandatory:** after a GPG key rotation (`BACKUP_GPG_RECIPIENT`
  changes) — a verification cycle MUST run within 24h.

## 3. Pre-flight

- [ ] SRE Lead identified as facilitator.
- [ ] Ephemeral staging instance provisioned (separate namespace from
      `staging` to avoid contaminating the 30d-staging window).
- [ ] `BACKUP_GPG_RECIPIENT` private key present on the runner.
- [ ] Pick a random snapshot from the last 30 days
      (`scripts/restore-from-snapshot.sh` accepts `--snapshot YYYY-MM-DD`).

## 4. Procedure

### Step 1 — Pick snapshot

```bash
LAST_30_DAYS=$(seq 0 29 | shuf -n 1)
SNAPSHOT_DATE=$(date -u -d "${LAST_30_DAYS} days ago" +%Y-%m-%d \
  2>/dev/null || date -u -v-"${LAST_30_DAYS}"d +%Y-%m-%d)
```

### Step 2 — Provision ephemeral staging

```bash
export CORELINK_ENV="staging-verify-$(date -u +%Y%m%d)"
# Allocate D1 databases + R2 bucket + KV namespace bindings under the
# `corelink-verify-*` naming prefix.
./scripts/provision_ac_buckets.sh --env "${CORELINK_ENV}"
```

### Step 3 — Restore

```bash
./scripts/restore-from-snapshot.sh \
    --snapshot "${SNAPSHOT_DATE}" \
    --env "${CORELINK_ENV}" \
    --skip-byok-check
```

Expected: exit 0; elapsed < 30 min.

### Step 4 — Smoke tests

Run, in order:

1. **Signup smoke** — create a synthetic tenant, assert that the
   tenant row lands in `corelink_core.tenants`.
2. **CAS write smoke** — write a 4 KiB blob via the CAS API, assert
   the SHA-256 returned matches the local computation.
3. **Audit chain smoke** — run `python3 scripts/verify_audit_chain.py
   --emit-root` and confirm the root parses (Merkle root from the
   snapshot day need not match because the ephemeral instance now
   has fresh writes; what matters is **chain validity**).

If any smoke test fails → **SEV-2** + restore from a different snapshot
+ if that also fails → **SEV-1** (backups unrestorable).

### Step 5 — Tear down

```bash
# Drop ephemeral D1 databases, R2 bucket, KV namespaces.
wrangler d1 delete corelink_core_verify --skip-confirmation || true
wrangler r2 bucket delete corelink-cold-"${CORELINK_ENV}" --remote || true
```

### Step 6 — Report

Commit a verification report to
`specs/_audits/YYYY-MM-DD-backup-verification.md` with:

- `snapshot_date` selected.
- restore elapsed (target ≤ 30 min).
- smoke test results (signup / CAS write / audit chain).
- pass/fail verdict.
- any drift discovered (GPG key, bucket name, retention policy).

## 5. Ownership + cadence

| Attribute | Value |
|---|---|
| Owner | SRE Lead |
| Cadence | Monthly, first Monday 09:00 UTC |
| SLA | Complete cycle ≤ 2h wall |
| Escalation | SEV-2 on smoke fail; SEV-1 if two consecutive snapshots fail |
| Retention | 7 years (per Quality Standard 14.s17.7) |

## 6. Failure modes

| Mode | Detection | Mitigation |
|---|---|---|
| GPG recipient drift | restore decrypt fails | rotate key, re-encrypt last 30d, restart cycle |
| Wrangler API broken | restore exits non-zero | open Cloudflare ticket; do NOT mark backups untrustworthy yet |
| Manifest checksum mismatch | restore aborts in Phase 1 | escalate SEV-1, audit prior backups |
| Smoke fail on CAS write | step 4.2 | usually staging infra; not backup-related |

## 7. References

- `scripts/backup-daily.sh`
- `scripts/restore-from-snapshot.sh`
- `tests/dr/dr-drill-cycle-2.sh` (D1 corruption + restore cycle)
- `.github/workflows/dr-drill-monthly.yml`
- AUDIT-S20-30D-STAGING-EVIDENCE C10 (DR drill semestral)

---

**Fim RB-BACKUP-VERIFICATION.**
