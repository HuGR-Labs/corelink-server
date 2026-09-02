### Fixed

- **B-159 sccache documentation now matches the served WebDAV contract.** The
  four locale copies of the integration guide list GET, PUT, HEAD, PROPFIND,
  MKCOL, and authenticated write DELETE, while keeping `.sccache_check` scoped
  to the startup probe/cleanup use. The docs warn that a failed write latches
  sccache read-only for the daemon lifetime and point operators to
  `sccache --show-stats` and `Cache errors` so a green build is not mistaken for
  a healthy write path; Vitest and route tests pin the six-method contract.
