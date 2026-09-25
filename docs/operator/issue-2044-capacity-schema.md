# Issue 2044: safe Cloudflare capacity schema observation

The protected observation workflow makes one `GET /accounts/{account}/cloudchamber/me`
request from `main`. This is the exact route used by the Cloudchamber
`AccountService.getMe` client in the repository's locked Wrangler 4.111.0.
The capture does not walk or print provider-defined object keys.

The artifact records JSON types for a closed list of source-controlled capacity
path predicates. It records all other object members only as one aggregate key
count. It never records provider values, arbitrary paths, error bodies, headers,
or the token. `success` must be exactly `true` and `errors` exactly an empty
array. The response is capped at 1 MiB, 256 JSON nodes, and depth 8; exceeding
any bound fails closed without producing a receipt.

The fixed capacity predicates cover the Cloudchamber client contract: account
vCPU limit, per-deployment vCPU, memory limit, and usage. They also check the
Cloudflare API envelope and the result-wrapped forms. A predicate reports only
`missing` or a JSON type, never the provider value.

This observation can establish which allowlisted paths are present and their
types; it cannot reveal an undocumented field name or turn an unknown usage
member into a concurrency measurement. Bind a parser only to the Wrangler
client contract and keep concurrency as a separate evidence question unless
Cloudflare supplies a documented, measurable response field.

## Public Cloudflare sources and signal boundaries (reviewed 2026-09-24)

Cloudflare's [Containers limits documentation](https://developers.cloudflare.com/containers/platform/limits/)
states standard per-account ceilings of 1,500 concurrent vCPU, 6 TiB concurrent
memory, and 30 TB concurrent disk. It also says to contact the account team,
open a support ticket, or use its form for higher account-level limits. These
are authoritative platform ceilings; they do not document the response schema
or establish whether this account has a customized entitlement. A historical
`GET /accounts/{account}/containers/me` receipt records values returned for
this account, but does not document that endpoint's schema. The separate
`GET /accounts/{account}/cloudchamber/me` observation follows the locked
Wrangler Cloudchamber client contract; that client contract is not a published
Cloudflare API schema.

Cloudflare's [Containers GraphQL metrics guide](https://developers.cloudflare.com/analytics/graphql-api/tutorials/querying-container-metrics/)
documents the `containersMetricsAdaptiveGroups` and
`containersUsageAdaptiveGroups` datasets. The examples expose fields including
`cpuTimeSec`, `allocatedMemory`, and `allocatedDisk`, grouped over explicit
date/time filters. These can support a measured usage report for the selected
window; they are time-filtered analytics aggregates, not a current instantaneous
account-usage snapshot or a concurrency count.

Cloudflare's [Wrangler Containers command reference](https://developers.cloudflare.com/workers/wrangler/commands/containers/)
documents `wrangler containers instances <APPLICATION_ID>` as a list of
instances for one application. Its JSON fields are `id`, `name`, `state`,
`location`, `version`, and `created`; non-interactive use fetches all pages.
This supports a timestamped, per-application instance census. It does not
identify tenants, document a cross-application atomic snapshot, or define
tenant-concurrency semantics. A fixed application allowlist and the repository's
one-container-per-active-tenant contract may support a separately labeled
container-count estimate, but cannot establish distinct concurrent tenants or
a provider-defined measurement window.

Therefore, the documented standard account ceiling is known, and Cloudflare
documents historical-window usage and per-application instance enumeration.
The remaining external gaps are an authoritative account-specific entitlement
readback schema (including any customization), a supported current usage field
with units/scope/time semantics, and a tenant-concurrency signal with its
population, units, scope, and measurement window. Do not bind these gaps to
unknown `/containers/me` fields or infer them from reservations or analytics.

## Capacity receipt parser

The later receipt mode accepts only the strict Cloudflare envelope
`success == true`, `errors == []`, `result` object, and `result.limits` object.
It binds quota to `total_vcpu`, `vcpu_per_deployment`,
`memory_mib_per_deployment`, and `total_memory_mib`. The locked Wrangler
4.111.0 Cloudchamber client reads these as `account.limits.*`. Each field must
be finite, numeric, positive, and consistent with its per-deployment
counterpart. The probe rejects any endpoint drift from that exact Wrangler
route before sending a request.

The receipt records usage and concurrency as `unavailable`. This endpoint has
not established a documented account measurement path for either signal, and
the parser never derives them from quota, reservations, container count, or
unknown provider members. A future provider-authoritative signal can replace
only those explicit unavailable records with a separately reviewed contract.

The workflow is protected by the `production-capacity-read` environment,
requires the literal `run-1656-read-only` confirmation, and accepts only a
manual dispatch on `main`. A merge alone does not run the observation.
