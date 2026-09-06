### Fixed

- **B-114/B-135/B-138 runner-image boundary is now machine-checked locally.** The
  server repository keeps these sibling-repository items explicitly open/manual,
  refuses invented DevEnv-image or OCI-label wiring, and requires the local
  container build to fail before attempting work when its baked tools are absent;
  the verifier also requires that preflight step to precede the first build
  invocation, so a later workflow reorder cannot bypass the boundary. Its small
  shell lexer ignores YAML/shell comments and echo/string bait, and requires an
  active `for`/`if`/`exit 1` sequence rather than matching prose.
  A bounded mutation self-test proves each guard can detect its weakened form;
  it does not claim that the sibling image exists, that its trigger/labels are
  consumed, or that the sibling build's containerd disk peak is fixed.
