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
