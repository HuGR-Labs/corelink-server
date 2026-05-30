---
id: "RB-STORAGE-FALLBACK"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-30"
updated: "2026-05-30"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "storage", "r2", "inmemory", "betterstack", "pagerduty", "silent-degradation", "wave-33"]
---

# RB-STORAGE-FALLBACK — Container fell back to InMemory storage

> **Status:** ACTIVE. Triggered by BetterStack monitor `4467865` on
> `https://corelink-api.humangr.com/_health/container` when the response body
> does NOT contain `"storage":"r2"` (i.e., `"storage":"inmemory"` is returned).
>
> **Severity:** P1 — silent data-loss risk. Writes accepted by the container are
> not persisted to R2. No client-visible error is returned, so this degrades
> without any user-facing signal.
>
> **RTO target:** ≤ 30 min from page to R2 connectivity restored.

## 1. Trigger conditions

This runbook fires when BetterStack monitor `4467865` raises an alert, meaning:

- `GET https://corelink-api.humangr.com/_health/container` returns HTTP 200 **but**
- The response body does NOT contain the substring `"storage":"r2"`
- Typical failing body: `{"status":"ok","storage":"inmemory"}`

The container initialization sequence tries R2 first; if R2 credentials are
missing or invalid, it silently falls back to `InMemoryStorage`. All writes
succeed but nothing persists across restarts.

## 2. Diagnosis

### 2.1 Confirm the active storage backend

```sh
curl -sf https://corelink-api.humangr.com/_health/container | python3 -m json.tool
```

Expected healthy response:
```json
{"status": "ok", "storage": "r2"}
```

Degraded response that triggered this page:
```json
{"status": "ok", "storage": "inmemory"}
```

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
for i in $(seq 1 18); do
  RESP=$(curl -sf https://corelink-api.humangr.com/_health/container)
  echo "$RESP"
  echo "$RESP" | grep -q '"storage":"r2"' && { echo "RECOVERED"; break; }
  sleep 10
done
```

BetterStack will auto-resolve the incident once the monitor detects
`"storage":"r2"` in the next check cycle (≤ 180 s after recovery).

## 4. Escalation

| Condition | Action |
|---|---|
| Secrets exist but R2 still fails after redeploy | Escalate to Cloudflare support; R2 regional incident |
| R2 bucket deleted or misconfigured | Escalate to Gustavo Schneiter immediately — bucket recreation is a data-loss event |
| InMemory was active for > 30 min | Audit what writes landed in that window; those objects are lost — run postmortem per `RB-POSTMORTEM-PROCESS.md` |
| PagerDuty policy not yet wired | BetterStack sends email alerts until PD policy is configured (see §5) |

## 5. PagerDuty escalation wiring — OPERATOR TODO

BetterStack's PagerDuty integration requires console-side setup. The routing
key `PAGERDUTY_ROUTING_KEY` is stored in `.env.local` but BetterStack does
NOT expose an API endpoint to create PagerDuty integrations programmatically
(v2 API returns 404 for `/api/v2/integrations`, `/api/v2/policies`, etc.).

**Required manual console steps:**

1. Log into https://uptime.betterstack.com
2. Navigate to On-call > Integrations > Add integration
3. Select "PagerDuty"
4. Paste the routing key from `.env.local` (`PAGERDUTY_ROUTING_KEY`)
5. Save the integration — note the integration ID
6. Go to On-call > Policies > Create policy
7. Name it "CoreLink Storage P1"
8. Add a step: "Alert via PagerDuty" using the integration from step 4
9. Set escalation delay: 0 min (immediate)
10. Save the policy — note the policy ID
11. Call the BetterStack API to attach the policy to monitor `4467865`:
    ```sh
    source .env.local
    curl -X PATCH \
      -H "Authorization: Bearer $BETTERSTACK_API_TOKEN" \
      -H "Content-Type: application/json" \
      -d '{"policy_id": "<POLICY_ID_FROM_STEP_10>"}' \
      "https://uptime.betterstack.com/api/v2/monitors/4467865"
    ```

Until this is done, alerts go to the team email only.

## 6. Monitor configuration summary

| Field | Value |
|---|---|
| Monitor ID | `4467865` |
| URL | `https://corelink-api.humangr.com/_health/container` |
| Type | `keyword` (body must contain required keyword) |
| Required keyword | `"storage":"r2"` |
| Alert condition | Keyword absent from body = degraded |
| Check frequency | 180 s (plan cap; 60 s was requested) |
| Regions | us, eu, as, au |
| Status page resource | `8890128` under section "API" (page 247652) |
| policy_id | `null` (pending console wiring per §5) |
| Created at | 2026-05-30T18:52:57Z |

## 7. Related documents

- `docs/operator/storage-backing-alert-2026-05-30.md` — creation log and wiring details
- `docs/operator/betterstack-state-2026-05-29.md` — BetterStack account state (existing monitor `4466381`)
- `specs/_runbooks/RB-SECRETS-DRIFT.md` — secret drift detection and rotation runbook
- `specs/_audits/2026-05-28-multimodel-prod-readiness-audit.md` — prod readiness audit noting wiring gap
