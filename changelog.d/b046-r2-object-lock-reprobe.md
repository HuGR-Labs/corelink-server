### Added

- Added a redacted, explicitly opt-in two-operation S3 probe for bucket-level
  Object Lock and per-object `COMPLIANCE` retention.
- Permission, credential, endpoint, and unknown failures remain
  `INDETERMINATE`; only an explicit provider `NotImplemented` is recorded as
  blocked evidence.
- B-046 remains open: this does not claim a Compliance guarantee, add a
  `compliance` database mode, or change the existing Governance legal-hold path.
