# R3-prep — K6 Load Test Suite (Operator Playbook)

> Status: **dispatch-only until staging is provisioned**; operator-only and requires a real staging target plus an owner-issued identity receipt. **DO NOT** run any of these scripts against the
> production environment from CI or any unattended automation. The
> kill-switch stampede + signup burst scenarios both mutate D1 state and
> can burn out free-tier D1 quotas if cleanup is skipped. There is **no
> arbitrary URL. The dispatch workflow requires an owner-issued target receipt,
> bounds each job, and calls the staging teardown endpoint in an `always()` step.
> A missing teardown token or failed teardown fails the run closed.

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
| 6 | `k6/scenarios/endurance-24h.js` | Realistic mix: 60% CAS read / 15% CAS write / 10% audit / 8% BYOK / 5% admin / 2% webhook | 50 VUs sustained 24h (5m ramp + 23h50m steady + 5m ramp-down). 50 tenants, Zipfian s=1.07. Configurable via `DURATION`: 24h (manual), 2h (operator dispatch), 30s (smoke). | drift floors per op (see analysis template) | error rate < 0.1% / 5-min window; 3-strike budget breach tripwire; memory drift ≤ 5 MiB/h |

Scenarios 1–5 are the **acceptance suite** (short-burst SLO assertions) for an
operator-provisioned staging run. Scenario 6 is the **endurance / drift-
detection harness**: both the 2h and full 24h variants are on-demand; there is
no automated performance gate in the retired staging workflow. See
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
| `K6_TARGET_IDENTITY_RECEIPT` | workflow pre-flight                    | Owner-issued JSON binding canonical staging to a 40-character deployment SHA; expires within 24h. |
| `K6_STAGING_TEARDOWN_TOKEN` | workflow teardown                       | Staging-only token for the bounded synthetic-state teardown endpoint. |
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
- **Post-GA**: on-demand only via `.github/workflows/load-test-nightly.yml`.
  The workflow is intentionally not scheduled; it requires a real staging
  deployment and dedicated secrets. Until then it is dispatch-only. When dispatched, the baseline comparison
  fails on a median regression > 20% versus the stored baseline.
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

The dispatch workflow uploads the JSON artifacts and target receipt to the workflow run; the HTML
reporter step runs offline on the operator workstation when needed.

## Cleanup

Each scenario creates synthetic state:

- **signup-orchestration**: creates ~15 000 staging tenants over a 6-min
  run. No cleanup script exists today. Do not run this scenario until the
  owner has approved a bounded staging-only cleanup procedure; the operator
  is responsible for removing only the synthetic load-test rows and recording
  the resulting count.
- **stripe-webhook-burst**: writes ≤ 10 rows in
  `stripe_webhook_events_processed` (only the 10 canonical event_ids).
  No automatic cleanup exists. An owner-approved operator may remove only the
  synthetic `evt_load_%` rows after verifying the target is the staging D1.
- **dsr-api**: enqueues ~12 000 DSR jobs that drain to /dev/null on staging
  (the DSR worker is wired to a no-op exporter on the load-test tenant).
- **cas-write-read**: writes ~60 000 1 MiB blobs (~60 GiB). Run on the
  `load-test-r3` tenant only — its R2 bucket has a 24h lifecycle rule that
  purges all objects.
- **byok-revoke-stampede**: re-warm the CMK + DEK cache via
  `scripts/byok-load-warmup.sh --cmk-id $K6_BYOK_TEST_CMK_ID --entries 10000`.

Before dispatch, the owner/operator must confirm the staging tenant, synthetic
identifiers, R2 lifecycle behavior, and the bounded cleanup steps above. The
workflow teardown receives only its run id and scenario and is derived from the
canonical target origin; it cannot be redirected to another URL. A failed or
missing teardown is a failed run and must be investigated before another run.

## Target environment safety rails

- `K6_TARGET_HOST` *must* equal `https://staging.corelink.humangr.com` (one
  trailing slash is the only normalization). Every workflow run also validates
  `K6_TARGET_IDENTITY_RECEIPT` before any scenario starts. The signup script
  refuses production hostnames as a second defense.
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

Bump the suite version (and re-baseline the operator comparison budget) when
the SLO catalog floors change. The current version is **r3-prep v1** —
matches HEAD of `wt/r3-prep-load-tests`.

## Hosted CI dispatch command manifest

The bounded lane is dispatched manually from the repository root with the exact
command below. The confirmation literal is required; omit secrets from the
command because GitHub resolves them from the protected `staging` environment.

```bash
gh workflow run load-test-nightly.yml --repo HuGR-dev/corelink-server --ref main \
  --field confirm=run-bounded-load \
  --field scenarios=signup,webhook,dsr,cas,byok
```

The operator must inspect the workflow run URL and uploaded target receipt after
GitHub accepts the dispatch. A missing receipt, invalid target, expired receipt,
missing teardown token, or teardown failure must leave the run failed closed.
