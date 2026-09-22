# Issue 2044: safe Cloudflare capacity schema observation

The protected observation workflow makes one `GET /accounts/{account}/containers/me`
request from `main`. The endpoint is not present in Cloudflare's published API
schema, so this capture does not walk or print provider-defined object keys.

The artifact records JSON types for a closed list of source-controlled capacity
path predicates. It records all other object members only as one aggregate key
count. It never records provider values, arbitrary paths, error bodies, headers,
or the token. `success` must be exactly `true` and `errors` exactly an empty
array. The response is capped at 1 MiB, 256 JSON nodes, and depth 8; exceeding
any bound fails closed without producing a receipt.

The fixed capacity predicates cover the response fields already used by the
operator contract and prior dated readback: account vCPU limit, per-deployment
vCPU, memory limit, and usage. They also check the Cloudflare API envelope and
the candidate result-wrapped forms. A predicate reports only `missing` or a
JSON type, never the provider value.

The earlier dated account readback reported `usage: null`. That is not a
measurement of active concurrency. This observation can establish which
allowlisted paths are present and their types; it cannot reveal an undocumented
field name or turn a null usage field into a concurrency measurement. Bind a
parser only to paths established by a reviewed artifact and keep concurrency
as a separate evidence question unless Cloudflare supplies a documented,
measurable response field.

The workflow is protected by the `production-capacity-read` environment,
requires the literal `run-1656-read-only` confirmation, and accepts only a
manual dispatch on `main`. A merge alone does not run the observation.
