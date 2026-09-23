### Fixed

- **B-088 pentest census test baseline.** Align the regression assertions with
  the canonical closed census: 208 occurrences across 60 published files.
- Admit B-373's historical Dependabot snapshot as an explicitly excluded
  B-101 source, pinned by digest rather than silently ignored.
- Preserve B-028's historical nine-alert source beside its empty post-merge
  refresh, keeping audit coverage and Dependabot verification truthful.
