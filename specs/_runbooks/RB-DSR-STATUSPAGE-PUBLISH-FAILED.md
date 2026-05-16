---
id: "RB-DSR-STATUSPAGE-PUBLISH-FAILED"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "S-17"
parent_wi: "WI-S11-002"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RB-DSR-GDPR"
  - "RB-DSR-LGPD-FULL"
  - "ONCALL-ESCALATION-MATRIX"
  - "privacy_model"
tags: ["runbook", "dsr", "statuspage", "privacy", "sev-1", "gdpr-art-30", "lgpd-art-37", "transparency", "fail-closed", "wave-18"]
---

# RB-DSR-STATUSPAGE-PUBLISH-FAILED — DSR Statuspage daily publish failure (SEV-1)

> **Status:** DRAFT. Owner: Gustavo Schneiter.
>
> **Scope:** operator response when the PagerDuty rule
> `DsrStatuspagePublishFailed` fires. The rule counts emits of the
> audit event `corelink.privacy.statuspage_publish_failed.v1` from the
> wave-17 DSR Statuspage publish scheduler
> (`crates/corelink-dsr-statuspage-scheduler`; cron `0 6 * * *` UTC
> wired into `crates/corelink-clerk-cf::dsr_statuspage_cron`).
>
> **Severity:** **SEV-1**. The DSR worker itself (full erasure
> processing per `RB-DSR-GDPR` / `RB-DSR-LGPD-FULL`) is **unaffected**
> by this failure — the wave-17 scheduler is a customer-visible
> transparency surface (the daily p95 of `dsr_resolution_hours` posted
> to the public Atlassian Statuspage metric). The page exists because
> the customer-visible metric stopping is a **GDPR Art. 30 / LGPD Art.
> 37 records-of-processing public-facing transparency** obligation; an
> outage > 7d requires outside-counsel notification (§5).
>
> **MTTA target:** **4 hours** (24/7 per `observability_model.md` §9.2;
> longer than the SEV-0 / SEV-1 security floor because the publish path
> itself does NOT block DSR completion — the cron will reconcile on the
> next tick at 06:00 UTC, so a 4h MTTA still beats the next firing
> window in every timezone).
>
> **MTTR target:** **24 hours** end-to-end (per S-17 ops maturity
> baseline). Drift > 2× (48h) → fitness-function regression
> (`corelink-runbook-tracker`); drift > 7d triggers the outside
> counsel notification path in §5.
>
> **Companion docs / refs:**
> - `dashboards/alerts/dash-dsr-statuspage-alerts.yml` (the PD alert rule)
> - `crates/corelink-dsr-statuspage-scheduler/src/audit.rs` (the 4 canonical event types)
> - `crates/corelink-dsr-statuspage-scheduler/src/scheduler.rs` (the cron orchestration)
> - `crates/corelink-statuspage-real/src/http.rs` (the wave-16 HTTP wiring)
> - `specs/03_architecture/observability_model.md` §9 (severity + clock semantics)
> - `specs/03_architecture/privacy_model.md` §6.2 (canonical DSR pipeline)
> - `specs/_audits/2026-05-15-dsr-worker-production.md` §5.6 (wave-17 scheduler surface)
> - `specs/_runbooks/RB-DSR-GDPR.md` (DSR processing — separate concern)
> - `specs/_runbooks/RB-DSR-LGPD-FULL.md` (DSR processing — separate concern)
> - `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md`

---

## 1. Detect

The alert rule `DsrStatuspagePublishFailed` fires when

```promql
sum (
  increase(corelink_dsr_statuspage_publish_failed_total[5m])
) > 0
```

i.e. **any** emit of `corelink.privacy.statuspage_publish_failed.v1` in
a rolling 5-minute window. The audit envelope does not key on
`tenant_id_hex8` (the DSR Statuspage publish is system-scoped — one
publish per UTC day per `(page_id, metric_id)`); the PagerDuty incident
body therefore carries the BLAKE3-pseudonymised `page_id_hex8` +
`metric_id_hex8` ONLY, never the raw Atlassian identifiers or the
`STATUSPAGE_API_KEY` (per INV-AUTH-AUDIT-PSEUDONYMIZATION +
CTRL-PRIV-001 + the wave-16 `redact_api_key` last-4 semantics).

A 5-min consecutive-failure burn is the **early-detection** target —
the cron fires at 06:00 UTC daily, and the wave-17 retry policy is 3
attempts exponential-backoff on 5xx / 429 within the same tick. A 5-min
window therefore catches every failure inside a single cron firing
**before** the operator's next-business-day mailbox notice.

