### Added

- ### B-044 orphan teardown contract

Added the external B-044 work-package contract and fail-closed static guard for
instance-to-handle proof, idempotent namespaced teardown, and mutation coverage.
This does not arm or claim runtime teardown; the live join and owner enablement
remain pending in `corelink-runners`.
