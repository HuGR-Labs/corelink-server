# Structural readback — 2026-09-22 (R3)

Campaign content HEAD: `2f81d873f` (evidence pin commit `c832859b7`).
Observed `origin/main`: `4b9a8469ec3e713b6df3b7562dfd70373121590b`.

The current checker rejects legacy schema 1.0 unless explicit historical-fixture
mode is supplied, requires anchors/navigation for extra canonical H2 sections,
and the publication/renderer paths require an explicit canonical repository.
The generated registry is deliberately non-approving.

```text
artifacts=420
implemented_checks_pass=420
fail=0
missing=0
population_registry=105
registry_structural_pass=68
registry_integrity_fail=37
publication_count=0
documentary_tests=137
adversarial_probe=13/13
git_diff_check=PASS
```

This certifies only implemented structural checks, registry generation and local
Python tests. It does not certify semantic completeness, current-main freshness,
cold review, Cargo/runtime reachability, deduplication, repository identity,
issue publication or merge status.
