### Fixed

- **B-098:** `corelink-runbook-tracker` now inherits the workspace lint policy,
  repository population counts in `CLAUDE.md` are checked dynamically, and a
  fail-closed verifier plus owner packet records the missing semver release
  action without fabricating or pushing a tag.
