### Added

- Added a redacted, explicitly opt-in two-operation S3 probe for bucket-level
  Object Lock and per-object `COMPLIANCE` retention.
- Permission, credential, endpoint, and unknown failures remain
  `INDETERMINATE`; only an explicit provider `NotImplemented` is recorded as
  blocked evidence.
- Re-probed the configured production-account endpoint on 2026-09-08 with
  local credentials: R2 rejected the credential shape before bucket creation,
  so the result is `INDETERMINATE` and no probe resource was created.
- B-046 remains open: this does not claim a Compliance guarantee, add a
  `compliance` database mode, or change the existing Governance legal-hold path.
