### Fixed

- **God-file ratchet hardening (B-126):** the gate now runs from the trusted PR
  base, covers pushes to `main`, compares candidate blobs without following
  working-tree paths, and rejects baseline additions, increases, duplicates,
  malformed rows, symlinks, and unreadable objects. A declared source-extension
  census and focal mutation tests prevent silent scope loss.
