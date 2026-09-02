### Fixed

- Fixed the B-075 and B-149 backlog checks so a partial worker dependency
  install cannot masquerade as a product regression and refactored test modules
  remain measured at their real compiled locations. B-149 now uses a
  mutation-tested, function-scoped verifier that rejects dead/comment-only
  evidence, wildcard enum coverage, incomplete fixtures, and stable reason-code
  drift. B-075 now validates both the executable Vitest shim and its package
  entrypoint, repairing the exact interrupted-prune state that can leave only
  the shim behind.
