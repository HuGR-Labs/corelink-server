# Production deployability gate

The PR-only `production-deployability` workflow is the fork-safe preflight for production
configuration. Its closed inventory is `reports/production-deploy-surfaces.v1.json`, and
`scripts/verify_prod_deployability.py` validates that inventory at both the pull request base
and head commits.

The check is intentionally credential-free and network-free: it validates committed Wrangler
TOML, production environment blocks, entrypoints, source/generated artifact declarations, and
the workflow or package deploy command that owns each surface. It discovers every committed
`wrangler.toml`, so adding a production config without adding an auditable manifest record is
red. The only exclusion is the documented `corelink-clerk-cf` proof-of-concept worker.

The preflight uses a local Python virtual environment, a ten-minute timeout, and no
`pull_request_target` or secrets. Production publishing remains confined to the existing
push/manual deploy workflows. The only excluded Wrangler file is an immutable allowlist
entry whose canonical POC name, entrypoint, environment identity, and reason are checked
at both base and head; a production surface cannot be moved into that exclusion.
