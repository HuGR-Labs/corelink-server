### Fixed

- **The WP ledger could read metadata hidden inside Markdown code fences.** The
  verifier now parses backtick and tilde fences statefully, matching delimiter
  character and length before closing a block, and only reads coverage,
  workflow-ownership, WP headings, and contract fields outside outer fences.
  Short, trailing, and mismatched closing delimiters are rejected. Regression
  tests cover nested special fences and fake headings/fields; the focused suite
  passes 21 tests and the live ledger remains 56/56 with the WP-150 map at
  135/135.
