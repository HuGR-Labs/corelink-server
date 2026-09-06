### Changed

- **B-134 makes the Docker-shim experiment observable without inventing a runtime pass.**
  `smoke-install` and `cosign-sign` now route through the `corelink` runner with explicit
  backend preflights and fail-closed checks; the installer image no longer contains a
  committed token, and the deploy verifier receives a validated zone ID from a secret.
  A static contract gate plus mutation tests protect the route, signature verification,
  placeholder rejection, and `UNMEASURED` ledger state. No hosted run was dispatched in
  this change, so promotion to PASS still requires run IDs, logs, and artifact/signature/
  webhook evidence from a real execution.
