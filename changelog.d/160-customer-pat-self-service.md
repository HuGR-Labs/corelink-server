### Added

- **B-160 — customer-authorized `POST /v1/pats`.** A validated Clerk session
  or canonical PAT now reaches the existing tenant-scoped self-service mint;
  the Worker derives the tenant, strips client trust headers, and never
  forwards `CORELINK_PAT_MINT_AUTH_KEY`. The plaintext canonical PAT is returned
  once only, while write-scope escalation, cross-tenant use, expired/revoked
  credentials, and missing authentication fail closed.
