### Fixed

- **Daily `secrets-drift` evidence no longer disappears silently (B-132).** The
  scheduled scan runs on the product-owned `corelink` fabric, fails closed when
  its 90-day report artifact is absent, and has an independent API-backed
  watchdog that opens or updates a stable evidence-gap issue for absent,
  failed, stale, or artifact-less runs.