**Step 1.1 — Acknowledge the page within 4 hours.** From PagerDuty
mobile or `/ack` in `#corelink-alerts` Slack thread.

**Step 1.2 — Pull the offending audit row.** The scheduler emits one
`corelink.privacy.statuspage_publish_failed.v1` event per failed tick.
Query the audit-chain D1 mirror for the last 30 minutes:

```bash
wrangler d1 execute corelink-audit \
  --command "SELECT
    event_id,
    occurred_at,
    date_yyyymmdd,
    page_id_blake3_hex8,
    metric_id_blake3_hex8,
    rows_read,
    p95_hours_observed,
    reason
  FROM audit_events_index
  WHERE event_type = 'corelink.privacy.statuspage_publish_failed.v1'
    AND occurred_at >= datetime('now', '-30 minutes')
  ORDER BY occurred_at DESC;"
```

The `reason` field is the canonical diagnostic; map it to §2 triage.

---

## 2. Triage

Classify the failure by the `reason` token in the audit envelope. The
wave-17 scheduler surfaces three canonical buckets:

| `reason` prefix | Classification | Triage path |
|---|---|---|
| `statuspage_auth_failed:401` / `:403` | Statuspage 4xx auth/scope | §2.1 — credential or scope drift |
| `statuspage_failed:5xx` / `retry_exhausted_5xx` | Statuspage vendor 5xx outage | §2.2 — vendor outage |
| `statuspage_rate_limited:429` post-retry | Vendor rate-limit floor | §2.2 — vendor outage (degraded) |
| `transport:` / `network:` / `dns:` | Network / egress outage | §2.3 — network outage |
| `d1_read_failed:` | D1 row source read error | §2.4 — internal D1 issue (NOT a Statuspage problem) |

### 2.1 Statuspage 4xx auth / scope drift

```bash
# 1. Confirm the secret is present and the last-4 matches expectation.
corelink admin secrets show \
  --row 42 \
  --format last4

# 2. Statuspage API key has not been rotated mid-run?
#    (Cross-check secrets-checklist.md row 42 last-rotated timestamp.)
grep -A2 "STATUSPAGE_API_KEY" docs/internal/secrets-checklist.md
```

If the secret rotated within the last 7d and the cron's worker binding
still has the old value → re-deploy the worker. If the secret is fresh
but Statuspage still rejects (401/403), open a Statuspage support
ticket — the scope on the OAuth credential may have been narrowed.

**Do NOT print the secret to the terminal or paste it into Slack.**
The `redact_api_key` helper exposes last-4 only; that is the only
form that may appear in an incident channel.

### 2.2 Statuspage vendor 5xx / 429 outage

```bash
# 1. Confirm the Statuspage status page itself (yes — Atlassian
#    Statuspage publishes its own status on a separate Statuspage):
curl -sSf https://metastatuspage.com/ | head -50

# 2. Cross-check the wave-16 retry policy already exhausted 3 attempts:
wrangler d1 execute corelink-audit \
  --command "SELECT retry_count, reason
             FROM audit_events_index
             WHERE event_type = 'corelink.privacy.statuspage_publish_failed.v1'
               AND occurred_at >= datetime('now', '-30 minutes');"
```

Vendor outage > 24h → §3.1 (customer email path).

### 2.3 Network / egress outage

Cross-check the Cloudflare Workers egress dashboard for the cron
worker's outbound IP block. If wider Workers egress is down, the DSR
publish is one symptom of many — coordinate with the active-platform
oncall (`RB-ACTIVE-FAILOVER`).

### 2.4 D1 read failure

This is **not a Statuspage problem** — the cron failed before any HTTP
call. Switch to `RB-D1-MIGRATION-APPLY` §3 diagnostics for the
`corelink-audit` and `corelink-dsr` D1 databases (the row source
queries the DSR ticket index). The wave-17 scheduler audit envelope
still emits `corelink.privacy.statuspage_publish_failed.v1` with the
`d1_read_failed:` reason prefix so the page is the same; the recovery
path differs.

---

## 3. Containment

The DSR worker (full erasure processing) is **NOT affected** by this
failure — the cron is a transparency surface, not a critical path.
Containment is therefore about the **customer narrative**, not the
DSR pipeline.

### 3.1 Statuspage vendor outage > 24h — customer email

If the Statuspage vendor outage exceeds 24h (i.e. the alert has been
firing on consecutive 06:00 UTC ticks for ≥ 2 days), send the
customer-comm template below. Coordinate with Privacy + Marketing
before sending.

