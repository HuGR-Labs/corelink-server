### Changed

- Re-anchored the work-package ledger to the exact D03 logical base
  `cff0876fa8ee1312a76369338a7f4f9006ee7115`, mechanically checked the current
  56-item open population across 258 backlog records, and excluded terminal
  items from dispatch.
- Added fail-closed checks for stale population metadata and cyclic executable
  WP dependencies; the CI grammar lane now requires both WP-140 and WP-146
  before WP-148, followed by WP-150.
