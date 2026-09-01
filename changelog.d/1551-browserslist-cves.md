### Security

- **`browserslist` is pinned to 4.28.7, the fixed release for
  CVE-2026-73088 and CVE-2026-73089.** The lockfile had resolved both 4.28.2
  and 4.28.6 through the documentation and admin-ui build dependency trees.

### Fixed

- **The workspace now has one reproducible `browserslist` resolution.** A root
  pnpm override consolidates every transitive consumer on 4.28.7 instead of
  relying on independently resolved vulnerable copies.
