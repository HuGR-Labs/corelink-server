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

Most requests must carry an `Authorization: Bearer <PAT>` header. The
customer PAT-issuance POST is the browser exception: the dashboard sends an
edge-validated Clerk session cookie (`__session`), while CLI callers may use a
canonical PAT for compatibility. The Worker resolves the tenant from the
credential and strips the Clerk JWT before forwarding to the tenant Durable
Object; clients never provide a tenant id.

```bash
curl --silent --config - <<EOF
url = "https://corelink-api.humangr.com/v1/users/me"
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
```

No other authentication scheme (Basic, API key header, query param) is accepted.
For `POST /v1/pats` (and its dashboard alias `POST /v1/customer/keys`), use a
validated Clerk session cookie for browser traffic or a canonical PAT for CLI
traffic. Missing or malformed authentication is rejected with `401`.

## Endpoints

### `GET /v1/users/me`

Returns the identity of the PAT used in the request.

**Request**

```bash
curl --silent --config - <<EOF
url = "https://corelink-api.humangr.com/v1/users/me"
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
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
  -H "Content-Type: application/octet-stream" \
  --data-binary @./output.tar.gz \
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST" --config - <<EOF
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
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
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST" \
  -o ./output-downloaded.tar.gz --config - <<EOF
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
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

### PAT issuance (`POST /v1/pats`)

This live self-service route issues an additional tenant-scoped PAT. Browser
callers authenticate with a validated Clerk session cookie; CLI callers may
use a canonical PAT. The plaintext token is returned exactly once. The
dashboard-compatible alias `POST /v1/customer/keys` uses the same mint flow
and per-tenant `pat-issue` limiter (burst 10, then 10/hour; one token every
360 seconds), applied before mint and audit. Listing is
`GET /v1/customer/keys`; dashboard revocation is
`POST /v1/customer/keys/{pat_id}/revoke`. The public `GET /v1/pats` and
`DELETE /v1/pats/{pat_id}` operations remain planned, and are not aliases for
the dashboard route.

Error media types depend on the layer. A `401` rejected by the edge Worker
uses the JSON `{error, message, request_id}` envelope. A request that reaches
the customer handler uses `text/plain` for its `400/401/403/429/500/503`
responses. Invalid or ungrantable `admin`/`owner` scope requests are handler
`401`, not `422`; a read-only caller requesting a write credential is `403`.
Rate-limited responses include `Retry-After`.

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

REAPI and customer endpoints other than PAT issuance normally use this JSON
shape. PAT issuance is the documented layer exception described above: edge
errors are JSON, while handler errors are `text/plain`.

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
