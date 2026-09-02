### Security

- **Team invitations now expire after 14 days.** `acceptTeamInvitation` enforces a
  half-open `invited_at_ms > now - TTL AND invited_at_ms <= now` predicate in D1, so a
  stale or future-dated invitation cannot be redeemed merely because its e-mail hash
  still matches. The acceptance update uses the same injected clock value and returns
  success only when exactly one row changes, preserving deterministic auditability,
  race safety, and preventing boundary drift.

### Fixed

- Added focused signup-worker coverage for fresh, in-window, stale, future-dated,
  exact-boundary, one-millisecond-inside, recycled-address, default-clock,
  no-write-on-refusal, injected-clock binding, and zero-row concurrent-update
  behavior. The fake D1 follows both SQL comparison operators, so mutations that
  remove either bound, widen `>` to `>=`, use `Date.now()`, or return success after
  `changes=0` fail.

### Remaining

- B-073 remains open: invitation-token binding, tenant scoping, Clerk e-mail
  verification, and an acceptance audit event are separate controls and are not
  claimed by this change.
