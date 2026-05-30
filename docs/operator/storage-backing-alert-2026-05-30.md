# Storage Backing Alert — 2026-05-30

Provisioned by agent wave ops run on 2026-05-30. Additive to existing
monitor `4466381` (`/_health`). Does NOT replace or modify `4466381`.

---

## New Monitor

| Field               | Value                                                        |
|---------------------|--------------------------------------------------------------|
| Monitor ID          | `4467865`                                                    |
| URL                 | `https://corelink-api.humangr.com/_health/container`        |
| Monitor type        | `keyword` (body must contain required keyword)               |
| Required keyword    | `"storage":"r2"`                                             |
| Alert condition     | Keyword absent = monitor goes down = alert fires             |
| Check frequency     | 180 s (plan cap; 60 s was requested, plan enforces 3-min min)|
| Regions             | us, eu, as, au (4-region global coverage)                    |
| Expected HTTP codes | 200                                                          |
| SSL verification    | enabled                                                      |
| SSL expiry alert    | 30 days                                                      |
| Domain expiry alert | 30 days                                                      |
| Request timeout     | 10 s                                                         |
| Recovery period     | 60 s                                                         |
| Current status      | **up**                                                       |
| policy_id           | `null` — PagerDuty wiring pending (see §PagerDuty below)     |
| Created at          | 2026-05-30T18:52:57Z                                         |

---

## Status Page Resource

Monitor `4467865` was added to status page `247652` under section "API":

| Field       | Value                          |
|-------------|--------------------------------|
| Resource ID | `8890128`                      |
| Public name | CoreLink Container Storage     |
| Section     | API (section ID `328864`)      |
| Status      | operational                    |

---

## Alert Semantics

The route `GET /_health/container` returns one of:

```json
{"status":"ok","storage":"r2"}      <- healthy: keyword present, no alert
{"status":"ok","storage":"inmemory"} <- degraded: keyword absent, ALERT FIRES
```

BetterStack `monitor_type=keyword` fires the alert when `required_keyword` is
**absent** from the response body. The keyword `"storage":"r2"` is absent when
the container fell back to `InMemoryStorage`, which is the exact silent
degradation this monitor is designed to catch.

---

## PagerDuty Integration Status

**Not yet wired via API.** BetterStack v2 API does not expose endpoints for
creating PagerDuty integrations or escalation policies programmatically
(all attempts returned 404). The `PAGERDUTY_ROUTING_KEY` in `.env.local` is
the production Events API v2 routing key.

**Required console actions (operator TODO — HIGH priority):**

1. Log into https://uptime.betterstack.com
2. On-call > Integrations > Add integration > PagerDuty
3. Paste `PAGERDUTY_ROUTING_KEY` from `.env.local` — save, note integration ID
4. On-call > Policies > Create policy "CoreLink Storage P1"
5. Add step: alert via PagerDuty integration from step 3, delay = 0 min
6. Save — note policy ID
7. Wire the policy to monitor `4467865`:
   ```sh
   source .env.local
   curl -X PATCH \
     -H "Authorization: Bearer $BETTERSTACK_API_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"policy_id": "<POLICY_ID>"}' \
     "https://uptime.betterstack.com/api/v2/monitors/4467865"
   ```

Until wired, alerts go to team email only (BetterStack `email: true` is set).

---

## Runbook

Full operator runbook (diagnosis + remediation steps):
`specs/_runbooks/rb-storage-fallback.md`

**TL;DR when paged:**

1. Confirm: `curl -sf https://corelink-api.humangr.com/_health/container`
2. Container returned `"storage":"inmemory"` — R2 credentials are broken
3. Check secrets on main worker + signup-worker:
   - `R2_S3_ACCESS_KEY_ID`
   - `R2_S3_SECRET_ACCESS_KEY`
   - `R2_S3_ENDPOINT`
4. Re-inject any missing/rotated secrets via `wrangler secret put`
5. Redeploy: `npx wrangler deploy --env prod`
6. Verify recovery: response must contain `"storage":"r2"`

---

## Verification gate

```sh
source .env.local
curl -H "Authorization: Bearer $BETTERSTACK_API_TOKEN" \
  "https://uptime.betterstack.com/api/v2/monitors/4467865" \
  | python3 -c "
import json, sys
d = json.load(sys.stdin)['data']['attributes']
print('url:', d['url'])
print('type:', d['monitor_type'])
print('keyword:', d['required_keyword'])
print('status:', d['status'])
print('freq:', d['check_frequency'], 's')
"
```

Expected output:
```
url: https://corelink-api.humangr.com/_health/container
type: keyword
keyword: "storage":"r2"
status: up
freq: 180 s
```

---

## Existing monitor preserved

| Monitor ID | URL                                              | Status |
|------------|--------------------------------------------------|--------|
| `4466381`  | `https://corelink-api.humangr.com/_health`       | up     |
| `4467865`  | `https://corelink-api.humangr.com/_health/container` | up |

Monitor `4466381` was NOT modified. The new monitor `4467865` is purely additive.

---

## Actions taken this run

1. Called `GET /api/v2/monitors` — confirmed 1 existing monitor (ID `4466381`), no duplicate at `/_health/container`.
2. Called `POST /api/v2/monitors` — created monitor `4467865` with `monitor_type=keyword`, `required_keyword="storage":"r2"`.
3. Called `POST /api/v2/status-pages/247652/resources` — added monitor `4467865` as resource `8890128` under section "API" (`328864`).
4. Verified `GET /api/v2/monitors/4467865` returns `status: up`, `required_keyword: '"storage":"r2"'`.
5. Documented PagerDuty console-only gap (hard pause trigger #2 per task spec).
6. Created runbook at `specs/_runbooks/rb-storage-fallback.md`.

---

## Notes

- Check frequency returned 180 s (plan minimum); 60 s was requested.
- `policy_id` is `null` — email-only alerts until PD console wiring is done.
- The provisioning script is at `scripts/ops/provision-storage-monitor.sh` (idempotent; checks for existing monitor at same URL before creating).