> Subject: `[Notice] CoreLink DSR completion metric temporarily unavailable`
>
> Hi `<name>`,
>
> Our public DSR completion metric on `corelink.humangr.com/status` is
> temporarily unavailable due to a vendor outage at our status-page
> provider (Atlassian Statuspage). The metric will resume publishing
> as soon as the vendor recovers.
>
> **Important:** full DSR processing is unaffected. Every data-subject
> request submitted via `corelink.humangr.com/dsr` continues to be processed
> against our canonical 30-day GDPR Art. 12 / 15-day LGPD Art. 19
> deadlines; only the public reporting of the aggregated p95 metric
> is paused.
>
> We will update this notice when the vendor restores service.
>
> — CoreLink Privacy

The email goes to every tenant Privacy Contact in the DPA roster. Drata
evidence task: `EVT-DSR-STATUSPAGE-OUTAGE-<YYYY-MM-DD>`.

### 3.2 Internal degraded-mode banner

```bash
# Surface the degraded mode on the internal monitoring dashboard
# (NOT the public Statuspage — which is broken). Grafana annotation:
corelink admin grafana annotation create \
  --dashboard dash-dsr-overview \
  --text "DSR Statuspage publish paused — RB-DSR-STATUSPAGE-PUBLISH-FAILED §3.1" \
  --tags "incident,sev-1,dsr-statuspage"
```

---

## 4. Mitigation

The wave-17 scheduler is **self-healing** — the cron at 06:00 UTC will
fire again on the next tick and the dedupe ledger (`cron_run_log` with
`(date_yyyymmdd, metric_id)` PRIMARY KEY) prevents double-publishing.
Operator mitigation steps:

### 4.1 Manual retry (one-shot, BEFORE the next cron tick)

```bash
# 1. Confirm the dedupe ledger does NOT already have today's row.
wrangler d1 execute corelink-dsr \
  --command "SELECT date_yyyymmdd, metric_id, recorded_at
             FROM cron_run_log
             WHERE date_yyyymmdd = CAST(strftime('%Y%m%d','now') AS INTEGER);"

# 2. If no row → manually trigger the scheduler tick.
corelink admin cron trigger \
  --name dsr_statuspage_cron \
  --reason "RB-DSR-STATUSPAGE-PUBLISH-FAILED §4.1 manual retry" \
  --paged-runbook RB-DSR-STATUSPAGE-PUBLISH-FAILED
```

The manual trigger emits the same 4 canonical events
(`statuspage_publish_scheduled.v1` →
`statuspage_publish_{succeeded,failed,skipped}.v1`) — the audit chain
treats it identically to a cron-fired tick.

### 4.2 Backoff + jitter (do NOT thunder the vendor)

The wave-16 `RetryPolicy` already implements exponential backoff (3
attempts on 5xx + 429) with deterministic jitter. **Do NOT loop the
manual retry** — give the vendor at least 15 min between attempts.

### 4.3 Cron auto-reconcile

If §4.1 manual retry also fails, do NOT escalate to §3.1 immediately —
let the **next cron tick at 06:00 UTC** retry naturally. The 24h gap
between firings is the canonical recovery window; an operator
thrashing the vendor between firings can trip Statuspage's account-
level rate-limiter and lengthen the outage.

---

## 5. Compliance impact (GDPR Art. 30 / LGPD Art. 37)

The DSR Statuspage publish is a **records-of-processing transparency**
surface, **not** a regulatory submission. The Statuspage metric is
voluntary customer-facing communication — it is not the canonical
RoPA / Art. 30 register (those are the documents in
`specs/_compliance/`).

### 5.1 Short outage (≤ 7 days)

No regulatory notification is required. Document the gap in the
post-incident memo (§7); the DPO reviews the SEV-1 incident at the
weekly compliance review per `RB-COMPLIANCE-WEEKLY-REVIEW.md`.

### 5.2 Extended outage (> 7 days)

The customer-facing DSR transparency surface being dark for > 7 days
is a documented **transparency gap** under GDPR Art. 30 / LGPD Art. 37
records-of-processing public-facing obligations (although the
underlying RoPA is unaffected — see §5.1). Action:

1. **Outside counsel notification** — open ticket
   `LEGAL-DSR-TRANSPARENCY-<YYYY-MM-DD>` with the rationale + the
   incident memo (§7). Outside counsel decides whether the gap rises
   to a notifiable matter under either regime.
2. **DPO escalation** — page the DPO via `RB-DPO-ESCALATION` §2.
3. **Vendor escalation** — open a Statuspage executive-tier support
   ticket; CoreLink Privacy + Legal joint sign-off.
4. **Document the workaround** — publish the aggregated p95 in the
   monthly compliance report (`specs/_compliance/`) instead of the
   public Statuspage, until vendor recovery.

