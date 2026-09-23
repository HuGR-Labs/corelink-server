# Current `main` drift readback — 2026-09-22

The campaign re-read `origin/main` after the pilot review wave. It now points
to `a2a03b2be6d4c59ad9f3d126578bd2e9054cca10` (`fix(cli): pin exact dlltool
build identity (#2127)`), advancing from the pilot source pin
`96fd67793704c597c697d24b0baf96731da102ef`.

The exact delta contains only these paths:

- `.github/workflows/issue-2050-cli-release-dry-run.yml`
- `.github/workflows/okf_wiki.yml`
- `.github/workflows/python-tests.yml`
- `scripts/verify_i2050_release_dryrun_contract.py`
- `tests/test_hosted_python_workflow_contract.py` (added)

No reviewed pilot source, manifest, migration, test-harness or ownership
artifact path changed in this delta. Therefore the five pilot byte/review
records remain valid for their pinned source selections, but the campaign is
not allowed to call `96fd...` the current repository HEAD. The new `main`
SHA is recorded as external drift requiring a final pre-freeze re-read and
integration review; it does not authorize publication or alter any pilot
approval.
