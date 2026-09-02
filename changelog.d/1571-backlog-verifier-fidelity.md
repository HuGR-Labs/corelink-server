### Fixed

- Fixed the B-075 and B-149 backlog checks so a partial worker dependency
  install cannot masquerade as a product regression and refactored test modules
  remain measured at their real compiled locations. B-149 now pins SHA-256
  checkpoints for the complete approved test and production-mapping files:
  any byte drift is review-required rather than being accepted by a partial
  syntax interpretation. Its registry is fail-closed to exactly the five reviewed
  path/digest pairs, and checkpoint reads hold one `O_NOFOLLOW|O_DIRECTORY`
  root descriptor for the whole certificate, rejecting root and component
  symlinks, root escapes, FIFOs, and other non-regular files. B-075 continues to validate both the executable Vitest
  shim and its package entrypoint, repairing the exact interrupted-prune state
  that can leave only the shim behind.
