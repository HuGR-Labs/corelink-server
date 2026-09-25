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

## Five acceptance axioms

1. A zero exit remains `SUCCESS`, even when test output mentions ENOSPC; a real
   test or compiler failure keeps `TEST_FAILURE` precedence over incidental
   infrastructure text.
2. A failed write or linker operation with a disk-full signature is classified
   as `ENOSPC`, while cancellation and timeout keep their own classifications;
   runner-loss text is `CANCELLED` only for a non-zero result.
3. The actionable infrastructure annotation and structured classification are
   emitted before the original non-zero gate status is returned unchanged.
4. Hosted failure evidence identifies its Actions run and exact commit SHA,
   includes capacity and available-byte measurements before filling and after
   the run-owned fixture is removed, and omits runner names and filesystem paths.
5. Cleanup is bounded to run-owned fixture paths, is verified by readback, and
   retains an EXIT-trap fallback for interrupted jobs.

These axioms establish repository and GitHub-hosted contract behavior. They do
not establish the state of the separate CoreLink self-hosted fleet.
