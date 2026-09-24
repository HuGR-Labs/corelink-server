# Structural readback — 2026-09-22 (R2)

Campaign branch: `8d0f43345`.

The candidate checker now also rejects rendered H2 headings without canonical
section IDs, while historical schema `1.0` fixtures remain isolated from current
`1.1` artifacts. Publication and renderer helpers now support strict canonical
repository URL identity; these are gate changes, not publication evidence.

```text
artifacts=420
implemented_checks_pass=420
fail=0
missing=0
documentary_tests=132
git_diff_check=PASS
```

This result certifies only implemented structural checks and local Python tests.
It does not certify semantic completeness, current-main freshness, cold review,
Cargo/runtime reachability, deduplication, or issue publication.
