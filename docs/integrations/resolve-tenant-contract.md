# `POST /internal/v1/auth/resolve-tenant` — consumer contract

Contract note for any external/internal consumer of the tenant resolver
(`crates/corelink-container/src/routes/auth_introspect.rs`). Codifies the A1
ordering decision (**Option 2, ratified**) so an arbitrary new user resolves
reliably.

## What it does

Resolves a Clerk principal id (`clerk_org_id` — an org id, else the user `sub`)
to its isolated CoreLink `tenant_id` via a **lookup-only** read of the
`tenant_org_map` table (migration 0083). It NEVER writes a mapping.

Request:

```json
{ "clerk_org_id": "org_..." }
```

Responses:

| Status | Body                              | Meaning                                   |
| ------ | --------------------------------- | ----------------------------------------- |
| `200`  | `{ "tenant_id": "<uuid>" }`       | Mapped — the org is provisioned.          |
| `404`  | `{ "error": "org_not_mapped" }`   | Not-yet-provisioned (see retry contract). |
| `401`  | `{ "error": "unauthorized" }`     | Missing/wrong dedicated internal secret.  |
| `400`  | `{ "error": "invalid_body" }`     | Body did not parse.                       |
| `503`  | (empty)                           | Fail-closed D1 backend fault.             |

## Ordering contract — why the resolver is lookup-only

**Option 1 (provision-in-resolver) was REJECTED.** The `clerk_org_id` is an
unverified, attacker-influenceable string; auto-creating a mapping on read would
risk binding a request to a WRONG / attacker-chosen tenant and break the tenant
isolation boundary. The resolver therefore stays LOOKUP-ONLY.

**Provisioning is the SOLE `tenant_org_map` writer.** Two provisioning
authorities exist, both writing the row out-of-band before/around a resolve:

- **CoreLink-Clerk:** the Clerk `user.created` webhook (signup-worker) writes
  the `clerk_org_id → tenant_id` row (`INSERT OR IGNORE`, idempotent).
- **githugr-Clerk:** the in-worker Option-B token exchange writes the row.

## Retry contract (the load-bearing part)

Because provisioning writes the row out-of-band, a `resolve-tenant` read can
race AHEAD of it — most commonly during **Svix webhook-delivery lag**, when the
`user.created` webhook has not yet been delivered/processed. In that window the
row simply is not there yet.

Therefore:

- **`404 org_not_mapped` is TRANSIENT during the provisioning window, not a
  permanent "no such tenant".** Consumers **MUST** retry a 404 with **bounded
  backoff** until the provisioning write lands. An arbitrary new user then
  resolves reliably first try after the lag clears.
- **`503` is the fail-closed D1-fault signal** — distinct from 404. It is also
  retryable, but it means a backend fault (never a provisioning-lag miss and
  never a guessed/wrong tenant).
- `401` / `400` are terminal client errors — do NOT retry.

## No endpoint behavior change

This note (and the accompanying docstrings) is a **contract clarification
only**. The endpoint's status codes, bodies, auth gate, and lookup-only posture
are UNCHANGED.

## See also

- `crates/corelink-container/src/routes/auth_introspect.rs` — handler
  (`handle_resolve_tenant`), SQL (`RESOLVE_TENANT_SQL`), decode helper.
- `apps/signup-worker/src/webhooks/clerk.ts` — the CoreLink-Clerk provisioning
  writer (`orgMapKeyFor`, the `tenant_org_map` insert).
- `CHANGELOG.md` — "Auto-provision the `tenant_org_map` on signup (go-live A1)".
