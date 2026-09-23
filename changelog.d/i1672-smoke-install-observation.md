### Fixed

- **#1672 smoke install evidence boundary.** The smoke lane runs manually on
  the `corelink` fleet label without live credentials, records Docker backend,
  process, readiness, endpoint, timeout, death, cleanup, and artifact
  provenance observations in a structured receipt, and reports backend and
  service failures independently. Hosted campaign runs verify this contract
  only; they cannot close the fleet-runtime evidence boundary.
