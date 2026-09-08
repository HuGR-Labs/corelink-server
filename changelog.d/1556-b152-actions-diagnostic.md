### Fixed

- **B-152 GitHub Actions diagnostic evidence collection (#1556).** The read-only
  collector now fails closed on capped or malformed pagination, timestamp
  precision that cannot be represented exactly, malformed step states, unavailable
  logs, unavailable local tooling, and bounded `gh` API timeouts. Its dedicated
  regression suite is collected by the official Python CI lane. It records
  evidence without asserting an unproven common root cause.
- The retained snapshot now carries the closed run/job/step identity population,
  UTC timestamps, log-body digests and conservative signatures for runner
  cancellation, Playwright webServer, job timeout, billing/startup, ENOSPC, and
  test failures. Missing logs remain `indeterminate`; no empty window can close
  the backlog item. `verify_b152_snapshot.py` validates the snapshot offline.
- A hosted-runner monitor now samples a bounded 20-minute window every 15
  minutes and retains metadata-only artifacts. Empty windows and unavailable
  logs remain explicitly non-closing.
