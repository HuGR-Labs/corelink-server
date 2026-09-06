### Changed

- Re-anchored the work-package ledger to the exact D03 logical base/head
  `codex/d03-delivery-20260906@7b992e9db123abeb76381b1c1337011692f2e834`.
  That D03 head contained 312 backlog records (14 open, 259 done and 39
  parked); the B-313 repair records the resulting 313-item tree without
  changing the 14-item executable owner population and excludes terminal items
  from dispatch.
- Added fail-closed checks for stale population metadata and cyclic executable
  WP dependencies; the CI grammar lane now requires both WP-140 and WP-146
  before WP-148, followed by WP-150.
