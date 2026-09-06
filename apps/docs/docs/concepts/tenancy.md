---
id: tenancy
title: Tenant model and PAT scoping
sidebar_position: 2
description: "How CoreLink isolates tenants, how PATs are scoped, and what cross-tenant access looks like (answer: impossible)."
---

# Tenant model and PAT scoping

## What is a tenant?

A **tenant** is the top-level isolation boundary in CoreLink. Every piece of data — CAS blobs, AC entries, audit events, billing records — belongs to exactly one tenant. No data is ever readable or writable across tenants.

You receive a UUIDv7 tenant ID during sign-up. It is a canonical UUID, for
example:

```text
0192f6e0-7b4a-7abc-8def-0123456789ab
```

Tenant addressing depends on the API surface. Tenant-addressed HTTP cache
routes include the tenant ID in the path:

```
/v1/cas/0192f6e0-7b4a-7abc-8def-0123456789ab/<sha256>
/v1/ac/0192f6e0-7b4a-7abc-8def-0123456789ab/<action_digest>
```

Native REAPI v1 routes (for example `/v1/cas/blobs/<digest>/<size>`) resolve
the tenant from the authenticated PAT and do not repeat it in the URL. Bazel
REAPI v2 carries the tenant ID as its `instance` path segment:
`/bazel/v2/<tenant_uuidv7>/...`; the Worker and container reject a request when
that instance does not equal the PAT-resolved tenant.

## Personal Access Tokens (PATs)

PATs are the only credential type CoreLink accepts for API calls. There are no API keys, OAuth tokens, or service accounts — a PAT *is* the service account.

### PAT properties

| Property | Details |
|---|---|
| Format | `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` — `<env>` is `pat` (user PAT), `ci` (CI runner token), or `ro` (read-only token) |
| Scoped to | Exactly one tenant at issue time |
| Shown once | Displayed in plaintext only on creation; never stored in plaintext server-side |
| Revocable | Yes, from the dashboard's **Keys** page or via `POST /v1/customer/keys/{pat_id}/revoke` with a server-trusted `owner`/`admin` team role. Minting additional PATs is `POST /v1/customer/keys`; listing and write-capable minting require cache-write. |
| Expiry | Customer-issued PATs expire after 90 days. The customer mint request has no caller-selected expiry field. |

### PAT scopes

When creating a PAT you can restrict it to a subset of operations:

| Scope | Grants |
|---|---|
| `cache:read` (also `cas:r` / `read-only`) | Read CAS blobs and action-cache entries |
| `cache:write` (also `cas:rw` / `read-write`) | Read and write CAS blobs and action-cache entries; write includes read |
| `cache:find-missing` (also `find-missing`) | Probe blob existence with `FindMissingBlobs`, without read or write |
| `admin` | Legacy cache-read/write superset; not grantable by customer self-service |

Scopes are exact tokens (separated by whitespace or commas); unknown tokens are
rejected. Omitting a capability means the PAT cannot perform that operation.
The starter PAT issued during sign-up is stored as `read-write` — enough for
all build-tool integrations.

### CI/CD best practice

Do not use your personal starter PAT in CI. Mint a dedicated CI PAT with the
minimum required scopes from the dashboard's **Keys** page, or with
`POST /v1/customer/keys` (typically `{"name":"ci-<repo>","scopes":["cache:write"]}`;
`cas:rw` is an equivalent spelling, and `admin` is not customer-grantable).

Store the returned token value in GitHub Actions secrets, Vault, or your secrets manager of choice.

## Cross-tenant isolation

CoreLink enforces tenant isolation at every layer:

1. **API routing**: tenant-addressed `/v1/cas/<tenant>/...` and
   `/v1/ac/<tenant>/...` routes validate the URL tenant against the PAT before
   touching storage. Native REAPI v1 resolves the tenant from the PAT, while
   Bazel REAPI v2 validates its `<instance>` segment against that tenant.
2. **Storage layer**: R2 object keys are prefixed by tenant ID. A bug that omits the prefix check cannot produce a key collision that leaks data from another tenant because the prefix is mandatory, not optional.
3. **Audit log**: each event carries the tenant ID and is stored in a tenant-specific KV namespace. Admin-level queries are scoped to the calling tenant's namespace.

Cross-tenant reads are **not possible** — not as a configuration option, not by request, not via the admin API. If you need to share artifacts between two tenants (for example, a shared library used by two product teams), push the blob under both tenants or use a single shared tenant with multiple PATs scoped by team.

This isolation guarantee is documented as invariant **INV-TENANT-ISOLATION** in the security model. See [Security](../security.md) for the full invariant list.

## Organization vs. tenant

Today, CoreLink has a 1:1 mapping between organization (sign-up unit) and tenant. Multi-tenant organizations (where one billing entity manages sub-tenants for different teams or environments) are on the roadmap but not yet available.

A common pattern in the meantime: create separate accounts for production and
staging, each with its own UUIDv7 tenant and PATs. Build pipelines are
configured per environment.

## Listing and revoking PATs

`POST /v1/pats` is the live public self-service creation route. Dashboard users
can issue through the compatible `POST /v1/customer/keys` alias; both routes
share the `pat-issue` per-tenant limiter (burst 10, then 10/hour, one token
every 360 seconds) before mint and audit. Its bucket is durably serialized by
the tenant Durable Object across restarts and container replacement; malformed
or unavailable state fails closed with `503`. A revoked PAT receives `401
Unauthorized` on subsequent requests.

Listing and revocation are self-service: `GET /v1/customer/keys` lists the
tenant's PATs (metadata only — no token material) alongside its BYOK status,
and `POST /v1/customer/keys/{pat_id}/revoke` revokes one. Listing retains its
cache-write gate; revocation is a destructive team-admin operation requiring
the server-trusted `owner`/`admin` role. Admins may revoke member PATs but not
owner PATs; members, viewers, and native PAT callers get `403`.

A separate admin-only read surface exists for support to inspect a tenant's
PATs (`GET /v1/admin/tenants/{tenant_id}/pats`); it needs an admin PAT and is
not callable with a regular customer token.
