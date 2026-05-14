# Production Secrets Runbook (WI-R2-14)

> **Last sealed:** 2026-05-14
> **Owner:** SRE Lead (co-owned with Security Lead)
> **Companion:** `docs/internal/secrets-checklist.md` (the matrix of all secrets)

This runbook covers the **operational** side of secrets management:

1. Initial population
2. Rotation procedure (manual + scheduled)
3. Compromise response (incident playbook)
4. Audit trail

All secret-name references below come from `secrets-checklist.md`. Read that
matrix first.

## 1. Initial population

### Cloudflare Workers (`cf-wrangler` tier)

Every row marked `cf-wrangler` in the matrix is populated via:

```bash
# One-shot per secret. The command prompts stdin for the value;
# DO NOT pass via CLI args (would land in shell history).
wrangler secret put STRIPE_SECRET_KEY        --env prod
wrangler secret put STRIPE_WEBHOOK_SECRET    --env prod
wrangler secret put CLERK_SECRET_KEY         --env prod
wrangler secret put PAGERDUTY_ROUTING_KEY    --env prod
wrangler secret put SLACK_WEBHOOK_URL_ALERTS_SEV1            --env prod
wrangler secret put SLACK_WEBHOOK_URL_ALERTS_SEV2            --env prod
wrangler secret put SLACK_WEBHOOK_URL_ENTERPRISE_INQUIRIES   --env prod
wrangler secret put SLACK_WEBHOOK_URL_BREACH_NOTIFICATIONS   --env prod
wrangler secret put SLACK_WEBHOOK_URL_ONCALL_HANDOFF         --env prod
wrangler secret put SLACK_WEBHOOK_URL_LIGHTHOUSE_CUSTOMERS   --env prod
wrangler secret put HUBSPOT_PRIVATE_APP_TOKEN                --env prod
wrangler secret put GOOGLE_APPLICATION_CREDENTIALS           --env prod
wrangler secret put AZURE_TENANT_ID         --env prod
wrangler secret put AZURE_CLIENT_ID         --env prod
wrangler secret put AZURE_CLIENT_SECRET     --env prod
wrangler secret put SENDGRID_API_KEY        --env prod
wrangler secret put TWILIO_ACCOUNT_SID      --env prod
wrangler secret put TWILIO_AUTH_TOKEN       --env prod
wrangler secret put STATUSPAGE_API_KEY      --env prod
wrangler secret put DT_API_URL              --env prod
wrangler secret put DT_API_KEY              --env prod
wrangler secret put DT_WEBHOOK_SECRET       --env prod
wrangler secret put DEPLOY_WEBHOOK_SECRET   --env prod
wrangler secret put PAGERDUTY_SYNTHETIC_ROUTING_KEY  --env prod
```

**Verify population** (does NOT print values):

```bash
wrangler secret list --env prod | sort
```

The output must include all secrets the `cf-deploy-prod.yml` gate expects.
Compare against the matrix.

### GitHub Actions (`gha-secret` tier)

Use the GitHub UI: Repository Settings → Secrets and variables → Actions → New
repository secret.

For environment-scoped secrets (e.g., production deploy):
Settings → Environments → `production` → Add secret.

**Critical**: When adding multi-line secrets like `.p12` certs or PEM keys,
base64-encode FIRST:

```bash
# .p12 (Apple Developer ID cert):
base64 -w0 ./CoreLinkDeveloperID.p12 | pbcopy
# Paste into GHA secret value field.

# .pfx (Windows EV cert) — same pattern.
base64 -w0 ./CoreLinkEV.pfx | pbcopy

# GPG private key (ASCII-armored already; just pipe):
gpg --export-secret-keys --armor "${KEY_ID}" | pbcopy
```

### Vercel admin-ui (`vercel-env` tier)

```bash
# Public (browser-bundled) keys:
vercel env add NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY production
vercel env add NEXT_PUBLIC_CORELINK_API_URL      production
vercel env add COOKIEBOT_DOMAIN_GROUP_ID         production
# Server-side keys (NEVER expose in NEXT_PUBLIC_*):
vercel env add CLERK_SECRET_KEY                  production
```

