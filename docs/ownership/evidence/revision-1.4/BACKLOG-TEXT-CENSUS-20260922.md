# Backlog text census — 2026-09-22

This is a read-only lexical census of the 105 provisional package identities
against the repository `BACKLOG.md` at the campaign checkout. It is not a
semantic duplicate decision and it does not promote the publication backlog
gate.

## Method

For every `package` and `manifest` in `docs/ownership/registry.json`, the
case-insensitive literal counts were computed in `BACKLOG.md`:

```text
count(package name) + count(manifest path)
```

An occurrence can be a historical reference, a consumer, a tracker or an
unrelated mention. It must be reviewed with aliases, issue intent and the
canonical backlog before a package can be marked `reuse`, `expand` or
`distinct`.

## Result

- Registry population checked: **105**.
- Packages with at least one literal name/manifest occurrence: **37**.
- Packages with no direct literal occurrence: **68**.
- An expanded literal pass over skill slugs and manifest directories found no
  additional package beyond the same 37; this still is not semantic review.
- Semantic decisions made by this census: **0**.
- Publication gate state: **PENDING**.

### 37 with literal occurrences

`corelink-ac`, `corelink-adapter-host`, `corelink-audit`,
`corelink-audit-chain`, `corelink-auth`, `corelink-billing`,
`corelink-billing-stripe`, `corelink-billing-stripe-materializer`,
`corelink-byok`, `corelink-cas`, `corelink-clerk`, `corelink-clerk-cf`,
`corelink-cli`, `corelink-client-verify`, `corelink-dual-approval`,
`corelink-enterprise-inquiry`, `corelink-failover-router`, `corelink-gc`,
`corelink-handler-cas`, `corelink-handler-customer`, `corelink-hash`,
`corelink-ops`, `corelink-pat`, `corelink-privacy`,
`corelink-privacy-erasure-worker`, `corelink-privacy-pseudonymize`,
`corelink-ratelimit`, `corelink-region`, `corelink-runbook-tracker`,
`corelink-server`, `corelink-slack-real`, `corelink-slo`,
`corelink-stripe-real`, `corelink-turbo-bridge`, `corelink-worker`,
`e2e-pilot-onboarding`, `e2e-tenant-isolation`.

### 68 without a direct literal occurrence

`chaos-campaign`, `corelink-ac-fuzz`, `corelink-adapters-cloud`,
`corelink-adapters-vault`, `corelink-analytics`, `corelink-audit-chain-fuzz`,
`corelink-bazel-bridge`, `corelink-billing-aggregator`,
`corelink-billing-emit`, `corelink-billing-reconcile`,
`corelink-billing-stripe-traits`, `corelink-byok-fuzz`, `corelink-cf-bindings`,
`corelink-chaos-scheduler`, `corelink-cli-fuzz`, `corelink-client-verify-fuzz`,
`corelink-config-do`, `corelink-core`, `corelink-crypto`,
`corelink-dpa-acceptance`, `corelink-dsr`, `corelink-dsr-statuspage-scheduler`,
`corelink-dt-cli`, `corelink-dt-reconcile`, `corelink-dt-webhook`,
`corelink-erasure-attestation`, `corelink-eviction`, `corelink-go`,
`corelink-handler-ac`, `corelink-handler-admin`, `corelink-handler-cas-erase`,
`corelink-hash-fuzz`, `corelink-meta`, `corelink-meta-fuzz`, `corelink-openapi`,
`corelink-py`, `corelink-r2-multipart`, `corelink-rate-headers`,
`corelink-reapi`, `corelink-reapi-fuzz`, `corelink-replica-worker`,
`corelink-replication`, `corelink-replication-coordinator`,
`corelink-rotation-adapters`, `corelink-runner-aggregate`,
`corelink-runner-overage`, `corelink-signup`, `corelink-statuspage-real`,
`corelink-telemetry`, `corelink-tenant-path`, `corelink-tenant-path-fuzz`,
`corelink-terraform-drift-consumer`, `corelink-tier-selection`,
`corelink-tracing`, `corelink-transparency-log`, `corelink-wasm`,
`corelink-worker-fuzz`, `e2e-billing-flow`, `e2e-byok-revoke`, `e2e-chaos`,
`e2e-dsr`, `e2e-failover-router`, `e2e-replication-failover`, `e2e-resilience`,
`e2e-signup-flow`, `e2e-user-journeys`, `migrate-single-to-multi-region`,
`sbom-publish`.

## Boundary

The census closes only the mechanical text-search portion of the backlog
preflight. It does not establish that a package has or lacks an existing issue,
does not inspect aliases or historical names, and does not alter the publication
ledger. Those decisions remain explicitly owned by the deduplication gate.
