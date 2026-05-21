# Production Secrets Runbook (WI-R2-14)

> **Last sealed:** 2026-05-21 (wave-31 dual-mode auth — Stripe client now supports BOTH direct and wallet-broker auth, switched by `STRIPE_AUTH_MODE`; see `specs/_audits/2026-05-16-wallet-broker-stripe.md §10`)
> **Owner:** SRE Lead (co-owned with Security Lead)
> **Companion:** `docs/internal/secrets-checklist.md` (the matrix of all secrets)

## Stripe auth modes

> **DEFAULT (2026-05-21):** Stripe runs in `direct` mode. The HuGR Wallet
> broker is temporarily unavailable for operational reasons, so CoreLink
> calls Stripe directly with `STRIPE_SECRET_KEY` until further notice.
> Flip back to `wallet-broker` (see column 2 below) once the broker is
> restored — there is NO automatic fallback; the mode must be set
> explicitly via `STRIPE_AUTH_MODE`.

The `crates/corelink-stripe-real` HTTPS client supports two auth modes,
selected per process via the `STRIPE_AUTH_MODE` env var. Both modes wrap
every credential in a redacting `SecretString`; the `Debug` impls on
`StripeClientConfig` / `StripeAuthMode` / `StripeRealClient` redact every
secret. Webhook signature verification stays direct in both modes (see
`STRIPE_WEBHOOK_SECRET` below) — webhooks are inbound HMAC against a
local key, no upstream credential involved.

| Direct mode (`STRIPE_AUTH_MODE=direct`, default) | Wallet-broker mode (`STRIPE_AUTH_MODE=wallet-broker`) |
|---|---|
| `STRIPE_API_BASE` (optional; default `https://api.stripe.com`) | `HUGR_WALLET_BASE` (optional; default `https://api.humangr.com`) |
| `STRIPE_SECRET_KEY` (REQUIRED; `sk_live_…` or `sk_test_…`) | `HUGR_WALLET_TOKEN` (REQUIRED; `hugrw_…` proxy token) |
| (no ref — points straight at Stripe) | `HUGR_STRIPE_REF` (optional; default `stripe-prod`) |
| URL shape: `{api_base}/v1/…` | URL shape: `{wallet_base}/_wallet/proxy/{stripe_ref}/v1/…` |
| Auth: `Authorization: Bearer sk_…` | Auth: `Authorization: Bearer hugrw_…` |
| CoreLink HOLDS the upstream Stripe key. | CoreLink does NOT hold the upstream key; wallet holds it in KV. |
| Rotation: standard Stripe dashboard → `wrangler secret put STRIPE_SECRET_KEY`. | Rotation of `hugrw_`: wallet UI → `wrangler secret put HUGR_WALLET_TOKEN`. Rotation of `sk_live_…`: wallet-owner-side, decoupled from CoreLink. |

Both modes fail CLOSED: a missing required env var → `StripeError::Authentication`
with a mode-specific message; an upstream 5xx after retries exhaust →
`StripeError::Generic { http_status: 5xx, .. }`. There is NEVER a silent
fallback to the OTHER mode (would expose the upstream key through the
broker, or vice versa).

`STRIPE_AUTH_MODE` accepts both `wallet-broker` (kebab-case) and
`wallet_broker` (snake-case) for deploy-template friendliness; any other
value → `StripeError::Authentication` naming the rejected value.

See `specs/_audits/2026-05-16-wallet-broker-stripe.md §10` (dual-mode
addendum) for the full enum shape + test net.

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
#
# Wave-31 dual-mode auth (2026-05-21): the Stripe client supports
# BOTH direct (default while the HuGR Wallet broker is temporarily
# unavailable) and wallet-broker auth, switched by `STRIPE_AUTH_MODE`.
# Populate the secrets for the mode you intend to run:
#   - DEFAULT (`STRIPE_AUTH_MODE=direct`):       STRIPE_SECRET_KEY
#   - When wallet is restored (`STRIPE_AUTH_MODE=wallet-broker`):
#                                               HUGR_WALLET_TOKEN
# Both modes always need STRIPE_WEBHOOK_SECRET (inbound HMAC, local).
# See `specs/_audits/2026-05-16-wallet-broker-stripe.md §10`.
wrangler secret put STRIPE_SECRET_KEY        --env prod   # sk_live_... — DEFAULT mode (2026-05-21 onward)
wrangler secret put HUGR_WALLET_TOKEN        --env prod   # hugrw_... — set only when STRIPE_AUTH_MODE=wallet-broker
wrangler secret put STRIPE_WEBHOOK_SECRET    --env prod   # whsec_... (inbound HMAC verify — direct in BOTH modes)
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
2. **Stage** — push new value to a `STAGE_` prefixed env var. The Stripe
   secret to rotate depends on the active `STRIPE_AUTH_MODE`:
   ```bash
   # Direct mode (DEFAULT, 2026-05-21 onward): rotate STRIPE_SECRET_KEY.
   wrangler secret put STAGE_STRIPE_SECRET_KEY --env prod

   # Wallet-broker mode: rotate HUGR_WALLET_TOKEN.
   wrangler secret put STAGE_HUGR_WALLET_TOKEN --env prod
   ```
