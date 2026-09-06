Fixed: migrate the CodeQL nightly matrix from unavailable hosted Actions to the
product-owned `corelink` fleet, with bounded concurrency, dependency caching,
retained SARIF evidence, and a fail-closed evidence watchdog. Advanced Security
enablement remains an owner-side action for Security-tab ingestion.
