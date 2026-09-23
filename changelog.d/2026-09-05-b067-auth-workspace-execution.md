### Changed

- # B-067: execute workspace and authentication tests

  The nightly workspace lane now executes the test suite under a bounded
  process-group deadline and four-job compiler ceiling. The dedicated
  `corelink-auth`/`corelink-pat` pull-request lane remains explicit, credential
  free, and SHA-pinned; its semantic verifier rejects compile-only and commented
  command mutations. The first-contributor message now describes the actual
  targeted PR gates instead of claiming that a workspace test runs on every PR.
