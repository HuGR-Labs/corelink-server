### Fixed

- Closed B-270 through B-281 from the final D03 bundled Rust pass: repaired
  extracted-module paths and scopes, restored the fenced physical-GC API,
  normalized audit D1 rows, corrected timing/SLI types, removed strict-Clippy
  residues, and made runtime invitation entropy a production dependency. A
  fail-closed structural verifier with mutation coverage protects every repair,
  including the repository-wide residual sibling-module census.