### 5.3 LGPD / GDPR posture summary

| Regime | Article | This incident's classification | Notification clock |
|---|---|---|---|
| GDPR | Art. 30 (records of processing) | Transparency-channel outage; RoPA intact | N/A unless > 7d (§5.2) |
| GDPR | Art. 33 (data-breach notification 72h) | **Does NOT apply** — no personal data disclosed | N/A |
| LGPD | Art. 37 (records of processing) | Transparency-channel outage; RoPA intact | N/A unless > 7d (§5.2) |
| LGPD | Art. 46 (security incident) | **Does NOT apply** — no incident-of-confidentiality | N/A |

---

## 6. Resolution criteria

Close the SEV-1 only when **3 consecutive cron ticks at 06:00 UTC**
publish successfully — i.e. 3 consecutive
`corelink.privacy.statuspage_publish_succeeded.v1` emits with no
intervening `_failed.v1`. The 3-tick rule prevents premature closure
during an intermittent vendor outage.

```bash
# Verify the last 3 daily ticks all succeeded.
wrangler d1 execute corelink-audit \
  --command "SELECT date_yyyymmdd, COUNT(*) FILTER (
               WHERE event_type = 'corelink.privacy.statuspage_publish_succeeded.v1'
             ) AS ok_count,
             COUNT(*) FILTER (
               WHERE event_type = 'corelink.privacy.statuspage_publish_failed.v1'
             ) AS fail_count
             FROM audit_events_index
             WHERE event_type IN (
               'corelink.privacy.statuspage_publish_succeeded.v1',
               'corelink.privacy.statuspage_publish_failed.v1'
             )
             AND occurred_at >= datetime('now', '-72 hours')
             GROUP BY date_yyyymmdd
             ORDER BY date_yyyymmdd DESC
             LIMIT 3;"
```

Expected output: 3 rows, each `ok_count = 1` and `fail_count = 0`.

---

## 7. MTTA / MTTR target + closure

**MTTA target ≤ 4h.** **MTTR target ≤ 24h** per S-17 ops maturity
baseline. Closure checklist:

- [ ] §1.2 audit row pulled; `reason` token classified.
- [ ] §2 triage path executed; root cause documented.
- [ ] §4.1 manual retry attempted (or §4.3 cron auto-reconcile observed).
- [ ] §3.1 customer email sent (if vendor outage > 24h).
- [ ] §6 three-consecutive-success verification GREEN.
- [ ] §5 compliance posture documented (no action / outside-counsel notification).
- [ ] Incident memo filed at
      `specs/_post_mortems/PM-DSR-STATUSPAGE-PUBLISH-FAILED-<YYYY-MM-DD>-<ticket>.md`
      within 5 business days.

---

## 8. Post-incident actions

For every SEV-1:

1. Post-mortem within 5 business days
   (`specs/_post_mortems/PM-DSR-STATUSPAGE-PUBLISH-FAILED-<YYYY-MM-DD>-<ticket>.md`).
2. If the failure pattern is vendor 5xx > 24h **repeatedly** (≥ 2
   incidents/quarter): open `WI-S17-OPS-DSR-STATUSPAGE-VENDOR-RESILIENCE`
   to add a fallback publish path (e.g. mirroring the metric to a
   self-hosted status surface).
3. If the failure pattern is `d1_read_failed:` repeatedly: open
   `WI-S17-OPS-DSR-D1-RESILIENCE` — the D1 read in the cron path is a
   stable-narrative hotspot.
4. Tag the runbook execution in the runbook-drill tracker:

   ```bash
   corelink runbook-drill record \
     --runbook-id RB-DSR-STATUSPAGE-PUBLISH-FAILED \
     --executor op_<id> \
     --evidence <asciinema-url> \
     --duration-seconds <ACTUAL> \
     --expected-seconds 86400
   ```

---

## 9. Fitness function

This runbook MUST be drilled semi-annually via a synthetic Statuspage
401 / 503 / 429 injection fired against the staging publish path
(`corelink admin chaos dsr-statuspage-publish-fail
--reason-token <token>`). Expected end-to-end drill duration **2
hours** (compressed from the 24h MTTR target — the drill validates the
path, not the regulatory wait). Drift > 2× (4h) triggers FM-202 review
per `corelink-runbook-tracker`.

---

## 10. See also

- `specs/_runbooks/RB-GA-CUTOVER.md` §0.2.9 + §0.5 + §3.8 + §4 G6 + §6.1.4 — GA cutover Statuspage banner pre-staging + DSR cron enablement + greenlight criterion G6 (DSR cron 100% success 24h) + T+24h cron review; pre-cutover re-read mandatory.
