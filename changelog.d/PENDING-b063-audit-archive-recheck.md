### Changed

- **B-063 remains open after a fresh production recheck.** Three read-only
  §3.3 queries on 2026-09-05 still returned 188 old unarchived rows in the
  redacted `93da3f7a/enam` partition; the available detector runs remain
  failures. The B-112 CLI release-chain repair is unrelated to the archive
  path and is not production recovery evidence.
