### Fixed

- **The WP-6 completeness command in `docs/campaigns/remediation/ROADMAP.md` was
  false-red by construction.** `gh pr view 1397 --json files` returns **100** of
  the PR's 173 files — the default GraphQL page, with no truncation notice — so
  the prescribed `diff` exited 1 against a correct manifest. On the REST
  endpoint the field is `filename`, not `path`: `--jq '.[].path'` returns 173
  **empty** lines and `jq` raises nothing. The command now paginates the REST
  listing and skips the TSV header. This is the roadmap's own §0.2 rule —
  *never conclude from truncated output* — failing inside the roadmap.
