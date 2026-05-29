---
id: http
title: HTTP API reference
sidebar_position: 1
description: Authentication, endpoints, request/response shapes, and error codes for the CoreLink REST API.
---

# HTTP API reference

Base URL: `https://corelink-api.humangr.com`

All endpoints require HTTPS. HTTP is not accepted.

## Authentication

Every request must carry a `Authorization: Bearer <PAT>` header.

```bash
curl -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

No other authentication scheme (Basic, API key header, query param) is accepted. If the header is missing or malformed, the API returns `401`.

## Endpoints

### `GET /v1/users/me`

Returns the identity of the PAT used in the request.

**Request**

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

**Response 200**

```json
{
  "tenant_id": "acme-prod",
  "token_prefix": "clk_live",
  "route_kind": "cas"
}
```

| Field | Type | Description |
|---|---|---|
| `tenant_id` | string | The tenant this PAT is scoped to. Matches the path segment in CAS/AC URLs. |
| `token_prefix` | string | `clk_live` (production) or `clk_test` (test environment). |
| `route_kind` | string | Always `cas` for data-plane PATs. |

---

### `PUT /v1/cas/<tenant_id>/<sha256>`

Upload a blob. The SHA-256 in the URL path must match the SHA-256 of the request body. If it does not match, the server returns `422`.

**Parameters**

| Name | In | Required | Description |
|---|---|---|---|
| `tenant_id` | path | yes | Your tenant identifier. Must match the PAT's tenant. |
| `sha256` | path | yes | Lowercase hex SHA-256 of the blob bytes (64 characters). |

**Headers**

| Header | Required | Value |
|---|---|---|
| `Authorization` | yes | `Bearer <PAT>` |
| `Content-Type` | recommended | `application/octet-stream` |
| `Content-Length` | recommended | byte length of body |

**Request**

```bash
DIGEST=$(sha256sum ./output.tar.gz | awk '{print $1}')

curl -s -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @./output.tar.gz \
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST"
```

**Response 201**

```json
{"hash": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}
```

**Response 409** — blob already exists (idempotent; safe to ignore)

```json
{"error": "conflict", "message": "blob already exists"}
```

---

### `GET /v1/cas/<tenant_id>/<sha256>`

Download a blob by digest.

**Parameters**

| Name | In | Required | Description |
|---|---|---|---|
| `tenant_id` | path | yes | Your tenant identifier. |
| `sha256` | path | yes | Lowercase hex SHA-256. |

**Request**

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST" \
  -o ./output-downloaded.tar.gz
```

**Response 200** — `Content-Type: application/octet-stream`, body is raw bytes.

**Response 404** — blob not in tenant's CAS.

---

### `GET /api/health`

Health check. Returns `200 OK` with `{"status": "ok"}` when the service is up. No authentication required.

```bash
curl -s https://corelink-api.humangr.com/api/health
# {"status":"ok"}
```

---

### `GET /v1/pats`

List all PATs for your tenant.

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/pats
```

**Response 200**

```json
[
  {
    "pat_id": "pat_01HX...",
    "label": "ci-bazel",
    "scopes": ["cas:read", "cas:write", "ac:read", "ac:write"],
    "created_at": "2026-05-01T12:00:00Z",
    "expires_at": null,
    "last_used_at": "2026-05-28T08:42:00Z"
  }
]
```

---

### `POST /v1/pats`

Create a new PAT.

```bash
curl -s -X POST \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "Content-Type: application/json" \
  -d '{
    "label": "ci-bazel",
    "scopes": ["cas:read", "cas:write", "ac:read", "ac:write"],
    "expires_in_days": 90
  }' \
  https://corelink-api.humangr.com/v1/pats
```

**Response 201**

```json
{
  "pat_id": "pat_01HX...",
  "label": "ci-bazel",
  "token": "clk_live_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
  "scopes": ["cas:read", "cas:write", "ac:read", "ac:write"],
  "expires_at": "2026-08-28T00:00:00Z"
}
```

The `token` field is only present in the creation response. It is not retrievable again.

---

### `DELETE /v1/pats/:pat_id`

Revoke a PAT immediately.

```bash
curl -s -X DELETE \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/pats/pat_01HX..."
```

**Response 204** — revoked. Subsequent requests with that PAT return `401`.

---

## Error codes

| HTTP status | `error` field | Meaning | Fix |
|---|---|---|---|
| `400 Bad Request` | `bad_request` | Malformed request (invalid JSON, missing field) | Check request body |
| `401 Unauthorized` | `unauthorized` | Missing or invalid PAT | Verify `Authorization` header |
| `403 Forbidden` | `forbidden` | PAT does not have the required scope, or tenant mismatch | Check PAT scopes and tenant in URL path |
| `404 Not Found` | `not_found` | Blob does not exist in the tenant's CAS | Push before pulling |
| `409 Conflict` | `conflict` | Blob already exists (PUT) | Idempotent — safe to ignore |
| `422 Unprocessable Entity` | `hash_mismatch` | SHA-256 in URL does not match body | Recompute the digest |
| `429 Too Many Requests` | `rate_limited` | Request rate exceeded | Back off and retry; see `Retry-After` header |
| `503 Service Unavailable` | `audit_closed` | Tenant's audit period is closed — writes temporarily suspended | Contact support; reads still work |

All error responses share this shape:

```json
{
  "error": "not_found",
  "message": "blob sha256:abc123... not found in tenant acme-prod"
}
```

## Rate limits

Rate limits are applied per-tenant per-endpoint family. The response includes:

```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 998
X-RateLimit-Reset: 1717000000
Retry-After: 60   (only on 429)
```

Default limits (subject to change per plan):

| Operation | Limit |
|---|---|
| CAS reads | 1 000 req/min per tenant |
| CAS writes | 500 req/min per tenant |
| Management (PAT CRUD) | 60 req/min per tenant |

Enterprise plans have higher limits. Contact sales for custom limits.

## Pagination

List endpoints (`GET /v1/pats`, audit log) return cursor-based pagination:

```json
{
  "items": [...],
  "next_cursor": "eyJ...",
  "has_more": true
}
```

Pass `?cursor=<next_cursor>` to fetch the next page.
