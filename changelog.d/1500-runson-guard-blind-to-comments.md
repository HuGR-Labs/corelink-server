### Fixed

- **The shared-`$HOME` rustup guard silently shrank its own reach — a comment on a
  `runs-on:` line removed that job from inspection.**
  `scripts/validate_no_shared_rustup_mutation.py` selected jobs with
  `runs_on.strip() == "corelink"`, but the repo's own convention is to justify a
  runner inline (`runs-on: corelink  # zero-hosted: python3 baked into the image`).
  With the comment present the equality is false and the job left the inspected
  set — no error, no warning, `OK` still printed. The guard never failed; it just
  covered less, and the only anti-vacuity check it carried
  (`self_hosted_jobs == 0`) could never fire because the set shrinks one job at a
  time. Measured on `main`: 24 live `runs-on:` lines carry a trailing comment, 20
  of which resolve to self-hosted/corelink; inspected count went **173 → 193**
  once the value is stripped before classification, and stayed green — the 20
  recovered jobs were hiding nothing, so what was lost was reach, not compliance.
  Found by teeth-testing the guard while migrating CI off the owner's Mac
  (WP-CI): injecting `uses: dtolnay/rust-toolchain@stable` into a freshly
  migrated job produced no complaint. `${{ }}` expressions are left intact and
  GitHub-hosted runners are still not classified as self-hosted, so the change is
  a strengthening rather than a widening. A 17-case regression suite
  (`tests/validate_no_shared_rustup_mutation_test.py`) pins all of it and was
  itself mutation-probed: reverting the fix fails 3 of its cases, including the
  teeth test — which had to be tightened first, because asserting only
  `rc != 0` passed against the broken code via the "inspected ZERO jobs" bail-out.
