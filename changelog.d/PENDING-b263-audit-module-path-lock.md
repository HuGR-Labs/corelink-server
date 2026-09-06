### Fixed

- **B-263 restores bundle compilation after the audit split.** The audit
  synthetic-data module now resolves from its committed sibling file, and the
  workspace lockfile records the existing `corelink-gc` dependency.
