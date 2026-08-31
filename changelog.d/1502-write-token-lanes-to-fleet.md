### Changed

- **The four write-token CI lanes moved off the owner's Mac onto the ephemeral
  Cloudflare fabric** (`dependabot-auto-merge`, `dependabot-policy`, `pr-labels`,
  `welcome-first-pr` — 6 jobs). All four are `pull_request_target`, so they carry
  a write-scoped token; evaluating a PR merge-ref under that token now happens in
  a disposable microVM instead of on the founder's machine.

  Two things this change is honest about rather than quiet about:

  - **It cannot be exercised before it merges.** GitHub runs the `pull_request_target`
    definition from the BASE ref, so every check on the pull request itself ran
    the `main` version of these files, on `corelink-builder-*`. None of the 6 jobs
    has executed on the fabric, and no amount of pushing to the branch will change
    that. Rollback if the first post-merge Dependabot PR or PR-label run stalls:
    revert this commit — `runs-on:` is the only line each job changes.
  - **`policy-gate` now names its host triple explicitly.** The step that runs
    `scripts/ci-use-host-toolchain.sh` gets `HOST_TRIPLE: x86_64-unknown-linux-gnu`.
    Without it, the script's `main` default (`x86_64-apple-darwin`, from when the
    only self-hosted fleet was Macs) would send it looking for a Darwin toolchain
    on a Linux box and exit 1 — this PR alone would have broken the Dependabot
    supply-chain gate. #1501 teaches the script to detect the triple from `uname`;
    naming it here removes the merge-order coupling instead of depending on it.

### Security

- **Documented, in-file, that `pr-labels` and `welcome-first-pr` have no actor
  gate and are bounded only by repository visibility.** Both are
  `pull_request_target` (and `welcome-first-pr` also `issues`), so after this
  change a triggering event spawns fabric microVMs. Measured 2026-08-31: the repo
  is PRIVATE with 0 forks, so today only members and invited collaborators can
  raise those events. `pull_request_target` is exempt from the "require approval
  for first-time contributors" policy that gates `pull_request`, so the day this
  repo is made public those lanes become an unauthenticated fleet-spawn trigger.
  Also recorded: a refused or dropped spawn leaves a job `queued`, which
  `timeout-minutes` does not bound, and the fabric's `redriveOrphanedJobs`
  reconciler retries only `MAX_ORPHAN_ATTEMPTS = 3` before a 30-minute
  dead-letter TTL — so a stranded `pr-labels` job reads to
  `scripts/pre-merge-gate-check.sh` as a pending check, i.e. ⛔ DO NOT MERGE.
