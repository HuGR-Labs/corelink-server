### Fixed

- **B-163 raw-curl directory uploads now fail closed and run portably.** The
  recipe materializes the archive with a Darwin/Linux-safe `mktemp` template,
  binds the URL digest to the exact uploaded bytes, and uses `curl
  --fail-with-body` so real HTTP 422/500 responses cannot print a false success.
  The English page, all three translations, and the shell fixture are covered
  by bash/zsh tests, stale-file cleanup checks, and a real local HTTP failure
  matrix covering both shells, all four locales, and statuses 422/500. The
  backlog verifier now invokes that focused test as its single source of truth,
  including the executable-command check for `--fail-with-body`.
