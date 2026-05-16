---
id: "ENDURANCE-24H-ANALYSIS"
type: "analysis_template"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["load-test", "endurance", "24h", "analysis", "template", "r-prep", "ga-evidence"]
---

# Endurance 24h — Drift Analysis Report (TEMPLATE)

> **Template for the post-run analysis artifact emitted by the
> 24h endurance drill (`RB-ENDURANCE-24H-DRILL`).** Copy this file to
> `tests/load/results/endurance-24h/YYYY-MM-DD/ANALYSIS.md`, fill in the
> placeholders, and attach to the GA evidence pack
> (`specs/_compliance/GA-GATE-CRITERIA.md` row R-6 staging endurance).
>
> All `<<…>>` markers are placeholders. Cells marked `pass/fail` must
> be replaced with the literal word; do NOT leave the placeholder.

## 1. Run header

| Field                | Value |
|----------------------|-------|
| Test run ID          | `<<endurance-24h-RUN-ID>>` |
| Script               | `tests/load/k6/scenarios/endurance-24h.js` |
| Script SHA           | `<<git rev-parse HEAD>>` |
| Start (UTC)          | `<<YYYY-MM-DDTHH:MM:SSZ>>` |
| End (UTC)            | `<<YYYY-MM-DDTHH:MM:SSZ>>` |
| Elapsed              | `<<24h00m>>` |
| Environment          | staging |
| Target host          | `<<https://staging.corelink.humangr.com>>` |
| Generators           | `<<3 regions: iad / fra / sin>>` |
| Sustained VUs        | 50 |
| Tenant pool          | 50 (Zipfian s=1.07) |
| Traffic mix used     | CAS read 60% · CAS write 15% · audit query 10% · BYOK 8% · admin 5% · webhook 2% |
| Prometheus RW URL    | `<<https://prom-rw.staging.corelink.humangr.com/api/v1/write>>` |
| Grafana dashboard    | `<<https://grafana.staging.corelink.humangr.com/d/endurance-24h>>` |
| On-call shift log    | `<<link to RB-ENDURANCE-24H-DRILL execution journal>>` |

## 2. Hourly latency histograms (p50 / p95 / p99 per operation, per hour)

Source PromQL (per row):

```promql
histogram_quantile(0.99,
  sum by (le, op, hour) (
    rate(endurance_op_latency_ms_bucket{test="endurance-24h"}[5m])
  )
)
```

Drop the metric pivot below (one row per (op, hour); 6 ops × 24 hours = 144
rows). Tooling: `scripts/endurance_analysis_pivot.py` (TODO; for now the
operator runs the PromQL above and pastes the table).

| Hour | op | p50 (ms) | p95 (ms) | p99 (ms) | error rate | sample count |
|------|----|---------:|---------:|---------:|-----------:|-------------:|
| 0  | cas_read     | `<<>>` | `<<>>` | `<<>>` | `<<>>` | `<<>>` |
| 0  | cas_write    | `<<>>` | `<<>>` | `<<>>` | `<<>>` | `<<>>` |
| 0  | audit_query  | `<<>>` | `<<>>` | `<<>>` | `<<>>` | `<<>>` |
| 0  | byok_op      | `<<>>` | `<<>>` | `<<>>` | `<<>>` | `<<>>` |
| 0  | admin_op     | `<<>>` | `<<>>` | `<<>>` | `<<>>` | `<<>>` |
| 0  | webhook      | `<<>>` | `<<>>` | `<<>>` | `<<>>` | `<<>>` |
| 1  | cas_read     | `<<>>` | `<<>>` | `<<>>` | `<<>>` | `<<>>` |
| … repeat for hours 1..23 |

## 3. Drift charts (24h totals)

For each chart, paste the rendered PNG into `assets/` and link below.
Each chart MUST overlay the SLO floor (red line) where applicable.

### 3.1 Latency drift

- **Chart**: `assets/endurance-24h-latency-drift.png`
- **Series** (one line per op): p99 latency per hour-of-test (24 buckets).
- **Floor lines**: SLO catalog §4.6 (CAS GET p99), §4.7 (CAS PUT p99),
  §4.8 (AC hit p99).
