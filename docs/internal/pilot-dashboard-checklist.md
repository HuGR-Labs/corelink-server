# CoreLink Pilot Dashboard Checklist

> **Audience:** the engineer building (or auditing) the pilot
> dashboard, plus the CS team that triages from it daily.
>
> **Status:** living checklist. The dashboard MUST surface these
> 10 metrics per-tenant. Additional panels are welcome; these are
> the floor.
>
> **Companion docs:**
> - `docs/internal/customer-success-playbook.md` — defines the
>   targets, yellow/red thresholds, and escalation triggers that
>   the metrics below feed into.
> - `docs/internal/pilot-comms-templates.md` — the canned messages
>   we send when a metric flips state.
>
> **Operational rule:** CS reviews this dashboard at 09:00 local
> time every business day. Any metric in YELLOW for ≥ 7 days OR
> any metric in RED for ≥ 48 hours triggers escalation per
> playbook §5 *without waiting for a scheduled review*.

---

## Dashboard requirements (cross-cutting)

Before the metrics list — these are the structural requirements
the dashboard MUST satisfy regardless of which BI tool we host it on:

- **Per-tenant filter** as the primary axis. The "fleet view" is
  a secondary panel, not the default.
- **Time window selector** with presets: 24h, 7d, 30d, pilot-to-date.
- **Traffic-light coloring** on every metric (green / yellow / red)
  matching the thresholds in the playbook §4.
- **Drill-down to raw events** for any metric (a panel that
  returns "looks bad" with no way to see which requests caused it
  is a dashboard bug).
- **Refresh cadence** ≤ 5 minutes for usage/latency panels;
  ≤ 1 hour for billing/storage panels.
- **Per-tenant CSV export** for the comms templates that need
  populated metric blocks (mid-pilot review email §4 in particular).

---

## The 10 metrics

### 1. Activation status & time-to-activation

**Definition:** boolean (activated y/n) plus the elapsed time
from signup-link issuance to the first successful authenticated
API call.

**Why it matters:** the leading indicator for whether the pilot
will hit time-to-first-blob within 24h. If activation hasn't
happened by T+4h, comms template §2 fires.

**Source:** auth-event stream filtered to `auth.activation.success`
joined to the pilot tracker's signup-link issuance event.

**Thresholds:** green ≤ 4h, yellow 4–24h, red > 24h.

---

### 2. Time-to-first-blob

**Definition:** elapsed time from activation to the first
`cas.write.success` event with this tenant's ID.

**Why it matters:** the single best leading indicator of pilot
success (playbook §3). Below 24h is "the pilot is working." Above
72h and the pilot is structurally at risk.

**Source:** audit log stream, `cas.write.success` events.

**Thresholds:** green ≤ 24h, yellow 24–72h, red > 72h.

---

### 3. Time-to-first-audit-export

**Definition:** elapsed time from activation to the first
successful audit export pulled via the admin API.

**Why it matters:** proves the compliance flow works end-to-end.
Pilots that never export audit logs cannot pass the day-30
conversion criteria.

**Source:** admin-plane access log, `audit.export.success` events.

**Thresholds:** green ≤ 7d, yellow 7–14d, red > 14d.

---

### 4. CAS storage utilization (cumulative & growth rate)

**Definition:** two panels — (a) total GB stored as of now;
(b) daily growth-rate in GB/day over the last 7 days.

**Why it matters:** the conversion criterion (§6) requires
≥ 50 GB at day 30. The growth-rate panel tells us whether the
tenant is on-track *before* day 30. Also catches the 100 GB
pilot-cap breach early (playbook §8).

**Source:** storage-quota subsystem aggregates.

**Thresholds (cumulative, day 30 projection):**
green ≥ 50 GB, yellow 25–49 GB, red < 25 GB.
**Cap alert:** red flash at ≥ 90 GB (10 GB from the 100 GB pilot cap).

---

### 5. Audit event volume (cumulative & per-day)

**Definition:** count of audit events emitted for this tenant,
both cumulative and the daily rate.

**Why it matters:** the conversion criterion (§6) requires
≥ 10,000 audit events at day 30. Low audit volume is also a
signal that the tenant isn't really using us — they may be doing
all their work on a fallback path.

