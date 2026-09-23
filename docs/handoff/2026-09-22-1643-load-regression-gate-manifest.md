# #1643 hosted load regression gate contract

This manifest is the exact review contract for the change tracked by
[#1643](https://github.com/HuGR-dev/corelink-server/issues/1643). It is safe to
run from the repository root after integration because it exercises the
existing credential-free hosted campaign contract; it does not dispatch k6,
provision staging, deploy a Worker, or write a GitHub secret.

## Hosted command

```sh
set -euo pipefail
gh workflow run campaign-ci.yml \
  --repo HuGR-dev/corelink-server \
  --ref main \
  --field suite=i1688
```

The hosted suite must report the `i1688 bounded load lane contract` step as
green. The command is intentionally consumed from the existing campaign
workflow; this change does not edit `.github/workflows/campaign-ci.yml`.

## Exact gate contract

| Contract item | Required value |
| --- | --- |
| Sanitized summary | `schema: 1`, `suite_version: r3-prep-v2` |
| Gated metric | `http_req_duration.med` in milliseconds |
| Regression threshold | `1.20` multiplier, stored in the baseline and matched exactly at comparison time |
| Baseline | `schema: 2`, `baseline_version: k6-baseline-v2`, previous run only |
| Cache namespace | `load-test-k6-baseline-v2` |
| Target | `https://staging.corelink.humangr.com` |
| Identity | one dedicated load-test `tenant_id` and one 40-character deployment SHA across every scenario and baseline |
| Population | exactly `signup,webhook,dsr,cas,byok`, with one successful status and one sanitized summary per scenario |
| Failure behavior | missing, duplicate, malformed, boolean, non-finite, mismatched, or regressing input exits non-zero and cannot publish a baseline |

The sanitizer keeps only the allow-listed identity and duration metrics. Boolean
JSON values are rejected before numeric coercion, so `true` cannot become a
synthetic `1.0ms` measurement. The comparator rejects a raw k6 summary, a
missing or NaN metric, a target or tenant identity mismatch, a stale deployment
identity, a baseline version mismatch, and any partial population. A passing
comparison is the only path that writes the next baseline.

## Closure boundary

The repository gate is deterministic and fail-capable, but #1643 remains open
while [#1700](https://github.com/HuGR-dev/corelink-server/issues/1700) is open.
The canonical staging endpoint, scoped secrets, and a real complete baseline
still require owner provisioning and a bounded hosted load dispatch. This PR
records no live-load evidence and does not claim that closure condition.

## Provenance

- Issue: #1643
- Dependency: #1700
- Required commit trailers: `Refs #1643` and `Signed-off-by: Gustavo Schneiter <gustavomalleths@gmail.com>`
- Workflow policy: use the existing hosted campaign contract; do not add a shared campaign lane for this change.
