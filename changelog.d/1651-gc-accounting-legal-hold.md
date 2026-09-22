### Fixed

- **B-071/#1651 GC finalization now preserves accounting and legal holds.** The
  additive D1 control migration refuses a purge when the tenant is under an
  active legal hold or has no `tenant_storage_state` row, and decrements
  `bytes_used` while increasing `bytes_reclaimed_lifetime` in the same
  transaction as the fenced metadata finalization. The native observation
  adapter also rejects held tenants and refuses candidate populations above
  the deterministic 250-row ceiling. The hosted verifier remains credentialless
  and destructive mode stays unavailable by default.
