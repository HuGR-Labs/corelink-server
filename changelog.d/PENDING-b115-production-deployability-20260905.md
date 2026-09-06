### Added

- **B-115 production deployability preflight.** Pull requests now run a fork-safe, credential-free gate against a closed inventory of all six production Wrangler surfaces. It validates base and head configurations, production entrypoints, source/generated artifacts, and deploy workflow/package markers; missing or stale inputs fail closed. The gate uses a bounded isolated Python interpreter and leaves credentialed production deploys push/manual-only.
