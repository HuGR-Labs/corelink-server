---
id: "RB-STORAGE-FALLBACK"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-30"
updated: "2026-09-05"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "storage", "r2", "inmemory", "betterstack", "pagerduty", "silent-degradation", "wave-33"]
---

# RB-STORAGE-FALLBACK — Container fell back to InMemory storage

> **Status:** ACTIVE, operator-triggered. The historical public storage alert
> is retired because anonymous health redacts storage. Confirm the condition
> with the authenticated probe below.
>
> **Severity:** P1 — silent data-loss risk. Writes accepted by the container are
> not persisted to R2. No client-visible error is returned, so this degrades
> without any user-facing signal.
>
> **RTO target:** ≤ 30 min from page to R2 connectivity restored.

## 1. Trigger conditions

Open this runbook when a protected operator probe reports an in-memory storage
backend:

- The authenticated health request returns HTTP 200.
- The response reports `storage` as `inmemory`.

The anonymous health endpoint intentionally returns only safe liveness fields
and is not a storage signal. The container initialization sequence tries R2
first; if R2 credentials are missing or invalid, it silently falls back to
`InMemoryStorage`. All writes succeed but nothing persists across restarts.

## 2. Diagnosis

### 2.1 Confirm the active storage backend

Set `CORELINK_API_ORIGIN` to the current API origin in the protected operator
shell. The dedicated admin key must be supplied through the header, never in a
URL or query string.

```sh
AUTH_HEALTH_URL="${CORELINK_API_ORIGIN:?set the current API origin}/_health/container/authenticated"
curl -sf \
  -H "X-Corelink-Internal-Auth: $CORELINK_ADMIN_AUTH_KEY" \
  "$AUTH_HEALTH_URL" | python3 -m json.tool
```

Expected healthy response: `.status == "ok"` and `.storage == "r2"`.

The degraded response that triggers this page has `.storage == "inmemory"`.

### 2.2 Check R2 secrets on both workers

The container initializes R2 with three environment secrets. These must be
set on **both** the main worker and the signup-worker.

```sh
# List secrets currently bound to the main worker (names only — values never exposed)
npx wrangler secret list --env prod

# List secrets on signup-worker
npx wrangler secret list --name corelink-signup-worker --env prod
```

Expect to see all three:
- `R2_S3_ACCESS_KEY_ID`
- `R2_S3_SECRET_ACCESS_KEY`
- `R2_S3_ENDPOINT`

If any secret is missing from the output, proceed to §3.

### 2.3 Check for recent secret rotation

```sh
# Review deployment history for secret changes
npx wrangler deployments list --env prod | head -10
```

If a secret rotation happened in the last 24 h (new key generated, old one
deleted before propagation), the container may have started with stale or
absent credentials.

### 2.4 Check R2 bucket reachability directly

If the secrets exist but R2 connectivity is broken (bucket deleted, regional
outage, permission policy change):

1. Go to Cloudflare dashboard → R2 → `corelink-objects` bucket.
2. Verify bucket exists and is not in an error state.
3. Check Cloudflare status: https://www.cloudflarestatus.com for R2 incidents.

## 3. Remediation

### 3.1 Re-inject missing or rotated secrets

```sh
# Re-set the R2 access key on main worker
echo "<new_access_key_id>" | npx wrangler secret put R2_S3_ACCESS_KEY_ID --env prod

echo "<new_secret_access_key>" | npx wrangler secret put R2_S3_SECRET_ACCESS_KEY --env prod

echo "https://<account_id>.r2.cloudflarestorage.com" | npx wrangler secret put R2_S3_ENDPOINT --env prod

# Repeat for signup-worker
echo "<new_access_key_id>" | npx wrangler secret put R2_S3_ACCESS_KEY_ID --name corelink-signup-worker --env prod
echo "<new_secret_access_key>" | npx wrangler secret put R2_S3_SECRET_ACCESS_KEY --name corelink-signup-worker --env prod
echo "https://<account_id>.r2.cloudflarestorage.com" | npx wrangler secret put R2_S3_ENDPOINT --name corelink-signup-worker --env prod
```

Values come from: Cloudflare dashboard → R2 → Manage R2 API tokens → the
`corelink-worker-rw` API token (or create a new one if rotated).

### 3.2 Redeploy the container worker

After secrets are updated, the container worker must restart to re-initialize
storage with the new credentials:

```sh
# Trigger a redeploy of the main container worker
cd /Users/gustavoschneiter/Documents/HuGR/corelink-server
npx wrangler deploy --env prod
```

### 3.3 Verify recovery

```sh
# Poll until storage=r2 is confirmed (runs every 10s, up to 3 minutes)
AUTH_HEALTH_URL="${CORELINK_API_ORIGIN:?set the current API origin}/_health/container/authenticated"
for i in $(seq 1 18); do
  RESP=$(curl -sf \
    -H "X-Corelink-Internal-Auth: $CORELINK_ADMIN_AUTH_KEY" \
    "$AUTH_HEALTH_URL")
  echo "$RESP"
  echo "$RESP" | jq -e '.storage == "r2"' >/dev/null && { echo "RECOVERED"; break; }
  sleep 10
done
```

The protected probe is the source of truth after recovery. The retired public
alert must not be used to auto-resolve this condition.

## 4. Escalation

| Condition | Action |
|---|---|
| Secrets exist but R2 still fails after redeploy | Escalate to Cloudflare support; R2 regional incident |
| R2 bucket deleted or misconfigured | Escalate to Gustavo Schneiter immediately — bucket recreation is a data-loss event |
| InMemory was active for > 30 min | Audit what writes landed in that window; those objects are lost — run postmortem per `RB-POSTMORTEM-PROCESS.md` |
| Historical public storage alert is encountered | Do not use it as a storage signal; use the authenticated probe |

## 5. Historical alert record

A BetterStack storage check was created on 2026-05-30 against an anonymous
container-health response and a body marker. Its status-page entry and alert
configuration are historical evidence only. The anonymous response cannot
carry `CORELINK_ADMIN_AUTH_KEY`, so the check must not be enabled or used for
alerting. Any alerting integration must invoke the authenticated probe through
a protected secret-bearing system.

## 6. Related documents

- `docs/operator/storage-backing-alert-2026-05-30.md` — historical record and current probe
- `docs/operator/betterstack-state-2026-05-29.md` — BetterStack account state
- `specs/_runbooks/RB-SECRETS-DRIFT.md` — secret drift detection and rotation runbook
- `specs/_audits/2026-05-28-multimodel-prod-readiness-audit.md` — prod readiness audit
