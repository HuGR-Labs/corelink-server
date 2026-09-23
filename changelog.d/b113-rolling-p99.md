### Changed

- # B-113 CI lane population and rolling p99

- The dispatch-only endurance lane now downloads exactly one non-expired result
  artifact from each of seven prior successful run IDs, validates the complete
  six-operation population, and writes deterministic `rolling-7d-p99.json`
  before enforcing the +20% p99 threshold. Missing history, failed downloads,
  malformed summaries, duplicate runs, and partial/non-finite metrics fail
  closed.
- The workflow wording now reflects reality: fuzz remains parked on the
  self-hosted macOS fleet, SBOM runs on release publication or explicit
  dispatch, and endurance/load tests remain dispatch-only pending staging and
  owner approval.
