### Fixed

- **Public PAT mint aliases could drift into separate rate-limit budgets (B-160).**
  `POST /v1/pats` and `POST /v1/customer/keys` now share the same per-tenant
  `pat-issue` bucket across Clerk and native-PAT callers, preserving the exact
  ten-per-hour policy and its 360-second retry boundary.
