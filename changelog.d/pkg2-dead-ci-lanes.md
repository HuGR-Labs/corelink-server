### Fixed

- **`neon-shadow-reconcile-daily` used GNU `date -d` on a macOS runner (PKG-2).**
  The lane runs on the self-hosted `corelink-builder` fleet, where
  `date -u -d "yesterday"` is `date: illegal option -- d`; under `set -e` the
  step aborted before reading anything, which is why the lane had 59 runs and
  zero successes. Switched to the BSD `-v-1d` form with a GNU fallback, so it
  keeps working if the lane ever moves to Linux. Verified by executing the real
  step: exit 0, `Reconciling for UTC date: 2026-08-29`.
