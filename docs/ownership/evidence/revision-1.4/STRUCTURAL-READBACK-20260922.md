# Structural readback — 2026-09-22

This readback covers campaign branch `23dcbba10d67fe7cda6ce2eb2f1ba3f0d85ee146`.
It is a documentary gate only; it does not approve semantics, cold review,
runtime reachability, or publication.

## Scope

- 105 package directories were enumerated under `docs/ownership/crates/`.
- Four artifacts were checked per package: ownership skill, reference, blast
  radius, and maintenance manual.
- Declared profile was read from each package blast-radius frontmatter and used
  for all four package checks.

## Command and result

The fixed checker was imported from
`docs/ownership/tools/check_docs.py` and run with the repository root, real
paths, and the declared `S`/`H` profile. The aggregate result was:

```text
artifacts=420
implemented_checks_pass=420
fail=0
missing=0
```

`python3 -m unittest discover -s docs/ownership/tests -p 'test_*.py'` also
passed 127 tests. `git diff --check` passed after removing mechanical trailing
blank-line noise.

## Limits and invalidation

The checker's `IMPLEMENTED_CHECKS_PASS` verdict is not an approval. It does not
certify semantic completeness, external links, runtime claims, profile
eligibility, independent cold review, cross-package identity reconciliation, or
current-main freshness. Any byte change invalidates the corresponding artifact
readback and any prior review for that byte.
