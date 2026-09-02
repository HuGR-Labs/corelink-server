### Fixed

- **The concurrency scanner no longer treats arbitrary interpolation as cross-ref
  isolation (B-172).** It now fails closed for workflow-name, matrix, and SHA
  groups, quoted or plain-text lookalikes of `github.ref`, ambiguous duplicate
  keys, and malformed YAML. It validates trigger-dependent PR-number grouping
  and carries actionlint-clean adversarial fixtures before the report's
  current-main evidence is reused.
