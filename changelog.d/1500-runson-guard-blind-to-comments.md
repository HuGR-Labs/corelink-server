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
  of which resolve to self-hosted/corelink; stripping the value before
  classification recovers **all ~20 at once**, and the guard stayed green — the
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
    `inspected >= 150`, but reverting the fix leaves reach *above* 150, so the
    mutation *passed* that assertion and the one check meant to notice a silent
    shrink could not see the very class this PR closes. The floor is now **188**,
    and the property it has to hold is a relation, not a literal:
    `mutant reach < 188 <= live reach`. Re-probed after the change — the mutant
    is killed and the unmutated suite is 17/17 green, on both trees: 8 of 17
    cases fail under the mutation on this branch, 4 of 17 on `main` with the fix
    applied. The count differs because the two trees hold different workflows;
    what does not differ is that `test_live_repo_reach_is_not_vacuous` is among
    the failures in both, which is the case the floor exists for.

    The absolute reach is a property of the tree you measure, so it is recorded
    with the tree attached rather than stated bare — both readings are correct
    and they differ, which is exactly how a number like this misleads a reader a
    month later. Snapshot 2026-08-31: on this branch (base `af42b328`)
    live **193** / mutant **173**; on `main` with the fix applied — the tree the
    gate sees *after* merge — live **195** / mutant **174**. 188 kills the mutant
    and clears live reach in both. The comments in the suite and in the script
    now carry the relation and the reproduce command instead of a literal that
    goes stale on the next workflow that gains or loses a self-hosted job.
  - **The new docstring enumerated three `runs-on:` spellings and omitted two the
    guard still cannot see**: a block sequence (value on the following lines,
    which `RUNS_ON`'s `(.+?)` does not match at all) and a quoted scalar
    (`runs-on: "corelink"`, whose quotes survive the strip so `== "corelink"` is
    False). Neither occurs in `.github/workflows/` today — verified with a
    positive control, the same greps do match planted instances — so they are
    latent holes, now named as such in the docstring. Closing them means parsing
    the YAML instead of scanning lines, which is a different change; tracked as
    **B-140**.
