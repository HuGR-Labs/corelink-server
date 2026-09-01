### Fixed

- **The Docs Reality endpoint gate no longer treats a route as a prefix.** Exact
  routes, one-segment parameters, and explicitly declared catch-alls now have
  separate matching semantics, so a documented `/v1/zzz-nonexistent` cannot be
  laundered by a bare `/v1` entry. Route-shaped comments are excluded from the
  source inventory, and an empty/truncated route population or missing flagship
  recipe fails closed. Added focused regressions for exact, parameterized,
  wildcard, bare, nonexistent, malformed-config, and comment/prose cases.
