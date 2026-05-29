# Betterstack State — 2026-05-29

Validated and wired by Stream B12 (Phase 0 / Wave 33 orbits).

---

## Monitor

| Field               | Value                                              |
|---------------------|----------------------------------------------------|
| Monitor ID          | `4466381`                                          |
| URL                 | `https://corelink-api.humangr.com/_health`         |
| Monitor type        | `expected_status_code` (expects HTTP 200)          |
| Regions             | `us`, `eu`, `as`, `au` (4-region global coverage) |
| Check frequency     | 180 s (plan limit; 30 s was requested)             |
| Current status      | **up**                                             |
| SSL verification    | enabled                                            |
| SSL expiry alert    | 30 days                                            |
| Domain expiry alert | 30 days                                            |
| Request timeout     | 10 s                                               |
| Recovery period     | 60 s                                               |
| Created at          | 2026-05-29T22:14:58Z                               |
| Last checked at     | 2026-05-29T22:14:59Z                               |
| Paused              | false                                              |

---

## Status Page

| Field              | Value                                       |
|--------------------|---------------------------------------------|
| Page ID            | `247652`                                    |
| Company name       | Human Guardrail / CoreLink                  |
| Public URL         | `https://status.corelink.humangr.com`       |
| Subdomain          | `hugrl.betterstatuspage.com`                |
| Custom domain      | `status.corelink.humangr.com`              |
| Aggregate state    | **operational**                             |
| History window     | 90 days                                     |
| Password protected | false                                       |
| Created at         | 2026-05-16T17:17:39Z                        |

### Sections

| Section ID | Name  | Position |
|------------|-------|----------|
| `328864`   | API   | 0        |

### Resources

| Resource ID | Public name          | Monitor ID | Section ID | Status      |
|-------------|----------------------|------------|------------|-------------|
| `8888673`   | CoreLink API Health  | `4466381`  | `328864`   | operational |

---

## SLA — 2026-05-29 (baseline, first day monitored)

| Metric             | Value    |
|--------------------|----------|
| Availability       | 100.00%  |
| Total downtime     | 0 s      |
| Number of incidents| 0        |
| Longest incident   | 0 s      |

> Note: Monitor was created at 22:14:58 UTC on 2026-05-29 so history before that timestamp is `not_monitored`. The 24 h window is the first real baseline.

---

## Pre-creation state (orphan audit)

Before this run, the Betterstack account was in the following state:

- **Monitors:** 0 (none existed)
- **Status page 247652:** existed (created 2026-05-16) but had 0 sections and 0 resources — completely empty despite the page having been configured during Wave 32 setup
- **Orphan monitors (monitors with no page association):** none (no monitors existed)
- **Orphan pages (pages with no resources):** page 247652 was orphaned — now resolved

---

## Actions taken this run

1. Called `GET /api/v2/monitors` — confirmed 0 monitors.
2. Called `GET /api/v2/status-pages/247652` — confirmed page is for HuGR/CoreLink and belongs to subdomain `hugrl`.
3. Called `GET /api/v2/status-pages/247652/sections` — confirmed 0 sections.
4. Called `GET /api/v2/status-pages/247652/resources` — confirmed 0 resources.
5. Created monitor `4466381` via `POST /api/v2/monitors`.
6. Created section `328864` ("API") via `POST /api/v2/status-pages/247652/sections`.
7. Added monitor as resource `8888673` ("CoreLink API Health") via `POST /api/v2/status-pages/247652/resources`.
8. Verified monitor status = `up` and SLA = 100%.

---

## Operator TODOs

| Priority | TODO                                                                                                                                                 |
|----------|------------------------------------------------------------------------------------------------------------------------------------------------------|
| HIGH     | **Check frequency is 180 s, not 30 s** — the free/starter Betterstack plan enforces a 3-minute minimum. Upgrade to a paid plan to reduce to 30 s.    |
| MEDIUM   | **Add an on-call policy / escalation policy** — `policy_id` is currently `null`. Alerts go only to email. Wire PagerDuty or SMS for P1 coverage.     |
| MEDIUM   | **Verify `status.corelink.humangr.com` DNS** — custom domain is configured in Betterstack but DNS CNAME must point to `hugrl.betterstatuspage.com`.  |
| LOW      | **Add additional monitors** for worker endpoints (Cloudflare Worker), CAS API, admin API, and key gRPC probes once those planes are wired.            |
| LOW      | **Enable PagerDuty integration** — Wave 32 memo mentions PagerDuty was configured; cross-check that the Betterstack<>PagerDuty webhook is active.     |
| INFO     | Monitor `check_frequency` returned 180 s from API even though 30 was requested; this is a plan-tier cap, not a bug.                                  |

---

## Validation checklist

- [x] Monitor for `corelink-api.humangr.com/_health` exists (ID `4466381`)
- [x] Monitor status is `up`
- [x] Monitor linked to status page 247652 under section "API"
- [x] Status page public URL: `https://status.corelink.humangr.com`
- [x] SLA today: 100% availability, 0 incidents
- [x] No secret values in this document
