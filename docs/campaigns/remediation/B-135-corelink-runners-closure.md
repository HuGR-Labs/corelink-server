# B-135 — corelink-runners closure evidence

This is the redacted cross-repository receipt for the B-135 implementation. The
receipt is intentionally commit- and tree-bound: a PR number without the tested
head and delivered `main` merge is not evidence.

```text
repository: HuGR-Labs/corelink-runners
tested_ref: main
tested_head: d8124b1ab89cf6afb08682442e94c4f4d18c6ba8
tested_tree: d9cadd3741c77bb58d7162922f1510ded41c844f
merged_pr: 561
merge_commit: ec9b6d69dbb1f3bd64ef4f9a4ce7e9d9100e69c1
merge_tree: d9cadd3741c77bb58d7162922f1510ded41c844f
merge_parent: 1119143dc0edd6dea9274c00c2a96024474553d7
sensitive_values: redacted; no credential material recorded
```

The tested and delivered tree carries these load-bearing behaviors:

- `.github/workflows/image-build-impact.yml` has a `pull_request` trigger for
  runner-image inputs and invokes the secretless
  `scripts/ci/runner-image-build-validation.sh` build-only lane.
- The validation workflow is read-only, uses non-persistent checkout credentials,
  and its structural verifier rejects registry publication, deploy commands, and
  token interpolation.
- `deploy/runner/Dockerfile` no longer carries the unconsumed
  `corelink.rust.*`, `corelink.gh.version`, or `corelink.node.version` labels;
  the nightly and cargo-fuzz values remain single-sourced into the runtime
  image instead.

This receipt proves B-135's repository behavior only. It does not claim that a
runner image was published, pinned, rolled, or deployed; those are the open
runtime obligations tracked by B-114 and B-138.
