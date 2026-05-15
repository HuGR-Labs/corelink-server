---
id: "DAY-1-DASHBOARD-SPEC"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "SRE Lead + VPMkt (dual)"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "ROADMAP-TO-GA.md §8 (R-8 Launch)"
tags:
  - "marketing"
  - "launch"
  - "dashboard"
  - "metrics"
  - "day-1"
  - "war-room"
  - "wt-r8-1"
---

# Day-1 Metric Dashboard — Specification

> **Purpose:** the single dashboard the war room watches from T-1h to T+72h. Five metric families; everything else is noise on launch day.
> **Audience:** WR-COORD, SRE-OC, VPMkt, CEO.
> **Cross-references:** `LAUNCH-CHECKLIST-V2.md` (when each metric is checked), `STATUS-PAGE-SPEC.md` (severity mapping for SLO burns), `marketing/launch/METRICS-DASHBOARD.md` (the broader T-0..T+7d metrics; this dashboard is the **launch-window subset**).

---

## 1. Dashboard layout

Single-screen, 5 panels arranged in a 2x3 grid (last cell = top-5 SLO burn table). Refresh: 60s. Tool: Grafana (primary) + Statuspage public view (secondary) + Linear support board (manual).

```
+---------------------------+---------------------------+
|  Panel 1: Signups         |  Panel 2: Activation      |
|  (60min / 24h / cumulative)| (first CAS write per     |
|                           |  signup; %)               |
+---------------------------+---------------------------+
|  Panel 3: Support tickets |  Panel 4: SLO error       |
|  (volume + category)      |  budget burn — top 5      |
+---------------------------+---------------------------+
|  Panel 5: Press / social mentions (manual entry)      |
+--------------------------------------------------------+
```

---

## 2. Metric families

### M1. Signups (last 60 min / 24h / cumulative)

| Metric | Source | Aggregation | Display |
|---|---|---|---|
| Signups last 60 min | Clerk `user.created` webhook → Loki sink | Rolling 60-min count | Big number + sparkline |
| Signups last 24h | Same | Rolling 24h count | Big number |
| Cumulative since T-0 | Same | Cumulative since T-0 timestamp | Big number + step-chart |
| Signup rate per minute | Same | 5-min rolling average | Sparkline (alert threshold: > 10x projected = bot suspect) |
| Geo distribution | Clerk `signup.country` | Bar chart top-10 countries | Side panel |
| Referrer attribution | `utm_source` + Referer header | Donut: PH, HN, LinkedIn, Twitter, press, direct, organic | Side panel |

**Targets (24h):**
- Floor: 500 signups in first 24h (engineering minimum to validate signup pipeline at scale).
- Stretch: 5,000 signups in first 24h.
- Bot signature: > 10x projected rate over a 5-min window → SRE-OC investigates.

### M2. Activation — first CAS write per signup

