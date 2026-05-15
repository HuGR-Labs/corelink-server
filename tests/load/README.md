# R3-prep — K6 Load Test Suite (Operator Playbook)

> Status: green for staging. **DO NOT** run any of these scripts against the
> production environment from CI or any unattended automation. The
> kill-switch stampede + signup burst scenarios both mutate D1 state and
> can burn out free-tier D1 quotas if cleanup is skipped.

This directory contains five k6 scripts that exercise the critical paths
mapped in R-3 (E2E + integration). They are designed to run dia-1 do staging
deploy — i.e. the first thing the operator runs after the GA staging cutover.

## Scenarios

| # | Script                          | Surface                       | Load profile                          | p99 floor | Other floors                                 |
|---|---------------------------------|-------------------------------|---------------------------------------|-----------|----------------------------------------------|
| 1 | `k6/signup-orchestration.js`    | `POST /v1/signup`             | 0 → 50 RPS ramp 1 min, 50 RPS 5 min   | ≤ 500 ms  | error rate ≤ 0.1%, idempotency holds         |
| 2 | `k6/stripe-webhook-burst.js`    | `POST /v1/billing/stripe-webhook` | 100 VUs replay 10 event_ids for 2 min | ≤ 200 ms  | 100% return 200, zero double-mutation        |
| 3 | `k6/dsr-api.js`                 | `POST /v1/privacy/dsr/{access,erasure,portability}` | 20 RPS sustained 10 min   | ≤ 1 s     | receipt JWT structural, idempotency holds    |
| 4 | `k6/cas-write-read.js`          | `PUT/GET /v1/cas/blobs/:digest` | 200 RPS PUT (1 MiB) + 1000 RPS GET, 5 min | PUT ≤ 800 ms / GET ≤ 150 ms | hit-ratio ≥ 90% post-warm-up    |
| 5 | `k6/byok-revoke-stampede.js`    | `POST /v1/admin/byok/cmk/:id/revoke` + status polling | single revoke, 10k DEK entries  | n/a       | full evict ≤ 60 s, SEV-1 alert fires, SLA counter == 0 |
| 6 | `k6/scenarios/endurance-24h.js` | Realistic mix: 60% CAS read / 15% CAS write / 10% audit / 8% BYOK / 5% admin / 2% webhook | 50 VUs sustained 24h (5m ramp + 23h50m steady + 5m ramp-down). 50 tenants, Zipfian s=1.07. Configurable via `DURATION`: 24h (manual), 2h (CI nightly), 30s (smoke). | drift floors per op (see analysis template) | error rate < 0.1% / 5-min window; 3-strike budget breach tripwire; memory drift ≤ 5 MiB/h |

Scenarios 1–5 are the **acceptance gate** (short-burst SLO assertions) — a
release is not staging-clean unless every script passes its thresholds.
Scenario 6 is the **endurance / drift-detection gate**: 2h variant runs
daily via `.github/workflows/endurance-2h-nightly.yml`; the full 24h
variant is a manual pre-GA drill — see
`specs/_runbooks/RB-ENDURANCE-24H-DRILL.md` and the analysis report
template at `tests/load/k6/scenarios/endurance-24h-ANALYSIS-TEMPLATE.md`.

## Pre-requisites

```bash
# k6 v0.50+ (the JS syntax used here is `arrival-rate` executors).
brew install k6     # macOS
# or: docker pull grafana/k6:0.51.0

# Verify scripts parse before sending to a real target.
for f in tests/load/k6/*.js; do k6 inspect "$f" || exit 1; done
```

## Environment variables

| Var                        | Required by                              | Notes |
|----------------------------|------------------------------------------|-------|
| `K6_TARGET_HOST`           | all                                      | Must point at staging. The signup script aborts on prod-shaped hostname. |
| `K6_AUTH_BEARER`           | signup, dsr, cas, byok                   | Staging PAT scoped to load-test tenant. NEVER export a real-tenant PAT. |
| `K6_STRIPE_WHSEC`          | stripe-webhook-burst                     | Staging Stripe `whsec_…` secret only. |
| `K6_MFA_STUB_TOKEN`        | dsr-api                                  | Staging Clerk stub MFA token (the prod accept-list rejects this header). |
| `K6_BYOK_TEST_CMK_ID`      | byok-revoke-stampede                     | Pre-warmed CMK id created by `scripts/byok-load-warmup.sh` (TODO). |

