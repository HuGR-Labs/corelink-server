### Fixed

- **Buck2 starter toolchain cell.** The starter now maps the bundled prelude's
  `toolchains` cell and provides its minimal system C++ toolchain target,
  allowing the pinned release to evaluate the C++ graph without adding
  speculative cells.
