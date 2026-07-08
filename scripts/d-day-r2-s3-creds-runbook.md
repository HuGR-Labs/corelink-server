# D-Day Runbook — R2 S3 Access Key Mint + Container Secret Put

**Owner:** SRE Lead  
**Wave:** WP-C1 (P0 wave Phase 1)  
**Date authored:** 2026-05-28  
**Paired with:** `wrangler.toml` `[[env.prod.containers]]` WP-C1 config  
**Audit ref:** `specs/_audits/2026-05-28-wp-c1-container-config-seal.md`

---

## Purpose

The CoreLink native container reaches Cloudflare R2 via the S3-compatible API
over egress (DECISION-GATE-1 Option A). It requires two credentials:

| Env var | Row | What it is |
|---|---|---|
| `R2_S3_ACCESS_KEY_ID` | secrets-checklist #139 | R2 S3 API token key ID |
| `R2_S3_SECRET_ACCESS_KEY` | secrets-checklist #140 | R2 S3 API token secret key |

These must be provisioned via `wrangler secret put` before the container can
connect to R2 buckets. **No real secret values appear anywhere in this repo.**

---

## Pre-requisites

- [ ] Cloudflare dashboard access with R2 permissions on the `HumanGuardrail`
  account (same account that holds the CoreLink R2 buckets)
- [ ] `wrangler` CLI authenticated (`wrangler whoami` confirms account)
- [ ] Account ID available (`wrangler whoami` prints it, or CF dashboard →
  right sidebar → "Account ID")

---

## Step 1 — Identify the target R2 buckets

The container will access the same buckets declared in `[[env.prod.r2_buckets]]`
in `wrangler.toml`. At a minimum:

```
corelink-objects-iad
corelink-objects-fra
corelink-objects-gru
corelink-objects-nrt
corelink-objects-syd
corelink-manifest-iad
corelink-manifest-fra
corelink-manifest-nrt
corelink-manifest-syd
```

Confirm the list is current before scoping token permissions.

---

## Step 2 — Mint the R2 API token in the Cloudflare dashboard