Store these in the operator's local `.envrc` (NEVER commit them). The CI
workflow reads them from GitHub Environment secrets bound to the `staging`
environment.

## Cadence

- **Pre-GA**: run all five scenarios manually on the staging deploy
  candidate before promoting to production. Required artifact in the GA
  evidence pack: `tests/load/results/{date}/*.json`.
- **Weekly post-GA**: automated via `.github/workflows/load-test-nightly.yml`
  on Sundays 02:00 UTC. Non-blocking — reports trend only. CI **does fail**
  on a p99 regression > 20% versus the prior week's baseline.
- **On-demand**: any time a change merges into `apps/server/**`,
  `crates/corelink-{signup,dsr,cas,tier-selection}/**`, or
  `migrations/d1/**`, an operator should re-run scenarios 1-4 manually.
  The BYOK stampede only re-runs when `corelink-byok-*` changes.

## Output

Each script can be run with JSON output and rendered to HTML for review:

```bash
k6 run --out json=tests/load/results/$(date +%Y-%m-%d)/signup.json \
  tests/load/k6/signup-orchestration.js

# Optional: HTML report via k6-html-reporter.
npx k6-html-reporter@1.x \
  --json tests/load/results/$(date +%Y-%m-%d)/signup.json \
  --output tests/load/results/$(date +%Y-%m-%d)/signup.html
```

The nightly CI uploads the JSON artifacts to the workflow run; the HTML
reporter step runs offline on the operator workstation when needed.

## Cleanup

Each scenario creates synthetic state:

- **signup-orchestration**: creates ~15 000 staging tenants over a 6-min
  run. Cleanup script: `scripts/load-test-cleanup.sh signup` (TODO — for
  R3 we rely on the staging D1 weekly purge cron).
- **stripe-webhook-burst**: writes ≤ 10 rows in
  `stripe_webhook_events_processed` (only the 10 canonical event_ids).
  Cleanup: `DELETE FROM stripe_webhook_events_processed WHERE event_id LIKE 'evt_load_%'`.
- **dsr-api**: enqueues ~12 000 DSR jobs that drain to /dev/null on staging
  (the DSR worker is wired to a no-op exporter on the load-test tenant).
- **cas-write-read**: writes ~60 000 1 MiB blobs (~60 GiB). Run on the
  `load-test-r3` tenant only — its R2 bucket has a 24h lifecycle rule that
  purges all objects.
- **byok-revoke-stampede**: re-warm the CMK + DEK cache via
  `scripts/byok-load-warmup.sh --cmk-id $K6_BYOK_TEST_CMK_ID --entries 10000`.

Failure to clean up will burn staging D1 quota. The nightly workflow runs a
cleanup job after every scenario.

## Target environment safety rails

- `K6_TARGET_HOST` *must* contain `staging.` or `dev.`. The signup script
  refuses production hostnames. Add the same guard to every new scenario.
- The webhook script requires a staging-only Stripe webhook secret
  (`whsec_test_…`). Production secrets MUST NEVER be exposed to this code
  path.
- The DSR script requires the staging MFA stub token — production rejects
  it server-side.
- The BYOK script requires admin-scoped staging PAT and a CMK id that the
  test pre-allocated; admin PATs scoped to real customers MUST NOT be used.

## Related specs

- `specs/03_architecture/slo_catalog.md` (canonical SLOs — every threshold
  here references a line in that catalog).
- `apps/server/src/webhook.rs` (R2-12 Stripe webhook — idempotency contract).
- `crates/corelink-tier-selection/src/ledger.rs` (D1 INSERT-OR-IGNORE
  guarantee that the webhook scenario relies on).
- `crates/corelink-dsr/` (DSR endpoint, MFA gating, receipt JWT).
- `crates/corelink-signup/` (orchestrator + idempotency store).

## Versioning

Bump the suite version (and re-baseline the nightly regression budget) when
the SLO catalog floors change. The current version is **r3-prep v1** —
matches HEAD of `wt/r3-prep-load-tests`.
