### Added

- # Buck2 starter: map the bundled prelude's public fbsource constraints

The Buck2 starter now maps the `fbsource` alias to a local public compatibility
shim containing only the seven configuration values referenced by the pinned
2026-08-01 bundled JavaScript prelude. This lets the prelude load without
vendoring or importing Meta-internal build targets.
