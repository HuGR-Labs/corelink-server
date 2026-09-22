# #2067 gRPC REAPI cache bridge contract

Status: frozen before implementation. Existing REST routes remain unchanged.

## Request boundary

| Field | Contract |
| --- | --- |
| transport | TLS HTTP/2 gRPC; no HTTP/REST fallback |
| endpoints | `engine_address`, `action_cache_address`, and `cas_address` resolve to the same ingress |
| auth | Every stateful RPC requires `authorization: Bearer <PAT>` and resolves tenant plus `cache:read`/`cache:write` from the authoritative PAT verifier. Tokens are never logged, cached as plaintext, or emitted in evidence. |
| instance name | Must equal the PAT-resolved tenant before any store access. |
| hash | SHA-256 only for Buck2 REAPI, using CoreLink's existing SHA-256 tagged tenant CAS keyspace. |
| execution | `Execution` is not implemented and returns gRPC `UNIMPLEMENTED`; the client platform is cache-only (`remote_enabled=false`, `remote_cache_enabled=true`). |

## Operations

| RPC | Scope | Backing path | Required response |
| --- | --- | --- | --- |
| CAS `FindMissingBlobs`, `BatchReadBlobs`, ByteStream `Read` | `cache:read` | decorated tenant CAS reader | exact tenant visibility; absent and cross-tenant are indistinguishable |
| CAS `BatchUpdateBlobs`, ByteStream `Write` | `cache:write` | decorated tenant CAS writer | SHA-256 digest and declared-size validated before persistence; quota, byte accounting, audit, and retries remain active |
| ActionCache `GetActionResult` | `cache:read` | decorated tenant AC lookup | `NOT_FOUND` on a miss |
| ActionCache `UpdateActionResult` | `cache:write` | decorated tenant AC update | protobuf result persists immutably; divergent overwrite is `ALREADY_EXISTS` |
| `Capabilities.GetCapabilities` | none | static | advertises SHA-256, ActionCache update enabled, and no execution capability |

`ActionResult` follows upstream fields 2 through 12, including
`ExecutedActionMetadata` and output symlinks. The bridge stores the canonical
protobuf result bytes and decodes the same complete vendored schema on a warm
read. It does not claim support for `Execution` RPCs; cache-only Buck2 clients
must keep `remote_enabled=false`.

Malformed digests and resource names are `INVALID_ARGUMENT`; quota or size
limits are `RESOURCE_EXHAUSTED`; backend or audit failures are `UNAVAILABLE`
or `INTERNAL`. None may degrade to an in-memory or REST path.

## Composition seam

The container passes the same already decorated CAS and AC trait objects used
by `routes::build_with_factory_and_byok` into the gRPC bridge. The bridge does
not construct a second handler set, since that bypasses production quota,
byte-accounting, tombstone, BYOK, and audit decorators. Its auth adapter calls
`PatVerifier::verify_capability` and makes its tenant/write result immutable
for each RPC.

Tonic routes and the current axum router multiplex over one HTTP/2 listener.
