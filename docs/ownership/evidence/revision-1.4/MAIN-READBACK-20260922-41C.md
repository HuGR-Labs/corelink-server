# `main` readback — 2026-09-22 (`41c89d72`)

Commanded readback:

```text
git fetch origin main
git diff --stat 0d1e85792bbe1b495bc8273ee63011417510140f..FETCH_HEAD
git diff --name-only 0d1e85792bbe1b495bc8273ee63011417510140f..FETCH_HEAD
```

Observed remote tip: `41c89d72a76ea3e4774034c7200a2cdab2f518aa`.

The delta is 60 files (`+1444/-117`). It contains no Cargo manifest or lockfile
change, but it does touch package/runtime surfaces and migrations, including:

- `crates/corelink-container/src/webhook_inbox_d1.rs`;
- `crates/corelink-container/src/routes/billing_ingest/tests_durable_classification.rs`;
- `crates/corelink-container/src/routes/dsr/adapter_d1_tests.rs`;
- `crates/corelink-tier-selection/src/runner_checkout_attempt.rs`;
- D1 migrations `0131` through `0142` (renames and contract changes);
- worker TypeScript stages;
- new/changed CI, Buck2, Terraform-drift and verification scripts.

Therefore the absence of manifest drift does not preserve all pilot evidence:
`corelink-server` and billing/D1-related relations require a new SOURCE
readback at this pin. CI/script additions remain SOURCE/UNKNOWN unless executed;
they do not prove runtime reachability or deployment. The ownership branch's
105 package paths are still not present on remote `main`.

This readback is evidence of source drift only. It does not approve, freeze or
authorize publication.
