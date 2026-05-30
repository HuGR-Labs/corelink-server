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
| **CRITICAL** | **BetterStack console: click "Enable SSL" for custom domain** — see section below. Smoke check [22] stays KNOWN-EXCEPTION until done.            |
| MEDIUM   | **Add an on-call policy / escalation policy** — `policy_id` is currently `null`. Alerts go only to email. Wire PagerDuty or SMS for P1 coverage.     |
| RESOLVED | **`status.corelink.humangr.com` DNS verified (2026-05-30)** — CF CNAME correctly points to `hugrl.betteruptime.com` (DNS-only). The operator doc previously said `hugrl.betterstatuspage.com` — that was incorrect; `betteruptime.com` is BetterStack's live CDN domain. Smoke check [14] fix: script now uses `dig CNAME +short` instead of `dig +short \| tail -1` to verify the CNAME target rather than the resolved IP. See `wt/c10-status-page-rewire`. |
| LOW      | **Add additional monitors** for worker endpoints (Cloudflare Worker), CAS API, admin API, and key gRPC probes once those planes are wired.            |
| LOW      | **Enable PagerDuty integration** — Wave 32 memo mentions PagerDuty was configured; cross-check that the Betterstack<>PagerDuty webhook is active.     |
| INFO     | Monitor `check_frequency` returned 180 s from API even though 30 was requested; this is a plan-tier cap, not a bug.                                  |

---

## TLS Provisioning — Root Cause Analysis (2026-05-30)

**Problem:** `https://status.corelink.humangr.com` returns HTTP 000 (TLS handshake failure — "no peer certificate available").

**Investigation performed (smoke check [22] TLS fix attempt):**

1. CF DNS record `status.corelink.humangr.com` → `hugrl.betteruptime.com` is `proxied: false` (DNS-only) — correct.
2. BetterStack API (`GET /api/v2/status-pages/247652`) shows `custom_domain: status.corelink.humangr.com` set correctly.
3. Port 80 HTTP-only request to `status.corelink.humangr.com` returns `301 → https://status.corelink.humangr.com/` — BetterStack's CDN edge sees and recognizes the domain.
4. Port 443 TLS handshake: `no peer certificate available` — BetterStack's CDN has no cert for this custom domain.
5. ACME HTTP-01 challenge path `/.well-known/acme-challenge/test` returns 403 Forbidden from BetterStack's edge — ACME HTTP-01 validation is blocked, which is why auto-provisioning never completes.
6. API cycle (clear `custom_domain` → re-set) was performed to re-trigger BetterStack's TLS provisioning pipeline. Polled for 4+ minutes — no cert appeared. HTTP-01 challenge blocked confirms provisioning cannot complete automatically.
7. Attempted CF proxy (`proxied: true`) as fallback: CF Universal SSL cert covers only `*.humangr.com` (1-level wildcard), NOT `status.corelink.humangr.com` (2-level subdomain). CF Advanced Certificate Manager requires a paid plan (zone is on Free). CF proxy reverted to `proxied: false`.

**Root cause:** BetterStack's TLS provisioning for custom domains requires a **manual "Enable SSL" activation in the BetterStack web console**. The API does not expose a TLS-trigger endpoint. ACME HTTP-01 is blocked by BetterStack's own edge (403 on `.well-known/acme-challenge/`) — likely because BetterStack uses Cloudflare for Platforms (SSL for SaaS) for custom domain certs, which requires manual per-domain activation in their dashboard.

**Why the 2-level subdomain matters (structural constraint):**  
`status.corelink.humangr.com` = `<host>.<sub2>.<apex>`. CF Universal SSL `*.humangr.com` covers only `<host>.<apex>`. Moving to `status.humangr.com` (1-level) would be covered, but the canonical domain is locked (plan: DO NOT change).

**Manual operator action required — CRITICAL:**

1. Log into BetterStack at `https://betterstack.com` → Uptime → Status Pages → page `247652` (HuGR / CoreLink).
2. Navigate to **Settings → Custom domain**.
3. The domain `status.corelink.humangr.com` should already be entered.
4. Click **"Enable SSL"** or **"Issue certificate"** (button label varies by BetterStack UI version).
5. BetterStack will use Cloudflare for Platforms to issue a cert for `status.corelink.humangr.com` — this typically takes 30–90 seconds after the console action.
6. After the cert appears, verify: `curl -sIL https://status.corelink.humangr.com` should return HTTP 200.
7. Once confirmed HTTP 200: remove the KNOWN EXCEPTION from smoke check [22] in `scripts/smoke-prod-corelink.sh` (change the `warn` branch to `fail`, update the comment).

**No alternative path exists without operator action.** A CF Worker stub redirect is not viable on the Free plan (no cert coverage for 2-level subdomain). A CF paid plan upgrade would cost $20/mo (Pro) and is an operator decision.

**BetterStack page ID:** `247652`  
**Status page URL:** `https://status.corelink.humangr.com`  
**CF DNS record ID:** `e7853992353f74b09258b1011c747704` (CNAME → `hugrl.betteruptime.com`, DNS-only — DO NOT proxy until TLS is fixed)

---

## Validation checklist

- [x] Monitor for `corelink-api.humangr.com/_health` exists (ID `4466381`)
- [x] Monitor status is `up`
- [x] Monitor linked to status page 247652 under section "API"
- [x] Status page public URL: `https://status.corelink.humangr.com`
- [x] CF DNS CNAME `status.corelink.humangr.com` → `hugrl.betteruptime.com` (DNS-only, record ID `e7853992353f74b09258b1011c747704`)
- [x] Smoke check [14] fix landed in `wt/c10-status-page-rewire` — uses `dig CNAME +short` to verify CNAME target
- [ ] `https://status.corelink.humangr.com` returning 000 (TLS handshake failure — no peer certificate) — root cause documented in "TLS Provisioning" section above; requires MANUAL OPERATOR action in BetterStack console to activate SSL for custom domain; tracked as KNOWN-EXCEPTION in smoke check [22]
- [x] SLA today: 100% availability, 0 incidents
- [x] No secret values in this document
