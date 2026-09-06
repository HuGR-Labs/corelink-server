---
id: security
title: Security model
sidebar_position: 11
description: PAT lifecycle, scopes, rotation policy, audit log, and the INV-TENANT-ISOLATION guarantee.
---

# Security model

## PAT lifecycle

Personal Access Tokens (PATs) are the only credential type CoreLink accepts. Understanding their lifecycle is critical for operating securely.

### Creation

PATs are created in two places:

1. **Sign-up wizard** — issues a starter PAT stored with the canonical `read-write` scope.
2. **Self-service PAT issuance** — the **Keys** page in the dashboard, backed by [`POST /v1/customer/keys`](./reference/api/endpoints/post-v1-customer-keys.mdx). Send `{"name": "...", "scopes": ["cas:rw"]}` and the response carries the new token once. Customer-issued PATs have a fixed 90-day lifetime; the request has no expiry field.

   Minting a token that holds any write or admin scope requires the caller to already hold a cache-write capability — a read-only (`cas:r`) token cannot mint itself a write token, and gets `403`.

At creation time, the plaintext token is displayed **exactly once**. CoreLink never stores the plaintext. There is no retrieval endpoint.

**Store your PAT in a secret manager before closing the creation dialog.** Suitable options:

- 1Password / Bitwarden (personal)
- AWS Secrets Manager / HashiCorp Vault (team/CI)
- GitHub Actions secrets (CI workflows)
- Doppler / Infisical (platform engineering)

Do not store PATs in:

- Git repositories (even private ones)
- `.env` files committed to source control
- CI job logs (`echo $CORELINK_PAT` is fine for debugging locally; not in CI)
- Slack / Discord messages

### HTTPS only

CoreLink does not serve HTTP. All API traffic uses TLS 1.2 or TLS 1.3. HTTPS is enforced at the Cloudflare edge; there is no opt-out. Connections on port 80 are redirected to port 443.

### Rotation

PATs have no automatic rotation. Recommended rotation cadence:

| PAT type | Cadence |
|---|---|
| Starter (personal) | 90 days or on personnel change |
| CI/CD | 90 days or on team change |
| Integration (shared) | 30 days |

Rotation is self-service: mint the replacement on the **Keys** page, cut your
clients over, then revoke the old token from the same page.

### Revocation

Revoke from the **Keys** page in the dashboard, or call
[`POST /v1/customer/keys/{pat_id}/revoke`](./reference/api/endpoints/post-v1-customer-keys-by-pat_id-revoke.mdx)
directly. Note the shape: revocation is a `POST` to a `/revoke` sub-path, not a
`DELETE` on the token.

Revoking is a destructive team-admin operation. It requires the server-trusted
`owner` or `admin` role from a Clerk dashboard session, not cache-write scope
alone. An owner may revoke any PAT in the tenant; an admin may revoke member
PATs but not the owner's PAT. Members, viewers, native PAT callers, and
cross-tenant targets are rejected.

Once revoked, subsequent authentication checks reject that PAT with `401`.
The Worker may serve a previously verified positive PAT result from its edge
cache for up to 30 seconds; stop using a token immediately after revocation
and allow that bounded cache window to expire.

## Scopes

PATs are scoped at creation time. The available scopes are:

| Scope | Grants |
|---|---|
| `cache:read` (also `cas:r` / `read-only`) | Read (download) CAS blobs and action-cache entries |
| `cache:write` (also `cas:rw` / `read-write`) | Write CAS blobs and action-cache entries; write includes read |
| `cache:find-missing` (also `find-missing`) | Probe blob existence only |
| `admin` | Legacy cache-read/write superset; not grantable through customer self-service |

Scopes are exact tokens and unknown tokens are rejected. Principle of least
privilege: give each PAT the minimum capability required. A CI job that
populates the cache normally needs `cache:write`; a read-only cache proxy needs
`cache:read`.

## Tenant isolation (INV-TENANT-ISOLATION)

The core security invariant of CoreLink:

> **INV-TENANT-ISOLATION**: No request can read or write data belonging to a tenant other than the one encoded in the PAT, regardless of the URL path, headers, or request body.

This is enforced at two layers:

1. **PAT validation**: The worker resolves the PAT to a `tenant_id`. For tenant-addressed routes, a URL tenant that does not match is rejected with `403` before storage; native REAPI routes have no tenant segment and use the PAT-resolved tenant directly.
2. **Storage key namespace**: R2 object keys are prefixed `<tenant_id>/cas/<hash>`. A storage bug that accidentally omits the tenant check cannot produce a collision because the key still contains the tenant prefix.

CoreLink does not offer cross-tenant sharing. If two teams need to share artifacts, they must use a shared tenant or push the artifact to both tenants independently.

## What we audit

Every successful CAS read, CAS write, and AC operation is appended to an immutable, tenant-scoped audit log. Each entry records:

| Field | Example |
|---|---|
| `event_type` | `cas.write`, `cas.read`, `ac.write`, `ac.read` |
| `tenant_id` | `0192f6e0-7b4a-7abc-8def-0123456789ab` |
| `content_hash` | `sha256:e3b0c4...` |
| `pat_prefix` | `aZ3xQ1` |
| `ip_address` | `1.2.3.4` (hashed in GDPR-constrained regions) |
| `timestamp` | `2026-05-28T08:42:00.123Z` |
| `bytes` | `4096` |

The audit log is append-only. Individual entries cannot be deleted. You can export your tenant's audit log as JSON or CSV from the admin dashboard.

## Encryption at rest

All blobs stored in R2 are encrypted at rest using AES-256 with
Cloudflare-managed keys. Customer-managed BYOK is not currently shipped: the
operator-only activation route fails closed with `501 byok_not_available` when
the real provider is unavailable, and there is no customer self-service
activation. Do not assume an Enterprise plan has BYOK enabled; contact sales
for current external availability and rollout status.

## Responsible disclosure

If you find a security vulnerability in CoreLink, please email [security@humangr.com](mailto:security@humangr.com). We acknowledge reports within 24 hours and target patches within 72 hours for critical issues.

Do not open public GitHub issues for security vulnerabilities.