3. **Canary** — deploy a 1% canary that consumes `STAGE_` first, falls back to
   the live var. Run for 30 min; check `corelink-stripe-real` error rates.
4. **Promote** — overwrite the live var (same name as the secret you staged):
   ```bash
   # Direct mode:
   wrangler secret put STRIPE_SECRET_KEY --env prod

   # Wallet-broker mode:
   wrangler secret put HUGR_WALLET_TOKEN --env prod
   ```
5. **Verify** — run `scripts/secrets-checklist-verify.sh` + smoke tests.
6. **Revoke old** — at the vendor:
   - Direct mode: revoke the prior `sk_…` key in the Stripe dashboard.
   - Wallet-broker mode: revoke the prior `hugrw_` token in the wallet UI.
   - For other direct-vendor secrets, revoke at the vendor dashboard.
7. **Clean up** — `wrangler secret delete STAGE_<NAME> --env prod`.
8. **Record** — emit a `secret.rotated` event to D1 `audit_outbox` with
   `{secret_name, rotator_actor, rotated_at, prev_kid, new_kid}`.

### 2.2 Auto-tested rotation (high-value secrets)

For the Stripe credential of the currently-active mode (`STRIPE_SECRET_KEY`
in direct mode, `HUGR_WALLET_TOKEN` in wallet-broker mode),
`PAGERDUTY_ROUTING_KEY`, and `CLERK_SECRET_KEY`, production deploys an
**out-of-band rotation drill** monthly: the SRE team rotates the secret in
staging following the playbook above and verifies the canary path works
end-to-end. Failures block the production rotation until the playbook is
fixed. See `RB-SECRETS-ROTATION-DRILL.md` (TBD).

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
| +5min | **Revoke** at vendor (NOT just rotate — explicit revocation). Stripe-direct (`STRIPE_SECRET_KEY`, default mode 2026-05-21 onward): revoke the leaked `sk_…` key in the Stripe dashboard. Wallet-broker (`HUGR_WALLET_TOKEN`): revoke the `hugrw_` token in the HuGR Wallet UI (instant; CoreLink does not hold the upstream Stripe key in that mode — wallet owner rotates `sk_live_…` separately inside the wallet KV). For Slack: revoke webhook at app config. For AWS: deactivate + schedule deletion of access key. | Rotation owner per matrix |
| +10min | **Generate new** credential at vendor. | Rotation owner |
| +15min | **Deploy** new value via `wrangler secret put` (or GHA secret update + redeploy). Skip canary — this is the compromise path, accept brief downtime over compromise. | Rotation owner |
| +30min | **Audit** — pull last 24h of activity for the compromised credential from the vendor's audit log. Cross-reference against CoreLink audit_outbox for replay. | Security Lead |
| +60min | **Replay** — if the secret was used to write/sign (Stripe charges, webhook deliveries), run the corresponding replay tool (`corelink-billing-replay`, etc.). | Rotation owner |
| +4h | **Postmortem draft** — 5-whys for how the credential leaked. | Security Lead |
| +72h | **Postmortem ship** — public summary if customer-impacting. | Security Lead |

### Per-secret compromise notes

- **`STRIPE_SECRET_KEY`** (Stripe direct mode, 2026-05-21 default):
  Revoke the `sk_…` key in the Stripe dashboard immediately. Switch
  `corelink-billing-aggregator` to read-only mode until a new key is
  minted, rotated in via `wrangler secret put STRIPE_SECRET_KEY`, and
  the canary clears. Replay last 24h via `corelink-billing-replay --since 24h`.
  Stripe retains 30d of events, so the audit trail of charges signed
  with the rotated key is recoverable via `events.list`.
- **`HUGR_WALLET_TOKEN`** (Stripe wallet-broker mode, when active):
  After revoking the `hugrw_` token in the wallet UI, immediately switch
  `corelink-billing-aggregator` to read-only mode until rotation completes.
  Replay last 24h via `corelink-billing-replay --since 24h`. Note: the upstream
  `sk_live_...` lives in wallet KV and is NOT exposed to CoreLink — a leak of
  `HUGR_WALLET_TOKEN` does NOT require Stripe-side key rotation (the wallet
  owner does that on a separate cadence inside the wallet). See
  `specs/_audits/2026-05-16-wallet-broker-stripe.md §6` blast-radius analysis
  + `§10` dual-mode addendum.
- **`STRIPE_WEBHOOK_SECRET`** (#3): UNCHANGED by wave-31 — inbound HMAC verify
  is direct, not brokered. Re-fetch missed webhooks via Stripe's `events.list`
  API with `created[gte]=T-24h`. Stripe retains 30d of events.
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
event.target = "HUGR_WALLET_TOKEN"
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
  "secret_name": "HUGR_WALLET_TOKEN",
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
| Stripe (direct mode, DEFAULT 2026-05-21) | `wrangler secret put STRIPE_SECRET_KEY` + Stripe dashboard for `sk_…` rotation | SRE Lead |
| Stripe (wallet-broker mode, wave-31; when broker is healthy) | `wrangler secret put HUGR_WALLET_TOKEN` + HuGR Wallet UI for upstream `sk_live_...` rotation (wallet-owner-side, decoupled) | SRE Lead (`hugrw_` token) + Wallet Owner (upstream key) |
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
