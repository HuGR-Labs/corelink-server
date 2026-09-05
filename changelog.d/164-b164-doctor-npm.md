### Fixed

- **B-164 — CLI diagnostics and npm troubleshooting now point at the real failure.**
  `corelink doctor` preserves HTTP status codes without response bodies, reporting invalid
  PATs as `COR_AUTH_INVALID` instead of quota exhaustion (and distinguishing forbidden,
  rate-limited, internal, and network failures). The npm integration guide and all three
  translations now inspect the scoped `@scope:registry` value and HTTP install log instead
  of querying npm's unrelated global registry.
