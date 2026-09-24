# Standard final-fix readback — 2026-09-23

Observed local `origin/main`: `a18d1146ac6cea79ef36ff56c00682a869060460`.
Campaign HEAD at readback: `3df52eb71acdd4e00084d42f0b64191674bf42eb`.
Prior main readback: [MAIN-DRIFT-READBACK-20260923-E0F.md](MAIN-DRIFT-READBACK-20260923-E0F.md), at `e0f231110524fe81ed0b8d3451903879daf9d519`.

## Current-main delta

`git diff --name-status e0f2311 a18d1146a` shows two changed GitHub workflows and eleven changed `docs/knowledge` crate/flow pages. It shows no changed Cargo manifest, lockfile, or Rust source in that interval. The earlier E0F drift readback and [CURRENT-MAIN-AFFECTED-PACKAGES-20260923.md](CURRENT-MAIN-AFFECTED-PACKAGES-20260923.md) remain the scope of outstanding SOURCE, manifest, and workspace/lock reconciliation. The current pin updates observation only; it does not claim these package artifacts are current-main approved.

## Standard tool repairs

- Publication preflight blocks a marker-matched candidate unless its number is a positive integer and its URL is exactly `https://github.com/<owner>/<repo>/issues/<number>`. A matching marker with an inconsistent URL cannot be offered for reuse.
- Registry review-state derivation checks that the declared source commit is an ancestor of checkout HEAD and that every declared source hash equals `git show <source_commit>:<path>`, after the record consistency gate. A failure gives `INVALID_REVIEW_RECORD`; no missing record is promoted.
- The registry CLI defaults to the repository root. The generated package/manifest population must equal the census; stale calibration blocks output writes. Its JSON and Markdown were regenerated using the observed main pin. No issue was created or updated.

The generated registry still reports cold review `UNVERIFIED` and publication `NOT_PUBLISHED` for all 105 packages. These are evidence states, not approval. Contract freeze and issue publication remain dependent on separate cold review and current-main reconciliation.

## Verification

- `python3 -m unittest discover -s docs/ownership/tests -q`: 149 tests passed.
- `python3 docs/ownership/tests/adversarial_probe.py`: 13/13 synthetic expectations met.
- Registry generation with explicit `--root` and with the default root both returned population 105, publication count 0, calibration `PASS`. Their JSON and Markdown outputs were byte-identical by `cmp`; the generated registry had no structural failures.
- `git diff --check`: passed.
