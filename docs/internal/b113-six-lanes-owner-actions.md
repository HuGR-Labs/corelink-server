# B-113 seven-workflow residual owner actions

This packet records only actions that cannot be proven or executed from the
repository. It is deliberately separate from `scripts/verify_b113_lane_population.py`,
which proves the exact seven-workflow, nine-job population and bounded
contracts locally. `terraform-drift.yml` is explicitly excluded because its
control is owned by B-111.

The B-113 population is exactly these seven workflow files:

`nightly.yml`, `sbom.yml`, `buck2-starter-ci.yml`, `fuzz-nightly.yml`,
`endurance-2h-nightly.yml`, `load-test-nightly.yml`, and
`billing-health-daily.yml`. Buck2 contributes three independently observed
jobs (`build`, `negative-scenarios`, and `benchmark`), so the seven workflows
contain nine job boundaries. `terraform-drift.yml` is not a B-113 lane.

## Platform / CI actions

1. **B-113/nightly hosted mutants receipt**

   `.github/workflows/issue-1863-mutants-hosted.yml` is the only dispatch-only GitHub-hosted scheduled-equivalent evidence path for `mutants-workspace`.
   The legacy `nightly.yml` job remains disabled; do not dispatch it or treat a
   skipped legacy job as a receipt. Retain the final protected-main hosted run
   URL, SHA, runner, lane, and conclusion with secrets and personal data
   redacted. The retained aggregate must bind the green baseline and exact,
   duplicate-free union of all 27 deterministic shard inventories. A repository
   contract or an in-progress run does not prove that any dispatched run
   succeeded.

2. **Fuzz platform choice (`fuzz-nightly.yml`)

   Decide whether to provision the self-hosted macOS fleet with the required
   libFuzzer C++ sources or move this sanitizer matrix to a Linux runner. Run
   `30735420326` showed `FuzzerPlatform.h` and `libfuzzer/Fuzzer*.cpp` missing;
   this is the concrete platform decision. After the choice, dispatch once,
   retain the result, and only then consider restoring the schedule.

3. **Buck2 end-to-end observation (`buck2-starter-ci.yml`)

   Dispatch `build` with the existing `CORELINK_CANARY_PAT` binding. The old
   runner/PATH diagnosis is closed by code; run `31978160612` is historical
   evidence only. Do not restore PR/push/schedule triggers until a complete
   cold build, warm-hit assertion, and smoke run are green.

## Staging / load-test action

`endurance-2h-nightly.yml` and `load-test-nightly.yml` need either a staging
deployment with the documented `K6_TARGET_HOST`, PAT, Stripe, MFA, and BYOK
secrets, or an owner decision to retire the staging-targeted lanes. Runs
`26872039942` and `26705023347` predate the current `corelink` labels and do not
prove that the present workflows can execute. Do not run a real load test as
part of B-113 hygiene.

## Billing action (tracked by B-065)

Run `billing-health-daily` after retiring the redundant Stripe webhook endpoint.
Run `33494388796` is a valid failure: it observed three event types under both
the canonical and derived id schemes. The owner action is to disable the
redundant Stripe destination and then observe three consecutive healthy checks;
changing the repository checker to suppress this finding would be an invalid
false-green repair.
