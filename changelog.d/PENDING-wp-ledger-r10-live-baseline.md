### Changed

- Re-anchored the work-package ledger to the current `origin/main` baseline,
  mechanically checked the 107-item open population, and excluded terminal
  B-168 from dispatch.
- Added fail-closed checks for stale population metadata and cyclic executable
  WP dependencies; the CI grammar lane now requires both WP-140 and WP-146
  before WP-148, followed by WP-150.
