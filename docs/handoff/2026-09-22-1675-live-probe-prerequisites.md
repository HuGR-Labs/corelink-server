# Issue #1675 live probe prerequisites

## Current disposition

The live acceptance gate is **blocked**. The merged classifier and contract
lane are green, but there is no authorized non-production target on which to
run the two operational arms.

The canonical staging contract currently records:

- origin `https://staging.corelink.humangr.com`;
- `deployment_state: unprovisioned` with base resources only;
- public DNS `NXDOMAIN` in the provider readback;
- production resources untouched and no worker or GitHub secret values retained.

A read-only GitHub API check on 2026-09-22 found only `K6_TARGET_HOST` in the
`staging` environment. The five additional probe inputs are absent:
`K6_TARGET_IDENTITY_RECEIPT`, `K6_STAGING_PAT`, `K6_STAGING_MFA_STUB`,
`K6_STAGING_STRIPE_WHSEC`, and `K6_STAGING_BYOK_CMK_ID` (the latter four are
the scoped target credentials; `K6_TARGET_IDENTITY_RECEIPT` is the redacted
identity binding). No dispatch was made and no live receipt is claimed.

The existing `i1675` campaign run remains contract-only. Its artifact contains
zero `corelink.actions-timeout-receipt.v1` instances and cannot satisfy this
gate.

## Provider prerequisites

The SRE owner must complete these prerequisites through the existing staging
provisioning path before dispatching a probe:

1. Apply the reviewed staging topology in `infra/staging/topology.json` and
   preserve the existing staging-only resource names. Deploy only the staging
   workers and route `staging.corelink.humangr.com/*`; do not alter production
   routes, workers, databases, buckets, queues, or secrets.
2. Prove the target with the existing read-only checks:

   ```sh
   python3 scripts/verify_staging_target.py
   python3 scripts/verify_staging_topology_contract.py --expect ready
   dig +short staging.corelink.humangr.com
   ```

   The checks must show a reachable staging origin and the exact topology
   receipt. An environment secret name by itself is not target evidence.
3. Populate the protected `staging` environment with the exact secret names
   required by the topology contract. Values stay in GitHub's secret store,
   never in workflow inputs, argv, logs, or committed evidence. The target
   identity receipt must bind the staging origin to its deployment SHA and
   disposable probe tenant; it must not contain a production identifier.
4. Provide an owner-approved, disposable probe surface with two explicit
   server-side controls:

   - a timeout arm that holds one request past the declared short command
     limit and emits a signed request/response marker;
   - a communication-loss arm that records a real connection loss and emits a
     signed transport marker.

   Both controls must be staging-only, authenticated by the staging probe
   credential, bounded to one request, and cleaned up automatically. A client
   `exit 124`, elapsed time, or a missing response is not sufficient evidence.
5. For runner communication loss, SRE must approve the disposable runner
   method and its observation sink before use. The sink must retain heartbeat
   checkpoints and the final run/job identity while the runner is offline. A
   local simulation, a fabricated `communication_lost` field, or a target
   HTTP error must not be promoted to a runner-loss receipt.

## Exact manual lane

After prerequisites are complete, add the dedicated manual workflow
`i1675-live-probe.yml` (separate from `campaign-ci.yml`) with this fixed
contract:

- `workflow_dispatch` only; no schedule, pull request trigger, or rerun loop;
- `confirm=run-1675-live-probe` and `target=staging` are required inputs;
- `permissions: contents: read, actions: read` only;
- `environment: staging`, `runs-on: ubuntu-24.04` or the owner-approved
  disposable runner label, and a ten-minute job cap;
- checkout uses the repository's pinned `actions/checkout` SHA with
  `persist-credentials: false`;
- preflight requires the exact origin, target identity receipt, all required
  staging secret names, and a fresh DNS/health response;
- the timeout and communication-loss arms run once each, with a short declared
  limit and no production API or mutation;
- every arm writes one observation using
  `corelink.actions-job-observation.v1`, invokes
  `scripts/classify_timeout_receipt.py`, and retains one
  `corelink.actions-timeout-receipt.v1` receipt;
- the receipt includes the exact `GITHUB_SHA`, run ID, numeric job ID, UTC
  timestamps, declared limit, post-step evidence, and heartbeat/transport
  evidence. Secrets and raw hosted logs are excluded; retain only a redacted
  log digest and allowlisted markers;
- an unconditional cleanup step revokes the disposable probe tenant/state and
  verifies cleanup. Cleanup failure keeps the run red;
- the observer records the run URL, job URL, artifact ID, artifact SHA-256, and
  receipt classifications. It must fail closed if either arm is missing,
  contradictory, or only inferred from elapsed time.

The operator dispatches only after the provider prerequisites are evidenced:

```sh
gh workflow run i1675-live-probe.yml \
  --repo HuGR-dev/corelink-server \
  --ref main \
  --field confirm=run-1675-live-probe \
  --field target=staging
```

The resulting artifact is the first evidence eligible for the issue gate. It
must contain one `declared_timeout` receipt and one
`runner_communication_loss` receipt with the same exact SHA/run correlation.
Until that artifact exists, issue #1675 remains open and the contract-only
manifest must remain the stated evidence boundary.

## Evidence boundary

This handoff records provider absence and the executable prerequisites. It does
not assert a timeout, runner communication loss, cause, remediation, or issue
closure. No GitHub Actions run was dispatched by this change.
