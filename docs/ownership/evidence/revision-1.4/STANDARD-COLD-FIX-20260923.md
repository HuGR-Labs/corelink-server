# Standard cold-review repair — 2026-09-23

This repair addresses the two required findings in
`COLD-REVIEW-WAVE2-STANDARD-20260923.md`. It changes validation code and
regression tests only; it is not a new cold-review approval or a standard
freeze.

1. `metadata_only_delta` removes permitted skill fields from the byte
   comparison only when they are direct entries of the top-level `metadata`
   mapping. A changed `extra.package` or `extra.nested.package` remains in the
   comparison and invalidates the metadata-only readback. The existing valid
   metadata-only readback fixture still passes.
2. `record_readback` promotes an issue only when the URL is exactly
   `https://github.com/{repository}/issues/{number}`. A readback reporting
   number 7 with `/issues/8` now raises `ValueError`; the registry consequently
   reports `INVALID_READBACK` even if its ledger repeats the contradictory URL.

Regression verification: `test_revision.py` (54 tests),
`test_publication.py` (20 tests), and the registry readback case pass.
`git diff --check` passes. The full documentation test suite was run: 147
tests, one failure in the registry population structural assertion. Its
current-package validation reports unrelated documentation errors in
`corelink-adapter-host`, `corelink-handler-customer`, `corelink-rate-headers`,
and `corelink-reapi`; those package documents are outside this repair's edit
scope. This report does not claim a green full suite while those errors remain.