## 2. Rotation procedure

### 2.1 Scheduled rotation (per `Rotation cadence` column)

The matrix's `Rotation cadence` column is monitored by a CI job
(`scripts/secrets_rotation_audit.py` — TBD). For each secret with a cadence ≤
day-precision, the job checks the last-rotated timestamp recorded in the audit
trail (D1 `audit_outbox` table) and pages the rotation owner T-14d before
expiry.

**Manual rotation playbook (any secret)**:

1. **Pre-rotate** — generate new credential at vendor (do NOT revoke old yet).
2. **Stage** — push new value to a `STAGE_` prefixed env var:
   ```bash
   wrangler secret put STAGE_STRIPE_SECRET_KEY --env prod
   ```
3. **Canary** — deploy a 1% canary that consumes `STAGE_` first, falls back to
   the live var. Run for 30 min; check `corelink-stripe-real` error rates.
4. **Promote** — overwrite the live var:
   ```bash
   wrangler secret put STRIPE_SECRET_KEY --env prod
   ```
5. **Verify** — run `scripts/secrets-checklist-verify.sh` + smoke tests.
6. **Revoke old** — at the vendor.
7. **Clean up** — `wrangler secret delete STAGE_STRIPE_SECRET_KEY --env prod`.
8. **Record** — emit a `secret.rotated` event to D1 `audit_outbox` with
   `{secret_name, rotator_actor, rotated_at, prev_kid, new_kid}`.

### 2.2 Auto-tested rotation (high-value secrets)

For `STRIPE_SECRET_KEY`, `PAGERDUTY_ROUTING_KEY`, and `CLERK_SECRET_KEY`,
production deploys an **out-of-band rotation drill** monthly: the SRE team
rotates the secret in staging following the playbook above and verifies the
canary path works end-to-end. Failures block the production rotation until the
playbook is fixed. See `RB-SECRETS-ROTATION-DRILL.md` (TBD).

### 2.3 Roll-forward (no rollback)

We never roll back secret rotations. If a new credential breaks production,
generate a NEW credential (not the old one) and roll forward. This avoids
re-introducing potentially-compromised material.

## 3. Compromise response

If any production secret is suspected compromised (leaked in logs, exposed in
a public repo, lost device, etc.), execute this playbook:

### Timeline

| T+ | Action | Owner |
|---|---|---|
| 0min | **Detect** — incident declared (PD page or manual). Page SRE Lead + Security Lead via `#alerts-sev1`. | Detector |
| +5min | **Revoke** at vendor (NOT just rotate — explicit revocation). For Stripe: delete key at dashboard. For Slack: revoke webhook at app config. For AWS: deactivate + schedule deletion of access key. | Rotation owner per matrix |
| +10min | **Generate new** credential at vendor. | Rotation owner |
| +15min | **Deploy** new value via `wrangler secret put` (or GHA secret update + redeploy). Skip canary — this is the compromise path, accept brief downtime over compromise. | Rotation owner |
| +30min | **Audit** — pull last 24h of activity for the compromised credential from the vendor's audit log. Cross-reference against CoreLink audit_outbox for replay. | Security Lead |
| +60min | **Replay** — if the secret was used to write/sign (Stripe charges, webhook deliveries), run the corresponding replay tool (`corelink-billing-replay`, etc.). | Rotation owner |
| +4h | **Postmortem draft** — 5-whys for how the credential leaked. | Security Lead |
| +72h | **Postmortem ship** — public summary if customer-impacting. | Security Lead |

### Per-secret compromise notes

