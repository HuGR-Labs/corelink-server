### Fixed

- **B-152 GitHub Actions diagnostic evidence collection (#1556).** The read-only
  collector now fails closed on capped or malformed pagination, timestamp
  precision that cannot be represented exactly, unknown step states, unavailable
  logs, and unavailable local tooling. It records evidence without asserting an
  unproven common root cause.
