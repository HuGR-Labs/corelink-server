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

1. **Sign-up wizard** — issues a starter PAT with `cas:read cas:write ac:read ac:write` scopes.
2. **Admin dashboard** or `POST /v1/pats` — issues a PAT with any subset of scopes.

At creation time, the plaintext token is displayed **exactly once**. CoreLink stores only a salted SHA-256 hash of the token value. There is no retrieval endpoint.

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

PATs have no automatic rotation. You must rotate manually. Recommended rotation cadence:

| PAT type | Cadence |
|---|---|
| Starter (personal) | 90 days or on personnel change |
| CI/CD | 90 days or on team change |
| Integration (shared) | 30 days |

To rotate a PAT:

1. Create the new PAT (`POST /v1/pats`).
2. Update the secret in your secret manager.
3. Deploy/restart anything consuming the old PAT.
4. Revoke the old PAT (`DELETE /v1/pats/:pat_id`).

Do not delete the old PAT before the new one is in use — there is no grace period.

### Revocation

```bash
# List PATs to find the ID
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/pats

# Revoke
curl -s -X DELETE \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/pats/<pat_id>"
```

Revocation is immediate. In-flight requests using the revoked PAT will fail with `401` within milliseconds (bounded by the Cloudflare edge propagation window, typically < 100 ms).

## Scopes

PATs are scoped at creation time. The available scopes are:

| Scope | Grants |
|---|---|
| `cas:read` | Read (download) blobs from CAS |
| `cas:write` | Write (upload) blobs to CAS |
| `ac:read` | Read action cache entries |
| `ac:write` | Write action cache entries |
| `admin` | PAT management, user management, audit log export |

Principle of least privilege: give each PAT the minimum set of scopes required. A CI job that only populates the cache needs `cas:write ac:write`; a read-only cache proxy needs `cas:read ac:read`.

## Tenant isolation (INV-TENANT-ISOLATION)

The core security invariant of CoreLink:

> **INV-TENANT-ISOLATION**: No request can read or write data belonging to a tenant other than the one encoded in the PAT, regardless of the URL path, headers, or request body.

This is enforced at two layers:

1. **PAT validation**: The worker resolves the PAT to a `tenant_id`. If the URL path tenant does not match, the request is rejected with `403` before any storage operation.
2. **Storage key namespace**: R2 object keys are prefixed `<tenant_id>/cas/<hash>`. A storage bug that accidentally omits the tenant check cannot produce a collision because the key still contains the tenant prefix.

CoreLink does not offer cross-tenant sharing. If two teams need to share artifacts, they must use a shared tenant or push the artifact to both tenants independently.

## What we audit

Every successful CAS read, CAS write, and AC operation is appended to an immutable, tenant-scoped audit log. Each entry records:

| Field | Example |
|---|---|
| `event_type` | `cas.write`, `cas.read`, `ac.write`, `ac.read` |
| `tenant_id` | `acme-prod` |
| `content_hash` | `sha256:e3b0c4...` |
| `pat_prefix` | `clk_live` |
| `ip_address` | `1.2.3.4` (hashed in GDPR-constrained regions) |
| `timestamp` | `2026-05-28T08:42:00.123Z` |
| `bytes` | `4096` |

The audit log is append-only. Individual entries cannot be deleted. You can export your tenant's audit log as JSON or CSV from the admin dashboard.

## Encryption at rest

All blobs stored in R2 are encrypted at rest using AES-256 (Cloudflare-managed keys by default). Enterprise plan tenants can supply their own AES-256 key (BYOK). Contact sales to enable BYOK.

## Responsible disclosure

If you find a security vulnerability in CoreLink, please email [security@humangr.com](mailto:security@humangr.com). We acknowledge reports within 24 hours and target patches within 72 hours for critical issues.

Do not open public GitHub issues for security vulnerabilities.