- **`STRIPE_SECRET_KEY`** (#1): After revoke, immediately switch
  `corelink-billing-aggregator` to read-only mode until rotation completes.
  Replay last 24h via `corelink-billing-replay --since 24h`.
- **`STRIPE_WEBHOOK_SECRET`** (#3): Re-fetch missed webhooks via Stripe's
  `events.list` API with `created[gte]=T-24h`. Stripe retains 30d of events.
- **`PAGERDUTY_ROUTING_KEY`** (#11): Recreate integration. After rotation,
  fire a synthetic page via `corelink-synthetic-pager` to verify the new
  routing key delivers.
- **`CLERK_SECRET_KEY`** (#6): Rotation invalidates ALL active sessions.
  Banner the admin-ui ("Please re-authenticate") before rotation.
- **GPG / Apple / Windows code-signing** (#55–#64): If a signing key
  is compromised, ALL released binaries signed since the suspected compromise
  time MUST be considered untrusted. Generate new key, re-sign current
  release, publish a security advisory referencing affected release SHAs.
- **AWS/GCP/Azure BYOK staging creds** (#23, #24, #28, #32): These are
  PER-CUSTOMER scoped. After rotation, notify affected customers; their
  trust path may include the staging IAM principal.
- **Vault AppRole** (#33, #34): Customer's responsibility — notify the
  customer's security contact; they revoke the role-id + secret-id.

### Public disclosure threshold

If a secret compromise causes customer-data exposure or service degradation,
follow the `RB-BREACH-NOTIFICATION` runbook (72-hour GDPR clock + DPO
notification + Statuspage incident).

## 4. Audit trail

### 4.1 Cloudflare audit log

Every `wrangler secret put` emits a Cloudflare account audit log entry:

```
event.type   = "workers.secret.update"
event.target = "STRIPE_SECRET_KEY"
event.actor  = <CF user email>
event.at     = <ISO-8601>
```

Cloudflare retains these for 18 months.

### 4.2 D1 `audit_outbox` mirror

We mirror every secret-mutation event to D1 (`audit_outbox` table, see
`docs/internal/d1-schema-ac-meta.md`) via a webhook from the deploy pipeline:

```json
{
  "kind": "secret.rotated",
  "secret_name": "STRIPE_SECRET_KEY",
  "actor": "alice@humangr.com",
  "rotated_at": "2026-05-14T18:32:00Z",
  "rotation_reason": "scheduled-90d",
  "prev_secret_id_hash": "blake3:abc...",
  "new_secret_id_hash":  "blake3:def..."
}
```

The hash is over a deterministic identifier (e.g., last-4 chars + key ID), NOT
the secret value itself.

### 4.3 GitHub Actions audit log

GHA secret updates appear in the repository audit log (Settings → Audit log).
Filter on `action:org.update_actions_secret` or `action:repo.update_actions_secret`.

### 4.4 Reconciliation

Monthly, run `scripts/secrets_audit_reconcile.py` (TBD) which:

1. Lists CF Worker secrets (`wrangler secret list --env prod`).
2. Lists GHA secrets (via `gh api`).
3. Cross-references against `secrets-checklist.md`.
4. Reports drift to `#security-findings`.

## 5. Quick reference

### Where do I rotate X?

| Secret family | Where | Owner |
|---|---|---|
| Stripe | `wrangler secret put` + Stripe dashboard | SRE Lead |
| Clerk | `wrangler secret put` + Clerk dashboard + Vercel env | DevOps |
| PagerDuty | `wrangler secret put` + PagerDuty integration | SRE Lead |
| Slack webhooks | `wrangler secret put` + Slack app config | SRE Lead |
| HubSpot | `wrangler secret put` + HubSpot private apps | Sales Ops |
| AWS/GCP/Azure BYOK | GHA secret + cloud IAM console | Security Lead |
| Vault AppRole | Customer-side | Customer |
| Code-signing (GPG, Apple, Windows) | GHA secret + vendor portal | Release Manager |
| Cloudflare API token | GHA secret + CF profile | SRE Lead |

### Who do I page?

- Runtime secret compromise → `#alerts-sev1` (auto-pages SRE Lead + Security Lead)
- Code-signing compromise → `#breach-notifications` (Release Manager + Security Lead)
- BYOK customer-side compromise → contact customer security
