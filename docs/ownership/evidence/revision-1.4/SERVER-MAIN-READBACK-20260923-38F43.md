# `corelink-server` source readback — 2026-09-23 (`38f43b6`)

## Pin and scope

- Campaign snapshot remains `50a5ab3a0e3e90bff56beb18fade52f50e3ff005`.
- This readback pins the current fetched `origin/main` tip at
  `38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018`, subject
  `test(billing): cover lost ingest acknowledgement (#2222)`.
- The preceding `cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6` is its ancestor and
  does not contain the lost-ACK test, focused workflow, or operator-contract
  delta. The earlier `MAIN-DRIFT-READBACK-20260923-CD74.md` was corrected to
  remove that mistaken attribution.
- Readback used immutable Git objects and static inspection only. No Rust test,
  build, hosted workflow, production operation, or GitHub write was executed.

## Verified package/source facts

- The `crates/corelink-container` tree includes the earlier runtime-helper
  extraction into `src/main_runtime.rs` and DSR classification split into
  `src/routes/dsr/adapter_d1/classification.rs` relative to the campaign
  snapshot.
- `git ls-tree -r --name-only 38f43b6 crates/corelink-container/src` counts
  401 tracked `.rs` files. The corresponding `tests/` path count is 14 Rust
  files, with 13 integration targets and one shared helper as recorded in the
  package manifest/readback. The source count remains unchanged.
- The current test file
  `crates/corelink-container/src/routes/billing_ingest/tests_durable_classification.rs`
  adds a loopback fixture that commits its SQLite INSERT, drops one response,
  then asserts the first store call is uncertain, the identical retry returns
  `StageOutcome::Deduped`, and exactly one durable winner remains.
- The production `D1UsageStagingStore::stage` contract is the same-coordinate
  insert/no-op and durable-winner fingerprint classification. The test gives
  source-level regression coverage for that behavior; it is not evidence that
  the implementation or a remote D1 service ran in this campaign.
- `.github/workflows/issue-1635-durable-classification.yml` selects the exact
  test when matching paths change on a pull request or on manual dispatch. It
  specifies Rust 1.91.1, read-only contents permission, and the selected test
  command. The workflow definition does not prove a hosted run occurred.
- `docs/operator/issue-1635-consumer-contract.md` says a bodyless 503 leaves
  the batch uncertain: retry the unchanged record with the same identity and
  do not settle until a typed acknowledgement reports accepted/deduped. It
  separately routes conflicts and rejected-only batches to reconciliation.

## Ownership impact and limits

The verified delta adds durable-classification validation and recovery guidance
to the server's maintenance/test map. REL-019 remains the server-to-billing-emit
ingest relationship; REL-071 records the focused workflow-to-test edge. No
claim is made about private consumer repository execution, production
reachability, D1 behavior outside the isolated source fixture, or runtime.

The 401-file census is preserved, and the server package artifacts now pin this
source revision for the changed test/workflow/operator-contract claims. Their
four review verdicts remain independent state; this source readback is not an
approval.
