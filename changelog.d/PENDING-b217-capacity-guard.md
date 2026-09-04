### Added

- **B-217 makes free build capacity a verified precondition for heavy local/CI work.** The new guard measures bytes and inodes, reports regenerable worktree caches and active/dirty/untracked candidates, serializes shared-cache materialization, and offers only narrowly validated, explicit cache cleanup rather than automatic deletion.
- Cleanup now fails closed on tracked cache paths, ignored data outside disposable roots, read/scan errors, divergent `main`/`master`, and unsafe symlink/deletion descriptors; child ENOSPC failures remain non-zero.
