---
id: "RB-NEON-SHADOW-LAG"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "R-PREP-WAVE-19"
parent_wi: "WI-S09-NEON-ANALYTICS-SHADOW"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RB-AUDIT-EXPORT-INTEGRITY"
  - "security_model"
tags: ["runbook", "audit", "neon-shadow", "analytics", "sev-2", "soc2", "cc7.2", "reconciliation"]
---

# RB-NEON-SHADOW-LAG — Neon analytics shadow lag + drift triage

> **Status:** DRAFT. Owner: Gustavo Schneiter.
>
> **Scope:** SEV-2 triage when the `audit_events_shadow` Neon analytics
> mirror falls behind the canonical R2 NDJSON archive
> (`corelink.audit.neon_shadow_sync_failed.v1` emit OR
> `NeonShadowLagSev2` / `NeonShadowDriftSev2` PD page). The shadow is
> ANALYTICS-ONLY — chain integrity remains authoritative on R2.
>
> **Severity classification:** SEV-2 (analytics lag), NEVER SEV-0.
> The R2 archive remains the canonical chain-integrity store
> (`INV-OBS-AUDIT-CHAIN-INTEGRITY` HIGH). A Neon shadow miss does NOT
> trigger the GDPR Art. 33 / LGPD Art. 48 72h breach-notification
> clock — analytics lag is operational, not a personal-data
> integrity / confidentiality incident.

## 1. Source-of-truth invariant

**The R2 NDJSON archive is the canonical chain store.** Every chain-
integrity question (`corelink audit verify <export>` failure, daily
verifier SEV-0, customer audit, regulator inquiry) MUST be answered
against R2. The Neon shadow is a convenience read tier with:

- **Nominal lag:** ≤ 5 min (matches the R2 archive producer's
  `DEFAULT_FLUSH_AFTER_MS = 5 min` cadence).
- **SEV-2 ceiling:** ≥ 60 min (`SHADOW_LAG_SEV2_THRESHOLD_MS = 3_600_000 ms`;
  pinned by `crates/corelink-audit-chain/src/neon_shadow.rs` constants).

The two failure scenarios this runbook covers — **lag** and **drift** —
are observationally distinct:

| Scenario | Detection | Page rule | Window |
|---|---|---|---|
| **Lag** | `observed_lag_ms ≥ 3_600_000` for 5 consecutive samples | `NeonShadowLagSev2` (Alertmanager) | 5-min rolling |
| **Drift** | Daily reconciliation cron: `R2_count != shadow_count` for any `(tenant, region, date)` | `NeonShadowDriftSev2` (PD direct from the workflow) | per-tenant-per-day |

## 2. Lag scenario (`NeonShadowLagSev2` PD page)

### 2.1 Detect

Either:

- PagerDuty page `NeonShadowLagSev2` (Alertmanager rule fires on
  `corelink_neon_shadow_observed_lag_ms_p99{region}` ≥ `3_600_000` for
  5 min) — see `dashboards/alerts/dash-neon-shadow-alerts.yml`.
- OR `corelink.audit.neon_shadow_sync_failed.v1` SEV-2 emit observed
  in the audit pipeline (success-arm-with-sev-2 means the sync
  succeeded BUT the lag at attempt time ≥ 60 min; failure-arm means
  the sync itself failed — both surface here).

### 2.2 Diagnose (in order)

```sql
-- A. Per-region last successful sync timestamp + lag (run in the
--    per-region Neon SQL console; the project DSN is in
--    `NEON_DB_URL_<REGION>` — `corelink-audit-chain::EnvVarResolver`
--    canonicalizes the env-var naming).
SELECT
  region,
  MAX(synced_at)                              AS last_synced_at,
  EXTRACT(EPOCH FROM NOW() - MAX(synced_at))  AS lag_seconds,
  MAX(seq)                                    AS last_synced_seq
FROM audit_events_shadow
WHERE event_time >= NOW() - INTERVAL '2 hours'
GROUP BY region
ORDER BY region;

-- B. Neon project status (Neon Console → project list → status badge).
--    GREEN  = project healthy.
--    YELLOW = compute autosuspended (cold start expected; bounded by
--             `NEON_AUTOSUSPEND_DELAY_SECONDS`, default 300).
--    RED    = project incident — escalate per §2.4 immediately.

-- C. Replication slot health (Neon manages this internally; if the
--    Neon platform exposes `pg_replication_slots` for the customer
--    DSN, this catches a stuck slot ahead of the project page).
SELECT slot_name, active, restart_lsn, confirmed_flush_lsn
FROM pg_replication_slots
WHERE slot_name LIKE 'corelink_audit_shadow%';
```