1. Navigate to **[Cloudflare Dashboard](https://dash.cloudflare.com)** and log
   in with the SRE Lead account.
2. From the left sidebar, select **R2 Object Storage**.
3. Click **Manage R2 API Tokens** (top-right of the R2 overview page).
4. Click **Create API Token**.
5. Configure the token:
   - **Token name:** `corelink-container-r2-prod` (or include date for rotation
     traceability: `corelink-container-r2-prod-2026-05-28`)
   - **Permissions:** Select **Object Read & Write** (minimum required for CAS
     put/get operations). If the container also manages bucket lifecycle or
     lists objects for cleanup, add **Object Read & Write** at minimum; do NOT
     grant **Admin Read & Write** unless explicitly required.
   - **Bucket scope:** Restrict to the buckets listed in Step 1. Do NOT grant
     access to `corelink-audit-archive` (audit-chain bucket; read-only token
     per CTRL-AUDIT-001 is provisioned separately).
   - **TTL:** Leave as "No expiry" (rotation is enforced manually every 90d
     per secrets-checklist #139/#140 rotation cadence; set a calendar reminder).
6. Click **Create API Token**.
7. **Copy both values immediately** (the secret key is shown only once):
   - **Access Key ID** → goes into `R2_S3_ACCESS_KEY_ID`
   - **Secret Access Key** → goes into `R2_S3_SECRET_ACCESS_KEY`
8. Store values in a temporary secure location (1Password vault, not
   clipboard-only) until the `wrangler secret put` step is complete.

> **Security note:** The Secret Access Key is shown exactly once at creation
> time. If you navigate away without copying it, you must revoke the token and
> create a new one — there is no "reveal" option.

---

## Step 3 — Set the account ID in wrangler.toml vars (non-secret)

The `R2_S3_ENDPOINT` in `wrangler.toml` is currently a placeholder:

```toml
[env.prod.containers.vars]
R2_S3_ENDPOINT = "https://CLOUDFLARE_ACCOUNT_ID_PLACEHOLDER.r2.cloudflarestorage.com"
```

Replace the placeholder with the actual account ID (this is public config, not
a secret — the CF account ID is already exposed in other `wrangler.toml`
bindings and GHA secrets as `CF_ACCOUNT_ID`):

```toml
[env.prod.containers.vars]
R2_S3_ENDPOINT = "https://<your-32-hex-account-id>.r2.cloudflarestorage.com"
```

Confirm the account ID matches `wrangler whoami` output. Commit this change
as part of the D-day deploy PR (no secret material; plain config).

---

## Step 4 — Put the secrets via `wrangler secret put`

Run the following two commands interactively (each prompts for the value via
stdin; never pass values as CLI flags or env vars to avoid shell history leaks):

```bash
# Access Key ID
wrangler secret put R2_S3_ACCESS_KEY_ID --env prod
# Paste the Access Key ID from Step 2 when prompted, then press Enter.

# Secret Access Key
wrangler secret put R2_S3_SECRET_ACCESS_KEY --env prod
# Paste the Secret Access Key from Step 2 when prompted, then press Enter.
```

> **CLOUDFLARE_ACCOUNT_ID** — this secret should already be present in the
> prod Worker from prior wave deploys. Verify with:
> ```bash
> wrangler secret list --env prod | grep CLOUDFLARE_ACCOUNT_ID
> ```
> If missing, put it as well:
> ```bash
> wrangler secret put CLOUDFLARE_ACCOUNT_ID --env prod
> ```

---

## Step 5 — Verify secrets are registered

```bash
wrangler secret list --env prod | grep -E "R2_S3_ACCESS_KEY_ID|R2_S3_SECRET_ACCESS_KEY|CLOUDFLARE_ACCOUNT_ID"
```

Expected output (exact names present, no values printed by Wrangler):

```
R2_S3_ACCESS_KEY_ID
R2_S3_SECRET_ACCESS_KEY
CLOUDFLARE_ACCOUNT_ID
```

---

## Step 6 — Redeploy the container

After secrets are registered, trigger a container redeploy so the runtime picks
up the new environment variables:

```bash
wrangler deploy --env prod
```

Or, if the container is deployed via the standard CI pipeline, push a deploy
trigger commit to the main branch and allow `cf-deploy-prod.yml` to run.

---

## Step 7 — Smoke test R2 connectivity

Once the container is running with the new credentials, verify R2 connectivity
via the smoke check:

```bash
# Via the standard smoke script (post-DNS Phase H)
./scripts/smoke-prod-corelink.sh

# Or manually: push a small test artifact and verify retrieval
# (adjust endpoint to your environment)
curl -s -X POST https://api.corelink.humangr.com/v1/cas \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @/dev/null \
  | jq .
```

A successful CAS write + read that returns `200 OK` confirms the container is
reaching R2 via the S3 API.

---

## Rotation procedure (every 90 days)

1. Create a **new** R2 API token (Step 2 above) — do NOT revoke the old token
   first (to avoid a window where the container has no valid credentials).
2. `wrangler secret put R2_S3_ACCESS_KEY_ID --env prod` with the new key ID.
3. `wrangler secret put R2_S3_SECRET_ACCESS_KEY --env prod` with the new secret.
4. Deploy (`wrangler deploy --env prod` or CI push).
5. Smoke test (Step 7).
6. **Only after** smoke passes: revoke the old R2 API token from the CF
   dashboard.
7. Update the `secrets-checklist.md` rotation date column for rows #139 / #140.

---

## References

- `wrangler.toml` `[[env.prod.containers]]` — WP-C1 config (this runbook's
  counterpart)
- `docs/internal/secrets-checklist.md` rows #139, #140
- `specs/_audits/2026-05-28-wp-c1-container-config-seal.md`
- `docs/internal/secrets-runbook.md` — general operational procedures
- Cloudflare R2 S3 API docs: https://developers.cloudflare.com/r2/api/s3/
