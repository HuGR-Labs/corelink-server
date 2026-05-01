---
id: "RB-FM-AC-BUCKET-LEAK"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-05-01"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "data-leak", "r2", "ac", "appsec"]
---

# RB-FM-AC-BUCKET-LEAK — R2 `corelink-ac-<region>` ACL Drift / Public Access Detected

> **FM:** R2 bucket public-access drift; envelope contents (tenant_id + command_line + env_vars) potentially exposed | **SLA:** mitigate < 1 h | **CRITICAL** — confirmed leak fires breach notification.

## Quando aplicar

- `.github/workflows/ac-bucket-acl-cron.yml` audit step prints `Bucket … CORS rules NOT empty`.
- Customer report de objeto AC accessible via public URL.
- AppSec scanner relata public R2 endpoint.
- `scripts/check_ac_infra.sh <env>` falha em produção.

## Detecção

- Cron audit failure: GitHub Actions notification fires (PagerDuty wiring lands S-09).
- Métrica `corelink.r2.ac_bucket.acl_drift{region}` > 0 (alert SEV-1 — emit landed S-09 §6.1.9).
- Customer-side report (raríssimo; envelope é signed mas plaintext).

## Comunicação

- **SEV-1 imediato** se confirmed drift (page Security + AppSec + Privacy + Architect).
- Status page: do **NOT** announce until exposure scope confirmed (premature disclosure → customer panic).
- **DO NOT** assume false positive — even 1 h of public access is breach territory under LGPD Art. 38 / GDPR Art. 32.

## Mitigação imediata (≤ 1 h)

### Step 1 — Re-assert empty CORS via Cloudflare REST API

```bash
for region in sam iad lhr nrt syd; do
  bucket="corelink-ac-${region}"
  curl --silent --show-error --fail-with-body \
    -X PUT "https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT_ID}/r2/buckets/${bucket}/cors" \
    -H "Authorization: Bearer ${CF_API_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"rules": []}'
done
```

The cron workflow `.github/workflows/ac-bucket-acl-cron.yml` includes
a `Re-assert canonical empty-CORS posture (self-heal)` step that runs
on failure. If the workflow already self-healed, jump to Step 2.

### Step 2 — Verify public access OFF

```bash
# Cloudflare dashboard does not expose Public R2 URLs unless explicitly
# enabled per-bucket. Confirm via API:
for region in sam iad lhr nrt syd; do
  bucket="corelink-ac-${region}"
  curl --silent --show-error \
    -X GET "https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT_ID}/r2/buckets/${bucket}/domains/managed" \
    -H "Authorization: Bearer ${CF_API_TOKEN}" \
  | python3 -c "import json, sys; d=json.loads(sys.stdin.read()); print(d.get('result', d))"
done
```

If `enabled: true` on any managed domain, **disable immediately** via
the dashboard OR API (`PUT … /domains/managed -d '{"enabled": false}'`).

### Step 3 — Quarantine tenants whose envelopes may have leaked

If forensics identifies a leak window:
1. Snapshot affected tenants from `ac_meta` (`SELECT tenant_id, action_digest, created_at FROM ac_meta WHERE region = ? AND created_at >= ?`).
2. Cross-reference with R2 audit log (Cloudflare Logpush) to identify which envelopes were fetched during the public window.
3. Mark affected tenant_ids for elevated audit + customer notification scope.

## Mitigação completa (≤ 4 h)

1. Patch the root cause (CI gate gap, manual dashboard change, IAM policy regression).
2. Add regression test: cron workflow OR new `scripts/check_ac_infra.sh` step that asserts public access OFF.
3. Confirm zero ongoing drift via `scripts/check_ac_infra.sh production` exits 0.
4. Enable elevated bucket logging (Cloudflare Logpush → R2 access log) for ≥ 30 days.

## Forensics

1. Cloudflare audit log: who toggled the ACL? When?
2. R2 access logs (Logpush): list every requestor IP + timestamp during the leak window.
3. Cross-reference envelope content: were any successfully fetched? If yes, decode + identify tenant impact.
4. PAT audit chain: any concurrent PAT abuse? (Cross-link RB-FM-160).

## Notificação obrigatória

- **Tenants vítima**: dado pode ter sido exposto; envelope contém `tenant_id`, `command_line` (PII risk), `env_vars` (secrets risk se customer mis-set).
  - Email formal + DPA reference; recommend customer rotate any secrets that may have been in env_vars.
- **DPA / ANPD**: 72 h notification window (LGPD Art. 38) **if confirmed exposure**; default to "notify" if any uncertainty.
- **Internal**: SEV-1 post-mortem; root cause + ADR if governance gap.

## Anti-scope (NEVER do these)

- ❌ Wait > 1 h to remediate "to investigate" — re-assert empty CORS first, investigate later.
- ❌ Delete the bucket — data loss; envelope contents may be needed for tenant remediation.
- ❌ Rotate ALL HKDF tenant keys reflexively — only required if envelope SIGNATURES were forged (separate FM); ACL leak alone is read-only exposure.
- ❌ Skip customer notification because "envelope is signed" — the signature is integrity, not confidentiality; envelope contents are plaintext.

## Post-mortem

5-Why mandatory. Output: ADR amendment if governance gap; new automated gate if process gap; customer comms template if notification cadence drift.

## Cross-references

- `.github/workflows/ac-bucket-acl-cron.yml` — nightly audit + self-heal.
- `scripts/provision_ac_buckets.sh` — initial provisioning + CORS hardening.
- `scripts/check_ac_infra.sh` — pre-deploy guard.
- `RB-BREACH-NOTIF` — breach notification cadence.
- `RB-FM-303` — AC cross-tenant (sibling; data integrity vs confidentiality).
- `RB-FM-AC-MIGRATION-BUG` — sibling runbook for D1 schema bugs.
- `specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md` — governance.