### 2.3 Decide

| Lag value | Action |
|---|---|
| 60–120 min, Neon project GREEN | Likely transient (R2-side flush stall). Watch one more 5-min sample; if it clears, write a brief postmortem note + close the page. |
| 120 min – 24 h, Neon project GREEN | Catch-up replay (§2.4 step A). The shadow IS divergent from R2 — operate the catch-up before customer analytics queries observe a 24h-stale window. |
| Neon project YELLOW (autosuspended) | The shadow sink retries with exponential backoff. If the page is still firing 10 min after the project flips back to GREEN, escalate per `ONCALL-ESCALATION-MATRIX.md` row 4 (Database). |
| Neon project RED | Region failover (§2.4 step B). DO NOT replay against a red project — wait for Neon platform to clear, then resume from the last-synced `seq`. |
| Sustained ≥ 24 h | Region failover (§2.4 step B) + disable sink (§2.4 step C) until the platform is GREEN + the catch-up replay completes. Notify customers via the status page (DSR statuspage publish path) when the customer-facing analytics endpoints are HTTP 503'd by the disable. |

### 2.4 Mitigations

**A. Catch-up replay** (preferred; preserves the canonical sync path):

1. Identify the last successfully-synced `seq` per `(tenant_id, region)`
   from the Diagnose §2.2A query.
2. Trigger the catch-up replay tool (the
   `archive_producer::ArchiveProducer` daemon re-emits any chunk that
   has not been successfully shadow-sync'd within the lag SLO; the
   daemon is idempotent via `ON CONFLICT (tenant_id, seq) DO NOTHING`
   on the shadow table PRIMARY KEY).
3. Watch the per-region `corelink_neon_shadow_observed_lag_ms_p99`
   panel; the lag should drop into the 5-min nominal band within 1
   replay cycle (≤ 60 min).

**B. Region failover** (only when Neon project RED OR sustained ≥ 24 h):

1. The shadow is per-region; failing a region over does NOT affect
   other regions. Disable the per-region sink via
   `RealNeonShadowSink::new` being skipped for that region in the
   server boot wire (env-var `NEON_DB_URL_<REGION>` unset by SRE).
2. R2 archives keep flowing (the canonical chain integrity is
   preserved). Customer analytics endpoints (`/v1/audit/analytics/*`)
   for the failed region return HTTP 503 with the canonical
   `region-shadow-disabled` exit-status until the project is back.

**C. Sink disable** (catastrophic — only with on-call lead approval):

1. Unset all `NEON_DB_URL_*` env vars at the binary boot path.
2. The `audit_analytics::ShadowSinkFactory` returns a 503 for every
   tenant. The R2 archive producer remains unaffected.
3. File a P2 incident; the daily reconciliation cron continues to
   run against R2 alone (it will report missing-shadow rows for the
   disable window — that is informational, NOT a SEV escalation).

## 3. Drift scenario (`NeonShadowDriftSev2` PD page)

### 3.1 Detect

PagerDuty page from
`.github/workflows/neon-shadow-reconcile-daily.yml`. The workflow
compares, for the previous UTC day:

- **R2 row count** for `(tenant_id, region, date)` — counted by
  walking `audit/{tenant_id}/{YYYY-MM-DD}/*.cloudevent.ndjson`.
- **Shadow row count** for the same window — counted by
  `SQL_RECONCILE_COUNT` (`SELECT COUNT(*) FROM audit_events_shadow
   WHERE event_time >= … AND event_time < …`).

A non-zero diff fires PD with `dedup_key=neon-shadow-drift-<region>-<date>`.

### 3.2 Diagnose

