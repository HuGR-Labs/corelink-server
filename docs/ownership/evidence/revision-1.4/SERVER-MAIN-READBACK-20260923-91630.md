# `corelink-server` current-main readback — 2026-09-23 (`91630ba`)

## Pin and method

- Campaign source snapshot: `50a5ab3a0e3e90bff56beb18fade52f50e3ff005`.
- Current fetched `origin/main`: `91630baebe3ae7abe686cd4e06a5621ecdc4ab73`,
  subject `docs(auth): repair knowledge citation evidence (#2225)`.
- Its parent chain contains `38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018`,
  which added the lost-ACK regression, focused CI workflow, and operator recovery
  guidance recorded in
  [`SERVER-MAIN-READBACK-20260923-38F43.md`](SERVER-MAIN-READBACK-20260923-38F43.md).
- Object diff `38f43b6..91630ba` changes seven `docs/knowledge/auth/` pages;
  it does not change `crates/corelink-container`, the focused workflow, or the
  operator contract. Thus the 38f43b6 server changes are present at this pin.
- This is SOURCE evidence from Git objects and static reads. No Rust test,
  build, hosted workflow, remote operation, or runtime observation occurred.

## Current package census and contract

- At `91630ba`, `git ls-tree -r --name-only` counts 401 tracked Rust files in
  `crates/corelink-container/src/` and 14 in `tests/`. The package still has 13
  integration-test targets and one shared Rust test helper; the source count is
  unchanged from the existing bounded census.
- The current package tree includes the prior `main_runtime.rs` extraction and
  DSR classification split, plus the durable staging lost-ACK test. Its
  SQLite-loopback fixture commits the insert, drops the response once, expects
  an uncertain first call, then requires `Deduped` on retry with one durable
  winner.
- The workflow `issue-1635-durable-classification.yml` selects that exact test
  for matching pull-request paths or manual dispatch and declares read-only
  repository contents permission. Its presence is not proof of a hosted run.
- The operator contract treats bodyless 503 as uncertain: retry the unchanged
  record under the same identity and do not settle until typed acknowledgement.
  Conflict and rejected-only responses follow reconciliation handling.

## Limits

The source/test contract supports the server package maintenance map and
REL-019/REL-071. It does not prove test execution, D1 service behavior, consumer
repository execution, deployment, production reachability, or runtime. The
four package artifacts use this commit as their immutable source pin; this
readback does not approve them or replace independent cold review.
