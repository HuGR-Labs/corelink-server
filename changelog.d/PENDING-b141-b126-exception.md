### Fixed

- **B-141/B-126 trust-boundary reconciliation:** the pull-request target census
  now records the file-size ratchet's narrow data-only exception. The ratchet
  remains fork-complete with read-only contents, base-loaded validator code,
  and candidate Git blobs only; mutation coverage rejects candidate execution,
  actions, writable permissions, non-base executables, and base resolution from
  `head.sha`/`HEAD_REF` aliases. The validator's only executable `git show`
  is required to consume the trusted `BASE_REF` expression; structural tests
  reject shell reassignment, aliases, inline environment overrides, and
  `$GITHUB_ENV` writes involving that token.
