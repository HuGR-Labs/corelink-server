---
id: "RB-ENDURANCE-24H-DRILL"
type: "runbook"
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
tags: ["runbook", "load-test", "endurance", "24h", "drill", "ga-evidence", "r-prep", "r-6"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2, §10.

# RB-ENDURANCE-24H-DRILL — Manual 24-Hour Endurance Drill

> **Parent WI:** R-6 staging endurance (see `ROADMAP-TO-GA.md` §6).
> **Companion CI:** `.github/workflows/endurance-2h-nightly.yml` (2h variant
> runs daily; this runbook covers the *manual* 24h exercise).
> **Companion script:** `tests/load/k6/scenarios/endurance-24h.js`.
> **Companion report template:** `tests/load/k6/scenarios/endurance-24h-ANALYSIS-TEMPLATE.md`.
> **Companion cadence file:** `specs/_compliance/BCP-DR-DRILL-CADENCE.md`
> (this drill is interleaved with the BCP/DR cadence but is *not* itself a
> BCP/DR drill — it is a sustained-traffic load exercise, classified
> under **R-6 staging endurance** in the roadmap).

## 1. Scope and intent

A 24-hour continuous sustained-traffic exercise against staging that
surfaces slow-drift failures the 5-minute scenarios cannot detect:

1. Memory leaks in Worker isolates / Container DEK cache.
2. p99 latency creep as background GC pressure builds.
3. Audit-chain verifier lag accumulating against ingest.
4. Log volume escalation crossing the 500 GiB/day budget.
5. SLO error-budget burn that only crosses threshold over multi-hour
   windows (the 1h × 14.4 multi-burn alert is invisible to 5-min scripts).

Outcomes feed the GA evidence pack (see
`specs/_compliance/GA-GATE-CRITERIA.md` row R-6 staging endurance) and
the Production Readiness Review (`specs/_templates/production_readiness_review.md`
§4 "load + endurance evidence").

## 2. Cadence

| Phase                 | Cadence                          |
|-----------------------|----------------------------------|
| Pre-GA (R-6 window)   | **Monthly** while pre-GA          |
| Post-GA (sustaining)  | **Bi-annually** (semestral)       |
| Trigger ad-hoc        | After any change to CAS / audit / BYOK hot-path crates that touches per-iter allocation; after Workers runtime version bump; before any tier upgrade |

The CI 2h variant (`endurance-2h-nightly`) runs daily and provides
between-drill drift continuity.

## 3. Prerequisites

### 3.1 Environment

- Staging environment fully deployed and healthy for ≥ 24h prior:
  - Worker version matches latest `main` head.
  - Container service running at ≥ 3 replicas / region.
  - D1 staging schema at the same migration head as production-bound
    branch.
  - R2 staging buckets healthy; 24h lifecycle rule active on
    `load-test-r3` and `load-test-endurance` tenants.
- Prometheus remote-write endpoint reachable from the k6 generators:
  `https://prom-rw.staging.corelink.dev/api/v1/write` (verify with
  `curl -I` before kick-off).
- Grafana dashboard `dashboards/grafana/DASH-ENDURANCE-24H.json` loaded
  (provisioned via `infra/grafana/`); the operator confirms all panels
  render.

### 3.2 k6 generator cluster

Three generators in three regions to spread network paths and prove
geo-locality of latency drift:

| Generator | Region (provider POP) | Role                          | k6 entry-point |
|-----------|-----------------------|-------------------------------|----------------|
| `gen-iad` | us-east-1             | Primary (kicks off scenario)  | `tests/load/k6/scenarios/endurance-24h.js` |
| `gen-fra` | eu-central-1          | Secondary (mirrors traffic)   | same |
| `gen-sin` | ap-southeast-1        | Tertiary (mirrors traffic)    | same |

Each generator runs the SAME script with `K6_TENANT_BUCKET={iad|fra|sin}`
(env-controlled tenant subset; subsets disjoint so total traffic
aggregates without double-counting tenants).

### 3.3 Access + credentials

- Staging PAT scoped to load-test tenants (NEVER a real-tenant PAT).
- Prometheus remote-write bearer token (read-write scoped to
  `endurance_*` metric prefix only).
- Grafana viewer credentials for the on-call shadows.
- PagerDuty access for the active-region primary (the synthetic-page
  drill stays paused during the 24h window — see §4.2).

### 3.4 Pre-run checklist

Print this checklist; the run operator initials each row before
kick-off.

- [ ] Staging environment green for ≥ 24h (Grafana `corelink-overview`
      dashboard all panels green; no active SEV-1/SEV-2).
- [ ] D1 staging tenant cleanup completed (`scripts/load-test-cleanup.sh
      endurance` — TODO; for now: confirm `t_endurance_*` rows < 100
      with `wrangler d1 execute --command "SELECT COUNT(*) FROM tenants
      WHERE tenant_id LIKE 't_endurance_%'"`).
- [ ] SLO error budgets at fresh state (no pending burn-down debt;
      confirm in Grafana `slo-budget` panel for each SLO in scope).
- [ ] On-call coverage confirmed for all 24h (3 × 4h shifts × 2
      regions = 6 shadow seats filled; primary on-call same-region as
      run operator).
- [ ] Synthetic page drill (RB-SYNTHETIC-PAGE-DRILL) paused for the
      24h window — paging during this exercise would pollute MTTA
      evidence.
- [ ] BCP/DR drills (`specs/_compliance/BCP-DR-DRILL-CADENCE.md`) NOT
      scheduled to fire during this window (drill calendar in Slack
      `#corelink-oncall`).
- [ ] `K6_ENDURANCE_CONFIRM=yes` will be set in the kick-off env
      (the script refuses to run for DURATION>30s without it).
- [ ] Stripe staging webhook secret valid (try one event with
      `tests/load/k6/stripe-webhook-burst.js` smoke).
- [ ] Three k6 generators reachable + k6 version ≥ 0.50 on each.
- [ ] Analysis template copied to
      `tests/load/results/endurance-24h/YYYY-MM-DD/ANALYSIS.md` with
      header pre-filled.

## 4. Execution

### 4.1 Kick-off command

Run from `gen-iad` (the primary generator). The other two generators
run the same command in parallel (each in its own tmux session with
the region-specific tenant bucket env).

```bash
set -euo pipefail

export K6_TARGET_HOST="https://staging.corelink.dev"
export K6_AUTH_BEARER="$(op read op://corelink-staging/k6-pat/credential)"
export K6_PROMETHEUS_RW_SERVER_URL="https://prom-rw.staging.corelink.dev/api/v1/write"
export K6_ENDURANCE_CONFIRM="yes"
export DURATION="24h"
export VUS="50"
# Region-specific tenant bucket (one of: iad / fra / sin):
export K6_TENANT_BUCKET="iad"

DATE_TAG=$(date -u +%Y-%m-%d)
mkdir -p "tests/load/results/endurance-24h/${DATE_TAG}/${K6_TENANT_BUCKET}"

k6 run \
  --out "experimental-prometheus-rw" \
  --out "json=tests/load/results/endurance-24h/${DATE_TAG}/${K6_TENANT_BUCKET}/results.json" \
  --summary-export "tests/load/results/endurance-24h/${DATE_TAG}/${K6_TENANT_BUCKET}/summary.json" \
  tests/load/k6/scenarios/endurance-24h.js \
  2>&1 | tee "tests/load/results/endurance-24h/${DATE_TAG}/${K6_TENANT_BUCKET}/console.log"
```

Each generator logs to its own console.log. The wrapper script is
`scripts/endurance-24h-launch.sh` (TODO — for now operator runs the
above by hand on each generator).

### 4.2 Monitoring duties (4-hour shifts)

| Shift (UTC)    | Role     | Primary | Shadow |
|----------------|----------|---------|--------|
| T+0..T+4       | hand-off | run operator | sec on-call |
| T+4..T+8       | hand-off | `<<>>` | `<<>>` |
| T+8..T+12      | hand-off | `<<>>` | `<<>>` |
| T+12..T+16     | hand-off | `<<>>` | `<<>>` |
| T+16..T+20     | hand-off | `<<>>` | `<<>>` |
| T+20..T+24     | hand-off | run operator | sec on-call |

Each shift logs:

- p99 latency snapshot per op (Grafana endurance dashboard).
- Error rate snapshot (must be < 0.1%).
- Memory drift (RSS growth since shift start; `< 5 MiB`/h target).
- Any alerts that fired (file under `console.log/anomalies.md`).

Hand-off uses the on-call handover template in
`specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` §4 Hand-off Checklist.

### 4.3 Abort criteria

Abort immediately (kill all three k6 generators with Ctrl-C and
file a SEV-2 incident) when ANY of:

- Real customer impact detected — staging shares L4 ingress with
  production for the LB warm pool. If production p99 climbs >50ms,
  abort.
- Error rate ≥ 5% sustained for 5 minutes.
- Memory drift > 50 MiB/h on any worker instance.
- Audit-chain verifier lag > 30 minutes (lag dashboard panel red).
- D1 staging quota > 80% (the run will exhaust quota before 24h).
- Any SEV-1 incident anywhere in the platform.

Abort = kill `k6 run` with Ctrl-C; the script's graceful ramp-down
will *attempt* to settle but on hard abort we leave it. Then:

1. Log the abort reason in
   `tests/load/results/endurance-24h/YYYY-MM-DD/ABORTED.md`.
2. Open SEV-2 incident in PagerDuty (routing key
   `endurance-drill-abort`).
3. Reschedule the drill per §2 (treat as failed run; not GA-eligible
   evidence).

## 5. Post-run

### 5.1 Pull analysis

Run from `gen-iad` after all three generators finish (T+24h):

```bash
set -euo pipefail
DATE_TAG=$(date -u +%Y-%m-%d -d "yesterday")
python3 scripts/endurance_analysis_pivot.py \
  --date "${DATE_TAG}" \
  --output "tests/load/results/endurance-24h/${DATE_TAG}/ANALYSIS.md"
```

> **Note**: `scripts/endurance_analysis_pivot.py` is a TODO for the
> next iteration. Until it lands, fill the
> `endurance-24h-ANALYSIS-TEMPLATE.md` by hand from PromQL queries
> documented in the template's §2 + §3.

### 5.2 Generate report

Use `endurance-24h-ANALYSIS-TEMPLATE.md` as the skeleton. The report
MUST be signed (§7 sign-off block) by:

- Run operator (kick-off + close-out)
- Run shadow (any 4h shift)
- Engineering reviewer (drift sign-off — usually CAS owner)
- GA-readiness reviewer (R-6/R-7 chair)

### 5.3 File action items

Every `DEFER` or `HOLD-FOR-FIX` recommendation MUST file at least one
GH issue. Tag with `endurance-24h` + `wave-r6` + the affected crate.

### 5.4 Archive

Move artifacts to immutable evidence path:

- `tests/load/results/endurance-24h/YYYY-MM-DD/` (raw artifacts)
- `specs/_audits/YYYY-MM-DD-endurance-24h-evidence.md` (signed
  ANALYSIS.md copy, doc_status: ACTIVE; this is the GA evidence-pack
  reference)

## 6. References

- `tests/load/k6/scenarios/endurance-24h.js` (the script)
- `tests/load/k6/scenarios/endurance-24h-ANALYSIS-TEMPLATE.md` (report template)
- `.github/workflows/endurance-2h-nightly.yml` (CI 2h variant)
- `specs/03_architecture/slo_catalog.md` §4.* (SLO definitions)
- `specs/_compliance/BCP-DR-DRILL-CADENCE.md` (drill calendar coordination)
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` (shift hand-off + escalation)
- `specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md` (pause coordination during 24h window)
- `ROADMAP-TO-GA.md` §6 (R-6 staging endurance row)
- `specs/_compliance/GA-GATE-CRITERIA.md` (R-6 evidence row)

## 7. Change log

| Date       | Change                                                  |
|------------|---------------------------------------------------------|
| 2026-05-15 | Initial drop — companion to endurance-24h k6 scenario. |
