# PR #1785 hosted CI command manifest

This manifest is for the product change tracked by [#1700](https://github.com/HuGR-dev/corelink-server/issues/1700). Run it from the repository root on the PR revision in the central hosted CI bundle.

## Bounded suite

```sh
set -euo pipefail
python3 scripts/verify_staging_target.py
python3 scripts/verify_staging_topology_contract.py --expect ready
python3 -m unittest -q \
  tests/test_verify_staging_target.py \
  tests/test_verify_staging_topology_contract.py
```

The suite covers the PR's fail-closed staging target contract, the existing complete topology contract, and both mutation test modules. It does not provision or deploy staging and does not run either k6 load workflow. Issue #1700 remains open for live staging provisioning and the separately authorized hosted load run.

## Provenance

- Product PR: #1785
- Issue: #1700
- Required commit trailers: preserve `Refs #1700` and `Signed-off-by` when rebasing or integrating.
- Workflow policy: consume the existing central CI bundle; do not modify `.github/workflows/campaign-ci.yml` for this product change.