| Metric | Source | Aggregation | Display |
|---|---|---|---|
| Activation % (overall) | Join: Clerk signups × CAS write `UpdateBlob` events keyed by tenant_id | (# signups with ≥1 successful CAS write) / (# signups) — rolling 24h | Big number (%) |
| Activation funnel by hour | Same | Cohort by signup hour | Cohort chart |
| Median time to first CAS write | Same | p50 of (first CAS write timestamp − signup timestamp) | Big number (minutes) |
| Activation by referrer | Same, filtered by `utm_source` | Cohort chart per referrer | Side panel |

**Targets (24h):**
- Floor: 25% activation in first 24h (signup → first CAS write).
- Stretch: 50%.
- p50 time-to-activation target: ≤ 30 min.
- Alert threshold: activation < 10% for any single hour cohort → CS-OC reviews; CTO investigates pipeline.

### M3. Support ticket volume + categorized

| Metric | Source | Aggregation | Display |
|---|---|---|---|
| Ticket volume last 60 min | support@corelink.dev / HubSpot inbox → Linear support board | Rolling 60-min count | Big number + sparkline |
| Ticket volume last 24h | Same | Rolling 24h count | Big number |
| Ticket categories | Manual tag by CS-OC: `signup`, `billing`, `cli`, `cache-correctness`, `byok`, `docs-bug`, `feature-request`, `pricing`, `other` | Donut chart | Side panel |
| First-response SLA compliance | Linear time-to-first-response | % within target (1h business / 4h off-hours) | Big number (%) |
| Critical ticket flag count | Manual flag: customer says "outage" or "data loss" or "wrong charge" | Count | Big red banner if > 0 |

**Targets (24h):**
- Volume baseline: 1 ticket per 10 signups expected; > 1 per 3 signups = signal of confusion.
- SLA: 100% < 4h first response during launch window (all-hands posture).
- Critical ticket flag: any positive value pages CTO.

### M4. Error budget burn — top 5 SLOs

Top 5 SLOs to watch:

| # | SLO | Source | Burn threshold (1h) |
|---|---|---|---|
| 1 | API p99 latency ≤ 250 ms | Prometheus `corelink_api_request_duration_seconds` | 2% of monthly budget in 1h = page |
| 2 | CAS Read p99 ≤ 100 ms (cache hit) | Prometheus `corelink_cas_read_hit_duration_seconds` | 2% in 1h = page |
| 3 | CAS Write p99 ≤ 500 ms | Prometheus `corelink_cas_write_duration_seconds` | 2% in 1h = page |
| 4 | BYOK envelope p99 ≤ 50 ms | Prometheus `corelink_byok_envelope_duration_seconds` | 2% in 1h = page |
| 5 | Signup-to-first-CAS-write success ≥ 95% | Synthetic E2E check every 5 min | 5% failure in 1h = page |

**Display:** stacked bar — current 1h burn rate + 24h burn rate vs. monthly budget. Cells turn yellow at 1% (1h), red at 2% (1h). Companion: 5-row table with raw p99 values.

**Page-out wiring:** any cell red ↔ PagerDuty SEV2 fires automatically. SEV2 triggers `STATUS-PAGE-SPEC.md` §5 partial-outage flow.

### M5. Press / social mention count (manual)

| Metric | Source | Aggregation | Display |
|---|---|---|---|
| Tier-1 press mentions | Manual entry by VPMkt / PR firm | Cumulative since T-0 | Big number |
| Ecosystem press mentions | Manual entry | Cumulative since T-0 | Big number |
| Twitter / X mentions of @corelinkdev | Twitter API or ahrefs Brand Mentions | Cumulative since T-0 | Big number + sentiment-pie (manual classification) |
| HN Show HN rank | Manual entry every hour during T-0..T+1d | Time series | Line chart |
| Product Hunt rank | Manual entry every hour during T-0..T+1d | Time series | Line chart |
| LinkedIn post engagement | LinkedIn analytics | Likes + comments + shares cumulative | Big numbers |

**Note:** Manual = WR-COORD enters values every 60 min during T-0..T+24h, every 6h thereafter. Automation deferred to a post-launch follow-up.

---

## 3. Data sources summary

| Source | What it provides | Auth |
|---|---|---|
| Clerk webhooks | Signup events | HMAC-signed webhook |
| Prometheus / Grafana | SLO metrics | Internal mTLS |
| CAS write event stream (Kafka or NATS) | First-CAS-write detection | Internal mTLS |
| Synthetic E2E pipeline (`tests/e2e/signup-launch-day.spec.ts`) | Signup-to-first-CAS-write health | Service account |
| Linear support board | Ticket volume + category | Linear API token |
| HubSpot / support@corelink.dev | Support intake | HubSpot API token |
| ahrefs Brand Mentions | Twitter / press mentions | ahrefs API |
| Statuspage public view | Incident overlay | Public |
| PagerDuty | SEV2/SEV1 page-outs | PagerDuty webhook |

---

## 4. Alert wiring

- **Signup rate > 10x projected (5-min window)** → SRE-OC investigates bot signature. No page-out; war room channel ping.
- **Activation < 10% for any hour cohort** → CS-OC + CTO investigate. No page-out; war room channel ping.
- **Critical support ticket flag > 0** → PagerDuty pages CTO. SEV-determined post-triage.
- **Any top-5 SLO burn > 2% in 1h** → PagerDuty SEV2 → `STATUS-PAGE-SPEC.md` partial-outage flow.
- **Status page itself shows red** → PagerDuty SEV1 → war room war room.

---

## 5. Snapshot artifacts (for retros)

At T+24h and T+72h, WR-COORD exports:
- `marketing/launch/metrics/SNAPSHOT-T+24H.md`
- `marketing/launch/metrics/SNAPSHOT-T+72H.md`

Each contains: panel screenshots, raw numeric snapshots, categorized support tickets, press mention log, decision log highlights.

---

## 6. War room display

The war room screen rotates between:
1. The Grafana Day-1 dashboard (most of the time).
2. The Statuspage public view (5 sec every 5 min — to see what customers see).
3. The Twitter live search for `@corelinkdev` (5 sec every 5 min).

If physical war room: 4K monitor with all three tiled. If remote-only: Zoom screen-share rotated by WR-COORD.

---

## 7. Anti-metrics (do not put on this dashboard)

- Revenue. Cumulative MRR. ARR. **No money on the launch-day dashboard.** Reasons: most signups won't convert in 24h; revenue noise in 24h leads to bad decisions; sales metrics belong on a different dashboard.
- Vanity counts. Total cache hits, total CAS writes. These are operational metrics, not launch-day signal.
- Sentiment "score" without manual classification. We classify by hand on launch day or we don't classify.

---

## 8. Fallback queries (if dashboard is down)

If Grafana is unreachable, the following raw queries reproduce each panel's headline numbers from CLI. Stored at `marketing/launch/queries/day-1-fallback.md` (created at H-18 provisioning).

| Panel | Tool | Query |
|---|---|---|
| M1 Signups | `psql` (Clerk replica) | `SELECT count(*) FROM users WHERE created_at > now() - interval '1 hour';` |
| M2 Activation | `psql` (analytics replica) | `SELECT count(distinct tenant_id) FROM cas_writes WHERE created_at > '<T-0>'::ts;` divided by signup count |
| M3 Tickets | Linear API | `linear list-issues --team support --created-after <T-0>` |
| M4 SLO burn | `promtool query instant` | `corelink_api_request_duration_seconds:burn_rate_1h` (etc.) |
| M5 Mentions | Manual | n/a |

---

## 9. Cross-references

- `LAUNCH-CHECKLIST-V2.md` — references this dashboard at L18 (final dashboard check), L28 (T+15 min SLO check), L31 (signup snapshot), L41 (T+24h metrics capture).
- `STATUS-PAGE-SPEC.md` — SLO burn alerts flow into status page severity mapping.
- `marketing/launch/METRICS-DASHBOARD.md` — superset / longer-window dashboard for T-0..T+7d.
- `specs/_runbooks/RB-LAUNCH-WAR-ROOM-COORDINATION.md` — operator playbook that consumes this dashboard.
- `ROADMAP-TO-GA.md` §8 — R-8 Launch wave parent.
