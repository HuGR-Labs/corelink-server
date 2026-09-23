# Billing correction readback — 2026-09-22

## Identity and scope

- Expected repository baseline for this correction: `11898804e5c8cfb518e3f48fe84e096917879152`.
- Package source pin recorded by the billing artifacts: `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.
- The package skill is present in the baseline and was updated; no code or shared registry files were changed.
- No Cargo tests or runtime procedures were executed for this correction.

## API-012 / INV-003 source readback

At the package source pin, `InMemoryAbuseScorer::score_and_apply` emits the score and decision
audits, calls `RateLimiter::update_plan` for the suspicious decision, then emits
`DowngradeApplied`. A failure of either earlier audit returns before `update_plan`. A failure of
`DowngradeApplied` returns an error after the limiter update. The code contains no compensating
limiter update or rollback at that point; the limiter effect is therefore partial and must not be
described as aborted or atomic. The score-window commit and metrics happen later and are not reached
on that error path.

The source test `audit_failure_aborts_decision_and_no_downgrade_applied` uses
`FailingAbuseAuditSink`, which fails every emit. It therefore fails on the first audit and checks
only the pre-update path. The test does not establish behavior when the later `DowngradeApplied`
audit fails. That case remains required validation and was not executed here.

## Test and procedure state

The source pin above identifies the code reviewed; it is not an execution record. Billing
acceptance procedures PROC-002 through PROC-006 and their package test suites remain
`BLOCKED_NOT_EXECUTED` / `NOT_RUN`; no command output, pass count, or test result is claimed.
Required evidence for a future run includes the resolved source pin, exact command, target and
toolchain, exit status, full output, and— for the post-update audit case—limiter state before and
after the failing audit.

## Files corrected

- `.claude/skills/own-corelink-billing/SKILL.md`
- `docs/ownership/crates/corelink-billing/REFERENCE.md`
- `docs/ownership/crates/corelink-billing/MAINTENANCE.md`
