# Release-standard repair readback — 2026-09-23

Readback at 19:57 UTC. This report records local candidate bytes and read-only remote checks. It does not approve the standard, publish an issue, or write to GitHub.

## Current main and publication identity

- `git rev-parse origin/main` and `git ls-remote origin refs/heads/main` both returned `7f966dda8234f54b58766f33e5fca2ad02cc898b` after `git fetch origin main`.
- From the prior standard pin `a18d1146ac6cea79ef36ff56c00682a869060460` to this pin, `git diff --name-only` lists 25 workflow, campaign-document, Python script, and Python test paths. It lists no Cargo manifest, Rust source, or ownership path. Older package SOURCE and manifest reconciliation remains separate.
- All 105 publication-ledger items use canonical repository `HuGR-dev/corelink-server` and stable repository ID `1232040291`. All 105 remain `BLOCKED`; their frozen contract URLs remain unset. Historical draft body hashes and manifest markers were not changed.

## Candidate gates and generated readback

- The publication preflight requires the exact `docs/ownership/STANDARD.md` URL path, an existing Git commit, matching committed and candidate standard bytes, and a commit reachable from a fetched `origin/main` that agrees with the current remote main. An invented SHA, wrong path, changed candidate, or local unpublished commit is blocked in tests. The current candidate is not yet a published frozen contract.
- The registry generator checks the supplied `--observed-main` against both fetched `origin/main` and live `refs/heads/main` before building and before writing outputs. A stale pin or stale fetch blocks both output files.
- The generated registry and index pin `7f966dda8234f54b58766f33e5fca2ad02cc898b`. They contain 105 packages, zero structural failures, calibration `PASS`, 105 `UNVERIFIED` cold reviews, 105 `NOT_PUBLISHED` publication states, and publication count zero.
- Candidate SHA-256: `STANDARD.md` `cdd52d6a6ade272f126be37da3be83df905df09e2c17a1ec5648592c315759a6`; `registry.json` `9f318b5f4146b626c0a397ec198ba18233a9a27cbb47dde15b71f4ba040d20a4`; `index.md` `cb70190903946e0e21cc0b8b76e8dc9b55ed4d2662bcdd44b32596e408ef8263`.
- `changed-framework-files.json` now matches the current bytes of all 12 listed framework files, including the seven stale paths identified by the release-standard cold review and the changed tools README. Earlier hashes remain in `previous_sha256` or historical reports.

## Verification

- `python3 -m unittest discover -s docs/ownership/tests -p 'test_*.py'`: 154 tests, OK.
- `python3 docs/ownership/tests/adversarial_probe.py`: 13/13 expected outcomes, zero false acceptances.
- `python3 docs/ownership/tools/generate_registry.py --root . --observed-main 7f966dda8234f54b58766f33e5fca2ad02cc898b --json-output docs/ownership/registry.json --markdown-output docs/ownership/index.md`: exit 0, population 105, publication 0, calibration `PASS`.
- `git diff --check`: exit 0. The framework inventory hashes were compared to current file hashes with no mismatch.

No issue API mutation, publication, merge, Cargo build, or runtime test was performed by this repair.
