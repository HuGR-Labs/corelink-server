### Security

- **Team invitations now expire after 14 days.** `acceptTeamInvitation` enforces a
  half-open `invited_at_ms > now - TTL` predicate in D1, so a stale invitation cannot
  be redeemed merely because its e-mail hash still matches. The acceptance update uses
  the same injected clock value, preserving deterministic auditability and preventing
  boundary drift.

### Fixed

- Added focused signup-worker coverage for fresh, in-window, stale, exact-boundary,
  one-millisecond-inside, recycled-address, default-clock, and no-write-on-refusal
  behavior. The fake D1 follows the SQL comparison operator, so mutations that remove
  the predicate or widen `>` to `>=` fail at the boundary test.

### Remaining

- B-073 remains open: invitation-token binding, tenant scoping, Clerk e-mail
  verification, and an acceptance audit event are separate controls and are not
  claimed by this change.