**Source:** audit event stream, count grouped by tenant_id.

**Thresholds (cumulative, day 30):** green ≥ 10k, yellow 5k–9.9k,
red < 5k.

---

### 6. Latency (p50 / p95 / p99) on CAS read & write

**Definition:** percentile latency panels for `cas.read` and
`cas.write`, computed over rolling 24h, 7d, and pilot-to-date.

**Why it matters:** SLO baseline capture happens at T+7 to T+14
(playbook §3). Any latency baseline materially worse than our
published SLOs is a RED status — likely misconfiguration or
network path issue that needs SRE involvement.

**Source:** request-tracing telemetry (per-operation histograms).

**Thresholds:** match published per-tier SLOs. Yellow at +20%
over SLO; red at +50% or any SLO breach in the window.

---

### 7. Availability (last 24h, last 7d, last 30d)

**Definition:** successful-requests / total-requests * 100, with
"successful" defined per our SLO definition (excludes client-4xx).

**Why it matters:** the conversion criterion (§6) requires
≥ 99.5% over 30 days. Visibility on the 30-day window must
update in real-time so we can spot a breach trajectory by day 25
and trigger the SLO-breach comms (template §5) and the conversion
discussion early.

**Source:** request-log aggregations.

**Thresholds:** green ≥ 99.5%, yellow 99.0–99.49%, red < 99.0%.

---

### 8. Support ticket volume & open age

**Definition:** two sub-panels — (a) tickets opened in the last
7d for this tenant; (b) age of the oldest open ticket.

**Why it matters:** ticket volume > 3/week is yellow (playbook
§4). The age panel catches tickets that should have escalated to
Tier 2 but didn't (Tier 1 SLA is 4 business hours per playbook §5).

**Source:** support ticket tracker, filtered by tenant tag.

**Thresholds (weekly volume):** green ≤ 3, yellow 4–7, red > 7.
**Thresholds (oldest open):** green ≤ 4h, yellow 4–8h, red > 8h.

---

### 9. SLO breach count (rolling 30 days)

**Definition:** count of distinct SLO breaches attributable to
CoreLink-side cause affecting this tenant in the last 30 days.

**Why it matters:** the conversion criterion (§6) requires 0
P0 incidents and 0 SEV-1 audit discrepancies. Each breach also
triggers comms template §5 in real-time. The dashboard panel is
the audit trail.

**Source:** incident-management system, filtered to incidents
tagged with this tenant_id.

**Thresholds:** green 0, yellow 1–2, red ≥ 3.

---

### 10. BYOK key health & rotation status (if BYOK enabled)

**Definition:** for tenants with BYOK enabled — (a) key
material age in days; (b) last successful rotation timestamp;
(c) any failed encrypt/decrypt operations in the last 7d.

**Why it matters:** the conversion criterion (§6) requires at
least one successful BYOK rotation if BYOK is in use. Stale keys
or failed crypto ops are a P0 signal — encrypt failure means we
can't write; decrypt failure means data is at risk of being
unreadable. Either is an automatic on-call SRE page.

**Source:** BYOK subsystem telemetry.

**Thresholds:** green = rotated within 90d AND zero failures;
yellow = 90–180d since rotation OR ≤ 3 failures; red = > 180d
OR ≥ 4 failures OR any failure in the last 24h.

**N/A handling:** for tenants without BYOK, the panel displays
"BYOK not enabled" rather than green-by-default — silent N/A
states have masked real outages in past incidents.

---

## Appendix: panels we explicitly do NOT need (yet)

To save the dashboard builder time — these are the metrics that
keep getting suggested but are pilot-phase out-of-scope:

- **Cost-per-tenant.** Pilot is free; STANDARD billing kicks in
  at GA. Track at GA.
- **MAU / DAU.** Workload-driven, not user-driven. Misleading at
  the cache layer.
- **NPS as a panel.** The day-15 NPS is a single-question email
  survey (playbook §4); the result is captured in the pilot
  tracker, not the dashboard.
- **Geo heatmaps.** Pilot tenants are single-region by ICP
  (playbook §1). Re-evaluate at GA when multi-region rollouts
  start.
- **Churn risk score.** We have ten pilots, not ten thousand —
  the CS owner *is* the churn risk model.
