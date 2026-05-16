---
id: "RB-24H-ENDURANCE-LOAD"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Engineering Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "load-test", "endurance", "24h", "wave-22", "r-prep", "ga-evidence", "customer-facing"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2, §10.

# RB-24H-ENDURANCE-LOAD — Wave-22 24h Endurance Load Campaign

> **Parent WI:** Wave-22 customer-facing endurance harness setup.
> **Companion runbook:** `specs/_runbooks/RB-ENDURANCE-24H-DRILL.md` (the
> general realistic-mix 24h drill — this runbook concentrates on the
> wave-22 *customer-facing* surface). Both runbooks share the
> `RB-GA-CUTOVER` §4 G1–G6 greenlight criteria.
> **Companion script:** `tests/load/k6/scenarios/endurance-24h-w22.js`.
> **Runner:** `scripts/run_24h_endurance.sh`.
> **Analysis:** `scripts/analyze_endurance_run.py`.
> **Audit:** `specs/_audits/2026-05-16-24h-endurance-harness.md`.

## 1. Scope and intent

A 24-hour continuous load campaign that exercises the wave-22 customer-
facing routes the pilot tenants will hit hardest during the GA cutover
window:

- `POST /v1/audit/export`
- `GET  /v1/audit/analytics/event-count`
- `GET  /v1/audit/analytics/timeline`
- `POST /v1/cas/upload`
- `POST /v1/dsr/erasure`
- `POST /v1/clerk/auth`

The campaign generates the evidence row required by
`specs/_compliance/GA-GATE-CRITERIA.md` R-6 (staging endurance) for the
wave-22 surface. It is *complementary* to (not a replacement for)
`RB-ENDURANCE-24H-DRILL.md`, which exercises the broader realistic-mix
traffic profile.

## 2. Cadence

| Variant   | When                                  | Duration | Profile               |
|-----------|---------------------------------------|----------|-----------------------|
| smoke     | Every PR that touches harness files   | 30s      | 50 RPS local 127.0.0.1 |
| dressrun  | Wave-25+ manual pre-cutover wiring    | 10min    | 100 RPS local mock    |
| nightly   | CI nightly at 02:00 UTC               | 2h       | 1000 RPS staging      |
| full      | Manual pre-GA drill (T-3 days)        | 24h      | 1000 RPS staging      |

Smoke is wired through `scripts/run_24h_endurance.sh smoke` and runs even
if the local server is not up — the runner degrades to
`smoke=red[unreachable]` so the harness wiring is still verifiable.

Dressrun is wired through `scripts/run_24h_endurance.sh dressrun` and
exercises the 10-min profile (2-min ramp-up to 100 RPS, 6-min sustain,
2-min ramp-down) against a local in-memory mock or the `apps/server`
binary in InMemory wiring. It is **not** a substitute for the 24h drill;
its sole purpose is end-to-end wiring validation (k6 → summary →
analyzer → verdict) before the full pre-GA drill is scheduled. See
§10 dress-run history and `specs/_audits/2026-05-16-endurance-10min-dressrun.md`.

## 3. Drift floors (per route p99)

| Route                                       | p99 floor | Notes |
|---------------------------------------------|-----------|-------|
| `POST /v1/audit/export`                     | 1500 ms   | export is async; the 200/202 split is taken from the route contract. |
| `GET  /v1/audit/analytics/event-count`      |  400 ms   | hot path — cache hit dominated. |
| `GET  /v1/audit/analytics/timeline`         |  600 ms   | bucketed aggregation; SLO §4 implies < 1s. |
| `POST /v1/cas/upload`                       |  800 ms   | 1 MiB payloads typical; cap at 1 MiB in harness. |
| `POST /v1/dsr/erasure`                      | 1000 ms   | tombstoning is the hot subpath. |
| `POST /v1/clerk/auth`                       |  300 ms   | session-token strategy only; longer strategies excluded. |

Global error rate must remain below 0.1% across each rolling 5-min
window. Three consecutive 5-min windows above the budget trip the
endurance budget breach and abort the run (parallels the 3-strike rule
in `endurance-24h.js`).