```sql
-- A. Locate the missing seqs per tenant. Run on the per-region Neon
--    project (the RLS GUC must be set to the tenant under investigation).
SELECT MIN(seq), MAX(seq), COUNT(*) AS observed
FROM audit_events_shadow
WHERE event_time >= to_timestamp(:from_ms::double precision / 1000.0)
  AND event_time <  to_timestamp(:to_ms::double precision   / 1000.0);

-- B. Confirm the R2 archive carries those seqs. The R2 NDJSON layout
--    is `audit/{tenant_id}/{YYYY-MM-DD}/{seq:08}.cloudevent.ndjson`
--    (per `corelink-audit-chain::canonical_r2_key`).
--    A missing seq in BOTH R2 + shadow is a chain break — escalate
--    to RB-AUDIT-EXPORT-VERIFY-FAILED (SEV-0).
```

### 3.3 Decide

| Drift shape | Severity | Action |
|---|---|---|
| Shadow row count < R2 row count (shadow missing rows) | SEV-2 (analytics lag) | Catch-up replay §2.4 step A. |
| Shadow row count > R2 row count (shadow has extra rows) | SEV-1 (audit append-only invariant violated on shadow) | Snapshot the divergent rows + open a P1 audit incident. The shadow PRIMARY KEY `(tenant_id, seq)` + `ON CONFLICT DO NOTHING` should make this impossible — investigate a wiring bug in `RealNeonShadowSink::sync_chunk`. |
| R2 row count == shadow row count but content diverges | SEV-0 (CHAIN BREAK) | Escalate to `RB-AUDIT-EXPORT-VERIFY-FAILED` immediately. R2 is canonical; the shadow content disagreeing means R2 is also corrupt OR the canonicalization pipeline drifted between writers. |

### 3.4 Mitigations

Same as §2.4 — catch-up replay for shadow-missing, P1 audit incident
for shadow-extra, SEV-0 chain-break for content-divergence. The
reconcile cron re-runs the next UTC day; mitigation is verified by
the next clean run.

## 4. Regulatory note (NOT a personal-data breach)

The Neon analytics shadow is an **operational analytics tier**:

- The R2 archive remains the canonical chain-integrity store with
  Object Lock Governance Mode 7y retention per
  `CTRL-AUDIT-001` + `specs/_audits/sealed/2026-05-15-audit-chain-retention.md`.
- A shadow lag / drift does NOT impair the customer's ability to
  produce a verifiable audit export (the export pipeline reads from
  R2, NOT the shadow).
- Therefore: **NO GDPR Art. 33 (72h notification) / LGPD Art. 48
  clock starts** on a Neon shadow incident. The DPO does NOT need to
  be paged; the SRE on-call handles the SEV-2 routine.

If the diagnosis in §3.3 escalates to a SEV-0 chain break, the
breach-notification clock starts there (per
`RB-AUDIT-EXPORT-VERIFY-FAILED.md` §5) — but only via the SEV-0
path, never directly from the shadow-lag pager.

## 5. First-3-actions cheat sheet (SEV-2 page)

1. **Acknowledge** the page in PagerDuty; set primary on-call status
   to "responding to neon-shadow lag".
2. **Run Diagnose §2.2A** — get the current lag value + region +
   last-synced seq. Paste into the incident channel
   `#corelink-alerts` (Slack mirror).
3. **Decide** from §2.3 table based on lag value + Neon project
   status; execute the matching Mitigation §2.4 path.

## 6. Closure criteria

- Per-region lag p99 below `SHADOW_LAG_NOMINAL_MAX_MS` (5 min) for
  3 consecutive samples (15 min total).
- Daily reconciliation cron next run reports 0 drift.
- A `corelink.audit.neon_shadow_synced.v1` emit with `sev = "info"`
  has been observed in the post-mitigation window for every affected
  `(tenant, region)` pair.
- Postmortem note appended to this runbook §7 (Recent incidents).

## 7. Recent incidents

_(append rows; one-line summary + commit / PR link with the
remediation)_

| Date | Region(s) | Lag observed | Mitigation | Postmortem |
|---|---|---|---|---|
| _none yet_ |  |  |  |  |
