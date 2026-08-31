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

  Two follow-up repairs from a cold review of this PR:

  - **The anti-vacuity floor was below the defect it guards.** The suite asserted
    `inspected >= 150` against a live reach of 193 — but reverting the fix leaves
    reach at 173, so the mutation *passed* that assertion and the one check meant
    to notice a silent shrink could not see the very class this PR closes. The
    floor is now 188: measured to fail the mutant (173 < 188) while leaving room
    for ordinary job churn. Re-probed after the change — the mutant is killed
    (8 of 17 cases fail), and the unmutated suite is 17/17 green.
  - **The new docstring enumerated three `runs-on:` spellings and omitted two the
    guard still cannot see**: a block sequence (value on the following lines,
    which `RUNS_ON`'s `(.+?)` does not match at all) and a quoted scalar
    (`runs-on: "corelink"`, whose quotes survive the strip so `== "corelink"` is
    False). Neither occurs in `.github/workflows/` today — verified with a
    positive control, the same greps do match planted instances — so they are
    latent holes, now named as such in the docstring. Closing them means parsing
    the YAML instead of scanning lines, which is a different change; tracked as
    **B-140**.