## 4. How to run

### 4.1 Smoke (local, 30s)

```bash
scripts/run_24h_endurance.sh smoke
```

The runner targets `http://127.0.0.1:8787` by default with a stub PAT.
If `corelink-server` is running locally the smoke run hits the real
routes; otherwise it writes `smoke=red[unreachable]` and exits 0. Either
way the harness wiring (k6 script, fixtures, analyser) is exercised.

### 4.1b Dressrun (local, 10min)

```bash
# Option A: against the in-memory Python mock target
python3 scripts/_dressrun_mock_target.py --port 8787 &
MOCK_PID=$!
scripts/run_24h_endurance.sh dressrun
kill "$MOCK_PID"

# Option B: against apps/server in InMemory wiring (preferred)
cargo run --release -p corelink-server &
SRV_PID=$!
scripts/run_24h_endurance.sh dressrun
kill "$SRV_PID"
```

The dressrun mode exports `DURATION=10min` and `K6_PROFILE=10min`. It
defaults to `http://127.0.0.1:8787` and tolerates unreachable target
(same semantics as smoke). Use it before scheduling a full 24h drill to
prove the harness + analyzer pipeline is intact on a clean dev box.

### 4.2 Nightly (CI, 2h)

Wired into the existing endurance nightly workflow (no separate workflow
file added — see commit history). Required env: `K6_TARGET_HOST`,
`K6_AUTH_BEARER`.

### 4.3 Full 24h drill (manual)

```bash
K6_TARGET_HOST=https://staging.corelink.humangr.com \
K6_AUTH_BEARER=$STAGING_PAT \
K6_ALLOW_ADVERSARIAL=yes \
  scripts/run_24h_endurance.sh full
```

The runner refuses if `K6_TARGET_HOST` does not contain `staging.`. The
operator must page themselves via PagerDuty BEFORE invoking
(`pd-cli incident trigger --service corelink-staging --title "24h endurance drill start"`).

## 5. How to interpret

The analyser at `scripts/analyze_endurance_run.py` ingests:

- `run-meta.json` — operator-populated for the manual gates (G2–G6).
- `k6-summary.json` — emitted automatically by k6.
- `k6-stdout.log` — free-form transcript.

…and writes `analysis.md` with verdict `GREENLIGHT` / `MANUAL-REVIEW` /
`BLOCK`. The verdict maps to `RB-GA-CUTOVER` §4 G1–G6:

| Gate | Wave-22 source                                                |
|------|----------------------------------------------------------------|
| G1   | k6 `route_latency` per route vs. the floors in §3.              |
| G2   | Operator copies the audit-chain integrity violation delta into `run-meta.json` from the staging dashboard. |
| G3   | Operator copies SEV-0/1 incident count from PagerDuty.          |
| G4   | Operator copies the pilot-tenant ack count from `specs/_audits/`.|
| G5   | Operator copies Neon shadow lag p99 from the dashboard snapshot.|
| G6   | Operator copies DSR cron 24h success pct from the dashboard.    |

If ANY gate is `UNKNOWN`, the analyser exits 0 with verdict
`MANUAL-REVIEW`; if ANY gate is `RED`, the analyser exits 10 (CI fails
closed).

## 6. What to do on regression

