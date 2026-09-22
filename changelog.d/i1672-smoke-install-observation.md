### Fixed

- **#1672 smoke install evidence boundary.** The smoke lane now runs manually on
  a GitHub-hosted runner without live credentials, records Docker backend,
  process, readiness, endpoint, timeout, death, cleanup, and artifact
  provenance observations in a structured receipt, and reports backend and
  service failures independently.
