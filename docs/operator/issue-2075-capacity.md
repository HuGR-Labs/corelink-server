# Issue 2075: documented capacity and live instance receipt

The protected receipt uses the documented Wrangler commands `containers list
--json` and `containers instances <APPLICATION_ID> --json`. It resolves a fixed
allowlist of the five production CoreLink applications, reads each instance
state, and retains only state counts by allowlist position. Provider IDs,
application names, instance names, image references, locations, versions, raw
responses, and credentials are discarded in memory and never written to logs
or artifacts.

Cloudflare's [Containers limits and instance types](https://developers.cloudflare.com/containers/platform/limits/)
page documents per-account concurrent ceilings of 1,500 vCPU, 6 TiB memory,
and 30 TB disk, and defines `basic` as 0.25 vCPU, 1 GiB, and 4 GB disk. These
published platform ceilings are reported separately from account-specific
entitlements. The five production declarations in `wrangler.toml` each cap at
200 `basic` instances; their configured reservation totals 1,000 instances,
250 vCPU, 1,000 GiB memory, and 4,000 GB disk.

The live instance census covers only the five applications. The receipt counts
provider states and estimates the running instances' resource allocation from
the declared `basic` shape. This is not measured account resource usage or
billable consumption. Published account ceilings are reported separately from
account-specific entitlements, which this read does not verify.

The receipt marks active tenant-region assignments unavailable. The existing
provider and repository evidence contract does not equate running instance
health, registered tenant state, or recent audit activity with concurrent
tenant workload or tenant-to-instance assignments. It also does not claim a
deduplicated cross-region tenant count.

The workflow is dispatch-only, requires the protected `production-capacity-read`
environment, a literal confirmation, the `main` ref, and exact checked-out
commit SHA equality. Its invocation uses the pinned Wrangler version, fixed
read-only list commands, and uploads only the aggregate receipt and digest.
The Wrangler Containers command permission scope is `containers:write`, although
this workflow invokes only list/read commands. Disable the workflow after its
single authorized read.

Cloudflare also documents time-integrated container metrics through
[`containersMetricsAdaptiveGroups` and `containersUsageAdaptiveGroups`](https://developers.cloudflare.com/analytics/graphql-api/tutorials/querying-container-metrics/).
Those analytics are resource estimates over a selected time window, not an
instantaneous account quota or billing measurement, so the receipt does not
present them as current account usage.