- **Acceptance**: linear regression slope on p99 vs hour
  `≤ +2 ms/h` per op; cumulative drift across 24h `≤ +50 ms` (CAS read),
  `≤ +150 ms` (CAS write), `≤ +30 ms` (AC hit), `≤ +50 ms` (BYOK / admin / webhook).

| op | slope (ms/h) | drift 0h→23h (ms) | floor | result |
|----|-------------:|------------------:|-------|--------|
| cas_read    | `<<>>` | `<<>>` | ≤ +50  | `<<pass/fail>>` |
| cas_write   | `<<>>` | `<<>>` | ≤ +150 | `<<pass/fail>>` |
| audit_query | `<<>>` | `<<>>` | ≤ +400 | `<<pass/fail>>` |
| byok_op     | `<<>>` | `<<>>` | ≤ +100 | `<<pass/fail>>` |
| admin_op    | `<<>>` | `<<>>` | ≤ +200 | `<<pass/fail>>` |
| webhook     | `<<>>` | `<<>>` | ≤ +60  | `<<pass/fail>>` |

### 3.2 Error-rate drift

- **Chart**: `assets/endurance-24h-error-rate-drift.png`
- **Series**: 5-min rolling `endurance_error_rate{hour=$h}` per hour.
- **Floor**: 0.1% per hour.
- **Acceptance**: max hourly rate `< 0.1%`; total run aggregate `< 0.05%`.

| metric                       | value | floor | result |
|------------------------------|------:|------:|--------|
| max 5-min rolling error rate | `<<>>` | 0.001 | `<<pass/fail>>` |
| run-aggregate error rate     | `<<>>` | 0.0005 | `<<pass/fail>>` |
| budget-breach strikes (3-in-a-row > 0.1%) | `<<>>` | 0 | `<<pass/fail>>` |

### 3.3 Memory drift

- **Chart**: `assets/endurance-24h-memory-drift.png`
- **Series**: `memory_drift_per_hour` (RSS MiB, polled every 60s).
- **Acceptance**: slope `≤ +5 MiB/h` AND 24h cumulative `≤ +120 MiB`.
  Above this we declare a slow leak suspect; file follow-up issue.

| metric                  | value | floor | result |
|-------------------------|------:|------:|--------|
| slope (MiB/h)           | `<<>>` | ≤ +5 | `<<pass/fail>>` |
| cumulative drift 0→23h  | `<<>>` | ≤ +120 MiB | `<<pass/fail>>` |
| memory poll failures    | `<<>>` | < 5 | `<<pass/fail>>` |

## 4. SLO compliance — hours-in-violation

For every SLO in `specs/03_architecture/slo_catalog.md §4.*`, fill the
table. `hours_in_violation` = count of 1h windows where the SLI fell
below target. Target is the team-tier value per §3.1 (interpolation rule
already applied in slo_catalog §3.1 since Lote 6.7).

| SLO id | SLO | tier (team) | hours_in_violation | budget_remaining | result |
|--------|-----|-------------|--------------------:|-----------------:|--------|
| SLO-AVAIL-CTL  | §4.1 Control-plane availability         | 99.9%   | `<<>>` | `<<>>` | `<<pass/fail>>` |
| SLO-AVAIL-GET  | §4.2 CAS GET availability                | 99.9%   | `<<>>` | `<<>>` | `<<pass/fail>>` |
| SLO-AVAIL-PUT  | §4.3 CAS PUT availability                | 99.9%   | `<<>>` | `<<>>` | `<<pass/fail>>` |
| SLO-AVAIL-AC   | §4.4 AC availability                     | 99.9%   | `<<>>` | `<<>>` | `<<pass/fail>>` |
| SLO-LAT-GET    | §4.6 CAS GET p99                         | 300ms   | `<<>>` | `<<>>` | `<<pass/fail>>` |
| SLO-LAT-PUT    | §4.7 CAS PUT p99                         | 800ms   | `<<>>` | `<<>>` | `<<pass/fail>>` |
| SLO-LAT-AC     | §4.8 AC hit p99                          | 60ms    | `<<>>` | `<<>>` | `<<pass/fail>>` |
| SLO-DEDUP      | §4.8.1 Dedup ratio                       | ≥ 30%   | `<<>>` | `<<>>` | `<<pass/fail>>` |
| SLO-CORR-CAS   | §4.9 CAS integrity                       | 100%    | `<<>>` | `<<>>` | `<<pass/fail>>` |
| SLO-ISOL-TEN   | §4.10 Tenant isolation                   | 100%    | `<<>>` | `<<>>` | `<<pass/fail>>` |
| SLO-FRESH-BIL  | §4.11 Billing freshness (webhook 95%<5s) | 95%     | `<<>>` | `<<>>` | `<<pass/fail>>` |
| SLO-FRESH-DSR  | §4.12 DSR SLA freshness                  | 99%     | `<<>>` | `<<>>` | `<<pass/fail>>` |

