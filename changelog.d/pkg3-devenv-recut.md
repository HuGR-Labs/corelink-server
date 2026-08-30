### Added

- **DevEnv customer surface, re-cut from the rejected #1397 (PKG-3).** The
  feature was rejected wholesale for a re-cut; this brings back only the 10
  rows the frozen manifest marks `SALVAR` without a `LOTE SEPARADO` marker —
  the customer page and nav entry, the DevEnv client/types, the worker ingress
  (`devenv_v1` + openapi route kinds, dual Clerk/PAT auth,
  `stripClientTrustHeaders` before forwarding to `RUNNER_DEVENV_DO`), the guard
  and the OpenAPI document, the `RUNNER_DEVENV_DO` binding, and the monthly
  vCPU meter table.

  Two conditions the manifest makes blocking, both satisfied here rather than
  deferred. The migration is renumbered **0094 → 0106**: `0094` on `main` is
  `runner_usage_counter`, and a duplicate ordinal fails
  `check_migrations_additive.py`. And `devenv_monthly_vcpu` is registered in
  `TENANT_ID_TABLES` and `ALL_TENANT_KEYED_TABLES` **in this same PR** —
  shipping the table without that trades the loud 500 #1405 caused for silent
  UNDER-erasure, which is the worse half of the pair.

  Deliberately NOT taken: the six `LOTE SEPARADO` basePath/pricing rows (a
  customer-facing redirect must not be reviewed inside a PR titled "devenv"),
  the six prod crons, the `AUDIT_BUCKET` binding, the plaintext Ed25519 seed as
  a non-secret prod var, an unused `CORELINK_API_BASE`, and an unrelated
  `undefined`→`null` hunk in the replication-coordinator body.
