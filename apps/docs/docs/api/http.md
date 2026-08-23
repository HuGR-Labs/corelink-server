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
  "token_prefix": "aZ3xQ1",
  "route_kind": "cas"
}
```

| Field | Type | Description |
|---|---|---|
| `tenant_id` | string | The tenant this PAT is scoped to. Matches the path segment in CAS/AC URLs. |
| `token_prefix` | string | A 6-character hash-derived identifier for log/rate-limit correlation — not a literal prefix of your token. |
| `route_kind` | string | Always `cas` for data-plane PATs. |

---

### `PUT /v1/cas/<tenant_id>/<blake3>`

Upload a blob. The native CAS is **BLAKE3**-keyed: the BLAKE3 digest in the URL path must match the BLAKE3 of the request body. If it does not match, the server returns `422 content hash mismatch`. (Compute it with `b3sum` — **not** `sha256sum`.)

**Parameters**

| Name | In | Required | Description |
|---|---|---|---|
| `tenant_id` | path | yes | Your tenant identifier. Must match the PAT's tenant. |
| `blake3` | path | yes | Lowercase hex BLAKE3 of the blob bytes (64 characters). |

**Headers**

| Header | Required | Value |
|---|---|---|
| `Authorization` | yes | `Bearer <PAT>` |
| `Content-Type` | recommended | `application/octet-stream` |
| `Content-Length` | recommended | byte length of body |

**Request**

```bash
DIGEST=$(b3sum ./output.tar.gz | awk '{print $1}')   # BLAKE3, not sha256

curl -s -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @./output.tar.gz \
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST"
```

**Response 201** — the body echoes the stored BLAKE3 hex:

```json
{"hash": "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"}
```

**Response 409** — blob already exists (idempotent; safe to ignore)

```json
{"error": "conflict", "message": "blob already exists"}
```

---

### `GET /v1/cas/<tenant_id>/<blake3>`

Download a blob by digest.

**Parameters**

| Name | In | Required | Description |
|---|---|---|---|
| `tenant_id` | path | yes | Your tenant identifier. |
| `blake3` | path | yes | Lowercase hex BLAKE3. |

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

### PAT management (`GET`/`POST /v1/pats`, `DELETE /v1/pats/:pat_id`) — planned, not yet live

These routes are specified for a future self-service PAT-management surface
(list, create, revoke) but are **not mounted today** — calling any of them
returns `404`. The only PAT-creation path that is live is the automatic
starter PAT issued by the sign-up wizard. An admin-only, read-only surface
exists for support to inspect a tenant's PATs
(`GET /v1/admin/tenants/{tenant_id}/pats`), but it requires an admin PAT and
is not something a regular customer token can call.

Until self-service PAT management ships, email
[support@humangr.com](mailto:support@humangr.com) to mint an additional PAT
or revoke one.

---

## Error codes

| HTTP status | `error` field | Meaning | Fix |
|---|---|---|---|
| `400 Bad Request` | `bad_request` | Malformed request (invalid JSON, missing field) | Check request body |
| `401 Unauthorized` | `unauthorized` | Missing or invalid PAT | Verify `Authorization` header |
| `403 Forbidden` | `forbidden` | PAT does not have the required scope, or tenant mismatch | Check PAT scopes and tenant in URL path |
| `404 Not Found` | `not_found` | Blob does not exist in the tenant's CAS | Push before pulling |
| `409 Conflict` | `conflict` | Blob already exists (PUT) | Idempotent — safe to ignore |
| `422 Unprocessable Entity` | `content hash mismatch` | BLAKE3 in URL does not match body (for example, you used `sha256sum`) | Recompute with `b3sum` |
| `429 Too Many Requests` | `rate_limited` | Request rate exceeded | Back off and retry; see `Retry-After` header |
| `503 Service Unavailable` | `audit_closed` | Tenant's audit period is closed — writes temporarily suspended | Contact support; reads still work |

All error responses share this shape:

```json
{
  "error": "not_found",
  "message": "blob af1349b9... not found in tenant acme-prod"
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

List endpoints (audit log, CAS listing) return cursor-based pagination:

```json
{
  "items": [...],
  "next_cursor": "eyJ...",
  "has_more": true
}
```

Pass `?cursor=<next_cursor>` to fetch the next page.
