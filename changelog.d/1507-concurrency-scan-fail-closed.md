### Fixed

- **The concurrency scanner now requires proven, unconditional cross-ref
  isolation (B-172).** It fails closed not only for workflow-name, matrix, SHA,
  quoted/plain-text lookalikes, ambiguous duplicate keys, and malformed YAML,
  but also for expressions that merely mention a discriminator behind a false
  guard or comparison. Only direct audited discriminators and exact documented
  fallback forms pass; actionlint-clean positive, negative, and mutation
  fixtures protect that boundary before the report's current-main evidence is
  reused.
