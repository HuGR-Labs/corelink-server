# CoreLink Synthetic Monitoring Probes

## Overview

This directory contains the BetterStack synthetic monitoring probe definitions for all CoreLink production endpoints. Probes are defined declaratively in `probes.yml` and applied via `scripts/apply-betterstack-probes.sh`.

**BetterStack status page ID:** `247652`
**Custom domain:** `https://status.corelink.humangr.com`

---

## Probe inventory (7 probes)

| # | Name | URL | Interval | Method | Notes |
|---|---|---|---|---|---|
| 1 | `corelink-api-health` | `https://corelink-api.humangr.com/health` | 30s | GET | JSON body assert `$.status == "ok"` |
| 2 | `corelink-app-ui` | `https://humangr.com/corelink` | 60s | GET | Clerk-gated Next.js; expect 200 |
| 3 | `corelink-docs` | `https://corelink-docs.humangr.com` | 60s | GET | Docusaurus CF Pages; expect 200 |
| 4 | `corelink-signup-health` | `https://corelink-signup.humangr.com` | 60s | GET | Signup worker; expect 200 |
| 5 | `corelink-get-install` | `https://corelink-get.humangr.com` | 60s | GET | Install script; expect 200 + `text/plain` |
| 6 | `corelink-admin-health` | `https://corelink-admin.humangr.com/health` | 60s | GET | R27 — admin worker health |
| 7 | `corelink-get-head` | `https://corelink-get.humangr.com` | 60s | HEAD | R27 — lightweight HEAD check |

Probes run from three regions: `us-east`, `eu`, `ap`.

---

## Applying probes

### Dry-run (safe — no API calls made)

```bash
bash scripts/apply-betterstack-probes.sh --dry-run
```

Prints a full list of probes and the BetterStack API JSON body that **would** be sent for each, then exits 0. No HTTP call is made.

### Live apply (Owner-only)

```bash
# Requires BETTERSTACK_API_TOKEN in env (gitignored .env.local)
source .env.local
bash scripts/apply-betterstack-probes.sh --apply
```

The script is idempotent: it GETs `/api/v2/monitors`, matches by `pronounceable_name`, and PATCHes existing monitors instead of POSTing duplicates.

### Update a single probe

```bash
bash scripts/apply-betterstack-probes.sh --apply --probe corelink-api-health
```

### Delete a probe

```bash
bash scripts/apply-betterstack-probes.sh --delete --probe corelink-api-health
```

Dry-run is enforced by default — pass `--apply` to execute live.

---

## Schema reference (`probes.yml` fields)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Machine name; used as idempotency key |
| `pronounceable_name` | string | yes | Display name in BetterStack dashboard |
| `url` | string | yes | Target URL |
| `method` | string | yes | HTTP verb: `GET`, `HEAD`, `POST` |
| `monitor_type` | string | yes | BetterStack type; currently always `"status"` |
| `check_frequency_seconds` | int | yes | Probe interval (30–3600) |
| `request_timeout_seconds` | int | yes | Per-request timeout |
| `expected_status_codes` | list[int] | yes | HTTP codes that mean OK |
| `content_type_assertion` | string | no | Assert `Content-Type` header contains value |
| `body_assertions` | list | no | Optional body assertions (see below) |
| `regions` | list[string] | yes | BetterStack POP regions |
| `confirmation_period_seconds` | int | yes | Seconds before alert fires (0 = instant) |
| `paused` | bool | yes | Whether probe starts paused |
| `tags` | list[string] | no | BetterStack labels |

### Body assertion schema

```yaml
body_assertions:
  - type: "json_body"       # or "text_body"
    path: "$.status"        # JSONPath (for json_body)
    value: "ok"             # expected value
```

---

## Architecture notes

- Probes are version-controlled here; the apply script is the only authorized mechanism for live changes.
- No secrets are stored in `probes.yml`. The `BETTERSTACK_API_TOKEN` is always read from env.
- Probe 1 (`corelink-api-health`) runs at 30s interval (tighter than the rest) because the API worker is the primary customer-facing surface and the one most likely to exhibit transient failures.
- Probes 6 and 7 were added under R27 to cover the admin worker and the install endpoint lightweight HEAD check.

---

## Cross-references

- BetterStack SEAL: `specs/_audits/sealed/2026-05-22-w32-phaseA-betterstack-live.md`
- Wave 32 Phase I sign-off: `specs/_audits/sealed/2026-05-26-w32-phaseI-signoff.md`
- Status page spec: `marketing/launch/STATUS-PAGE-SPEC.md`
- Synthetic monitoring SEAL: `specs/_audits/2026-05-27-synthetic-monitoring-seal.md`