> Skip rows for SLOs the endurance scenario does not exercise (e.g. DR
> failover §4.18/4.19 — covered by BCP/DR drills, not this load test).

## 5. Anomaly windows — 3-hour periods with > 2× baseline error rate

Baseline = run-aggregate error rate from §3.2.
Scan the 22 overlapping 3h windows (0-2, 1-3, … 21-23). List every window
where mean error rate ≥ 2 × baseline. Annotate suspected cause.

| Window (h) | Mean error rate | Ratio vs baseline | Suspected cause | Linked alert |
|------------|----------------:|------------------:|-----------------|--------------|
| `<<n/a>>`  | `<<>>` | `<<>>` | `<<none / chaos-X / GC pressure / …>>` | `<<PD incident id>>` |

If empty: write `**No anomaly windows detected.**` below the table.

## 6. Recommendation

Choose ONE:

- [ ] **GO** — All §4 SLOs pass; no anomaly windows; drift within floors;
  endurance evidence ready for GA gate (R-7).
- [ ] **DEFER** — One or more SLOs in §4 failed; root-cause known;
  re-run required after fix. Re-run window: `<<YYYY-MM-DD>>`.
- [ ] **HOLD-FOR-FIX** — Memory leak or drift exceeds floor without
  known cause. Engineering investigation required before re-run.

**Selected**: `<<GO / DEFER / HOLD-FOR-FIX>>`

**Rationale (3-5 sentences):** `<<…>>`

**Follow-up actions (file GH issues, link below):**
- `<<owner: org/repo#NNN — title — due date>>`
- `<<…>>`

## 7. Auditor-ready sign-off

> This section MUST be signed before the report is filed in the GA
> evidence pack. Auditor reviews this block for SOC 2 CC7.5
> (operational readiness) + ISO 27031 §8.4 (BCP rehearsal evidence).

| Role | Name | Signature (initials) | Date (UTC) |
|------|------|----------------------|-----------|
| Run operator (k6 kick-off)         | `<<>>` | `<<>>` | `<<>>` |
| Run shadow (4h shift coverage)     | `<<>>` | `<<>>` | `<<>>` |
| Engineering reviewer (drift sign-off) | `<<>>` | `<<>>` | `<<>>` |
| GA-readiness reviewer (R-6/R-7)    | `<<>>` | `<<>>` | `<<>>` |
| Auditor (external; for evidence-pack snapshots only) | `<<>>` | `<<>>` | `<<>>` |

**Artifact paths (immutable; do not edit after sign-off):**

- `tests/load/results/endurance-24h/<<YYYY-MM-DD>>/results.json`
- `tests/load/results/endurance-24h/<<YYYY-MM-DD>>/endurance-summary.json`
- `tests/load/results/endurance-24h/<<YYYY-MM-DD>>/assets/*.png`
- `specs/_audits/<<YYYY-MM-DD>>-endurance-24h-evidence.md` (this file
  copied & sealed)

---

**Cross-links:**

- Spec: `specs/03_architecture/slo_catalog.md` §4.*
- Runbook: `specs/_runbooks/RB-ENDURANCE-24H-DRILL.md`
- Roadmap row: `ROADMAP-TO-GA.md` §6 — endurance drill
- Companion BCP/DR cadence: `specs/_compliance/BCP-DR-DRILL-CADENCE.md`
- CI 2h variant: `.github/workflows/endurance-2h-nightly.yml`