| Symptom                                       | Action                                                                                            |
|-----------------------------------------------|---------------------------------------------------------------------------------------------------|
| G1 RED on a single route                      | Follow `RB-PERF-REGRESSION.md`. Pair the analyser report with the route's perf bench artefact.    |
| G1 RED across multiple routes                 | Suspect a shared dependency (D1, KV, AC bucket). Engage on-call DBA; do NOT cutover.              |
| G2 RED                                        | Follow `RB-AUDIT-EXPORT-INTEGRITY.md`. Audit-chain violations are SEV-1.                          |
| G3 RED                                        | Cutover BLOCKED. The SEV-0/1 incident must be closed AND the 72h cool-down restarted.             |
| G4 RED                                        | Block on Customer Success — they must obtain at least 5 pilot tenant acks (see `RB-GA-CUTOVER`).  |
| G5 RED (Neon lag > 5min)                      | Follow `RB-NEON-SHADOW-LAG.md`. Cutover BLOCKED until lag p99 ≤ 300s for ≥ 30 min.                 |
| G6 RED (DSR cron failed in last 24h)          | Follow `RB-DSR-STATUSPAGE-PUBLISH-FAILED.md`. Cutover BLOCKED.                                    |
| Memory drift > 5 MiB/h sustained for ≥ 2h     | Open SEV-2; capture heap profile from `/v1/admin/diagnostics/memory` and attach to the incident.  |
| Error budget breach (3-strike)                | k6 aborts automatically; capture the last 30 min of stdout + the route_errors counter breakdown.  |

## 7. Adversarial fixtures

The fixture file `tests/load/fixtures/customer-routes.ndjson` includes
cross-tenant, rate-limit-burst, and mid-stream-tamper rows. They fire
ONLY when `K6_ALLOW_ADVERSARIAL=yes` is set (default: off). The runner
defaults this OFF for smoke runs so the harness does not generate 403/429
noise on a dev box.

Adversarial expectations:

- `cross_tenant` requests must return `403` (or `404` on opaque-id routes).
- `rate_limit_burst` requests must return at least one `429` within a 50-req burst.
- `mid_stream_tamper` requests must return `400` or `422` (must NOT 5xx).

If an adversarial fixture returns the *wrong* status, that is a security
regression and is treated as **G2 RED** (audit-chain integrity proxy)
even if the gate metric itself is green.

## 8. References

- `tests/load/k6/scenarios/endurance-24h-w22.js`
- `tests/load/fixtures/customer-routes.ndjson`
- `scripts/run_24h_endurance.sh`
- `scripts/analyze_endurance_run.py`
- `scripts/_dressrun_mock_target.py`
- `specs/_audits/2026-05-16-24h-endurance-harness.md`
- `specs/_audits/2026-05-16-endurance-10min-dressrun.md`
- `specs/_runbooks/RB-ENDURANCE-24H-DRILL.md`
- `specs/_runbooks/RB-GA-CUTOVER.md`
- `specs/_runbooks/RB-PERF-REGRESSION.md`
- `specs/_runbooks/RB-AUDIT-EXPORT-INTEGRITY.md`
- `specs/_runbooks/RB-NEON-SHADOW-LAG.md`
- `specs/_runbooks/RB-DSR-STATUSPAGE-PUBLISH-FAILED.md`
- `specs/_compliance/GA-GATE-CRITERIA.md` (R-6 row)

## 9. Change log

| Version | Date       | Author              | Change                                           |
|---------|------------|---------------------|--------------------------------------------------|
| 1.0.0   | 2026-05-16 | Gustavo Schneiter   | Initial draft (wave-22).                         |
| 1.1.0   | 2026-05-16 | Gustavo Schneiter   | Wave-25 dress-run: 10min profile + dressrun mode + analyzer p99/threshold hardenings (`AUDIT-W25-ENDURANCE-10MIN-DRESSRUN`). |

## 10. Dress-run history

Each entry represents a `dressrun` invocation that validated the harness
+ analyzer plumbing before scheduling a full 24h drill.

| Date       | Wave | Operator           | Target                  | Profile | Verdict (real run) | GREENLIGHT path | Regression-flip (5x) | Notes                                                              |
|------------|------|--------------------|-------------------------|---------|--------------------|-----------------|----------------------|--------------------------------------------------------------------|
| 2026-05-16 | w25  | gustavoschneiter   | 127.0.0.1:8787 (mock)   | 10min   | BLOCK              | PASS            | PASS                 | 45 109 iterations / mock saturated cas/upload → real RED on G1 stdout-tail. Audit: `specs/_audits/2026-05-16-endurance-10min-dressrun.md`. Analyzer fixes: p99 numeric fallback + stdout-breach override. |
