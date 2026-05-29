---
id: tenancy
title: Tenant model and PAT scoping
sidebar_position: 2
description: "How CoreLink isolates tenants, how PATs are scoped, and what cross-tenant access looks like (answer: impossible)."
---

# Tenant model and PAT scoping

## What is a tenant?

A **tenant** is the top-level isolation boundary in CoreLink. Every piece of data — CAS blobs, AC entries, audit events, billing records — belongs to exactly one tenant. No data is ever readable or writable across tenants.

You receive a tenant ID during sign-up. It looks like a short, URL-safe identifier:

```text
acme-prod
```

The tenant ID appears in every API path:

```
/v1/cas/acme-prod/<sha256>
/v1/ac/acme-prod/<action_digest>
```

## Personal Access Tokens (PATs)

PATs are the only credential type CoreLink accepts for API calls. There are no API keys, OAuth tokens, or service accounts — a PAT *is* the service account.

### PAT properties

| Property | Details |
|---|---|
| Prefix | `clk_live_` (production) or `clk_test_` (test environment) |
| Scoped to | Exactly one tenant at issue time |
| Shown once | Displayed in plaintext only on creation; hash stored server-side |
| Revocable | At any time from the admin dashboard or `DELETE /v1/pats/:pat_id` |
| Expiry | Optional; set at creation time; defaults to non-expiring |

### PAT scopes

When creating a PAT you can restrict it to a subset of operations:

| Scope | Grants |
|---|---|
| `cas:read` | `GET /v1/cas/*` |
| `cas:write` | `PUT /v1/cas/*` |
| `ac:read` | Read action cache entries |
| `ac:write` | Write action cache entries |
| `admin` | User management, PAT management, audit log export |

Omitting a scope means the PAT cannot perform that operation. The starter PAT issued during sign-up has `cas:read cas:write ac:read ac:write` — enough for all build tool integrations.

### CI/CD best practice

Do not use your personal starter PAT in CI. Create a dedicated CI PAT with the minimum required scopes:

```bash
# Create a CI PAT with CAS + AC access only (no admin)
curl -s -X POST \
  -H "Authorization: Bearer $CORELINK_ADMIN_PAT" \
  -H "Content-Type: application/json" \
  -d '{"label": "ci-bazel", "scopes": ["cas:read", "cas:write", "ac:read", "ac:write"]}' \
  https://corelink-api.humangr.com/v1/pats
```

Store the returned token value in GitHub Actions secrets, Vault, or your secrets manager of choice.

## Cross-tenant isolation

CoreLink enforces tenant isolation at every layer:

1. **API routing**: every CAS and AC request carries the tenant ID in the URL path. The worker validates that the PAT's tenant matches the path tenant before touching storage.
2. **Storage layer**: R2 object keys are prefixed by tenant ID. A bug that omits the prefix check cannot produce a key collision that leaks data from another tenant because the prefix is mandatory, not optional.
3. **Audit log**: each event carries the tenant ID and is stored in a tenant-specific KV namespace. Admin-level queries are scoped to the calling tenant's namespace.

Cross-tenant reads are **not possible** — not as a configuration option, not by request, not via the admin API. If you need to share artifacts between two tenants (e.g. a shared library used by two product teams), push the blob under both tenants or use a single shared tenant with multiple PATs scoped by team.

This isolation guarantee is documented as invariant **INV-TENANT-ISOLATION** in the security model. See [Security](../security.md) for the full invariant list.

## Organization vs. tenant

Today, CoreLink has a 1:1 mapping between organization (sign-up unit) and tenant. Multi-tenant organizations (where one billing entity manages sub-tenants for different teams or environments) are on the roadmap but not yet available.

A common pattern in the meantime: create separate accounts for `acme-prod` and `acme-staging`, each with their own PATs. Build pipelines are configured per-environment.

## Listing and revoking PATs

```bash
# List all PATs for your tenant
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/pats

# Revoke a PAT by ID
curl -s -X DELETE \
  -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/pats/<pat_id>
```

After revocation, any in-flight requests using that PAT will receive `401 Unauthorized`.
