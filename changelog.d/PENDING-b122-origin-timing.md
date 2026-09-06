### Fixed

- **fix(observability): preserve truthful phase attribution across blocking tasks (B-122).**
  The moat cache now carries the request timing ledger into both synchronous CAS
  adapter calls made by `spawn_blocking`. `timed` and `PhaseScope` coalesce each
  phase's active wall-clock windows, so an inner `Store` scope or overlapping
  sibling scopes cannot count the same milliseconds twice. This keeps
  `Server-Timing` additive while retaining bounded, real measurements for
  blocking regions. Production baselines for
  B-102/B-107 remain pending a redeploy and are intentionally not asserted here.
