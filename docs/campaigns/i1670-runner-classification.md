# Campaign suite `i1670`

This is the fixed remote contract for issue #1670. The campaign workflow should
select `i1670` and run exactly:

```sh
set -euo pipefail
mkdir -p artifacts/campaign-ci/i1670
CORELINK_ARTIFACT_DIR=artifacts/campaign-ci/i1670 \
CORELINK_CLEANUP_ROOT="$GITHUB_WORKSPACE" \
  bash tests/test_classify_runner_failure.sh \
  2>&1 | tee artifacts/campaign-ci/i1670/contract.log
```

The lane uploads `artifacts/campaign-ci/i1670/`, including the wrapper's
`classification.json` when `CORELINK_CLASSIFICATION_ARTIFACT` points there.
It must use a Linux runner, preserve `pipefail`, and keep the workflow timeout
and cancellation settings. The lane is intentionally synthetic: it verifies
the detector and cleanup contract without claiming live runner ENOSPC evidence.

The test contract covers `SUCCESS`, `TEST_FAILURE`, `ENOSPC`, mixed diagnostics,
original exit preservation, summary and JSON artifact emission, bounded timeout,
and deletion limited to the target/cache/temp allowlist.
