# Issue 2075: documented Cloudflare capacity and live instance receipt

Cloudflare's [Containers limits and instance types](https://developers.cloudflare.com/containers/platform/limits/)
documentation (updated August 28, 2026) publishes per-account ceilings of
1,500 concurrent vCPU, 6 TiB concurrent memory, and 30 TB concurrent disk. It
defines `basic` as 0.25 vCPU, 1 GiB memory, and 4 GB disk. These public platform
ceilings are separate from any account-specific entitlement, which this receipt
does not verify.

The repository configures five production applications with up to 200 `basic`
instances each. That configured ceiling is 1,000 instances, 250 vCPU, 1,000 GiB
memory, and 4,000 GB disk if every configured slot is used. It is configuration
arithmetic, not a provider quota grant or a live usage measurement. The existing
regional budget in [issue 1656's capacity note](issue-1656-capacity.md) also
contains a separate runner reservation; the two declared reservations total
1,250 vCPU against the published 1,500 vCPU ceiling. The instance read below
does not measure runner use or account-wide resource consumption.

The protected receipt uses the documented Wrangler commands `containers list
--json` and `containers instances <APPLICATION_ID> --json`. It resolves a fixed
allowlist of the five production CoreLink applications, reads each instance
state, and retains only state counts in allowlist order. Application and
instance IDs, application and instance names, image references, locations,
versions, raw provider responses, and credentials are discarded in memory and
never written to logs or artifacts. Provider stdout is streamed with a 1 MiB
limit and a 90-second command timeout; stderr is suppressed.

The observed section covers only the five allowlisted applications. It reports
state counts and estimates running-instance allocation from the declared
`basic` shape. The estimate is not provider account usage or billable
consumption. The receipt marks active tenant-region assignments and the
deduplicated tenant count unavailable: instance state and registered tenant
state do not establish active tenant workload or tenant-to-instance
assignments.

The workflow is manual-only. It requires the protected `production-capacity-read`
environment, an explicit confirmation string, the `main` ref, and exact
checked-out commit SHA equality. The hosted verifier exercises the parser and
redaction contract without credentials. No protected provider workflow has
been dispatched for this change.

Cloudflare's [Wrangler command reference](https://developers.cloudflare.com/workers/wrangler/commands/containers/)
documents JSON output for both list commands. Cloudflare also documents
time-integrated container metrics through
[`containersMetricsAdaptiveGroups` and `containersUsageAdaptiveGroups`](https://developers.cloudflare.com/analytics/graphql-api/tutorials/querying-container-metrics/).
Those analytics are estimates over a selected time window, so the receipt does
not present them as an instantaneous account quota, current account usage, or
tenant-concurrency signal.
