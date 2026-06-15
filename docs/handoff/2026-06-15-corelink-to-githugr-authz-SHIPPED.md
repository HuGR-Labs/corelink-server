# SHIPPED → githugr TL — #3 tenant-lookup + #1 token-exchange (one PR)

> 2026-06-15 · from: CoreLink TL · re: your GREENLIGHT (`2026-06-15-GREENLIGHT-corelink-authz.md`)
> Both endpoints built as one PR, exactly to the ratified contract. Below is your
> "one ask back" — base URLs, the `X-Corelink-Internal-Auth` detail, and the
> `/introspect` 200 snippet — plus the exact request/response shapes so you can
> wire `githugr-live` / `clerk.rs` without a round-trip.

## Where they live (and why)

Both are **Worker** routes on the main `corelink-prod` Worker — NOT container
routes — because the audited Clerk JWT verifier (`lib/clerk_auth.ts`) and the D1
`CONFIG_DB` binding already live there. Each endpoint sits where its core
dependency is; `/introspect` stays container-side because it needs the Argon2id
`PatVerifier`. One audited verifier, one mint authority — no duplication.

## Base URL

- **Prod:** `https://corelink-api.humangr.com`
- **Staging:** there is no separate staging Worker today (workers_dev is disabled
  post-incident; see root `wrangler.toml`). Wire against **prod** — both endpoints
  are internal-auth gated and inert without the shared secret, so exposure is nil
  until you hold the secret. If you want an isolated staging Worker before cutover,
  say so and I'll stand one up.

## Auth: `X-Corelink-Internal-Auth`

- Both endpoints are gated by a **constant-time** compare of the
  `X-Corelink-Internal-Auth` request header against the **`CORELINK_INTERNAL_AUTH_KEY`**
  shared secret (same secret family as `/_internal/pat/mint` and the runners
  `/introspect` fabric secret; ≥32 chars or the endpoint fails CLOSED with 403).
- This is a **server-to-server** secret. Only githugr's **www server** holds it;
  the browser never sees it. Provisioning to you: I'll deliver the value
  out-of-band (it's the existing prod `CORELINK_INTERNAL_AUTH_KEY` — write-only in
  CF, so I hand it over directly, never in a repo). **Recommended:** put it behind
  a Worker-to-Worker **Service Binding** on your side rather than a public fetch
  (F4 hardening — same note as `/_internal/*`). Until you wire the binding, a
  direct HTTPS call with the header works.

---

## #3 — `POST /internal/v1/auth/tenant/lookup`

Resolve the shared Clerk `sub` → the CoreLink tenant it owns.

**Request**
```http
POST /internal/v1/auth/tenant/lookup
X-Corelink-Internal-Auth: <CORELINK_INTERNAL_AUTH_KEY>
Content-Type: application/json

{ "sub": "user_2abc..." }
```

**Response 200**
```json
{
  "tenant_id": "0c8f...-uuid",
  "role": "owner",
  "tier": "free",
  "tenant_state": "active"
}
```
- `role` is always `"owner"` (the clerk_user_id is the tenant owner, 1:1 at
  provision — migration 0056 UNIQUE INDEX). Persist `tenant_id` as the repo's
  `owner_tenant` (your #2 gate).
- **404 (fail-CLOSED)** `{ "error": "NOT_FOUND", ... }` when no tenant maps —
  you fail-closed on this, as you said.
- `400` if `sub` is absent. **Email fallback is N/A** (and moot): `tenant.email_hash`
  is `SHA-256(clerk_user_id)`, a privacy surrogate — there is no raw-email→tenant
  mapping. You always hold `sub` from the shared JWT, so the primary key is always
  available.
- `401` bad/missing internal-auth · `403` secret unbound · `405` non-POST · `500`
  D1 fault.

---

## #1 — `POST /internal/v1/auth/token-exchange` (RFC 8693)

Exchange `(user session JWT + audience)` for a short-TTL tenant-scoped PAT.

**Request**
```http
POST /internal/v1/auth/token-exchange
X-Corelink-Internal-Auth: <CORELINK_INTERNAL_AUTH_KEY>
Authorization: Bearer <Clerk __session JWT>
Content-Type: application/json

{ "audience": "<owner_tenant uuid>", "scope": "cas:rw" }
```
- `audience` **required** — the target tenant. `scope` optional (default `cas:rw`;
  `admin` is **refused** — least privilege).

**Response 200**
```json
{
  "token_plaintext": "corelink_pat_<id>.<secret>.<sig>",
  "pat_id": "uuid",
  "token_id": "16-char",
  "principal": "<opaque derived UUID>",
  "tenant": "<owner_tenant uuid>",
  "expires_ms": 1893456000000
}
```
- TTL ≈ **300s**. Cache the PAT per session until `expires_ms`; the browser never
  sees it. The Argon2id hash is never returned. `principal` is an opaque derived
  UUID, never the raw Clerk `sub`.
- **403 `{ "error": "FORBIDDEN", "message": "session tenant does not match audience" }`**
  when the session's tenant ≠ `audience`. **This is your cross-tenant-WRITE
  rejection** — a session for tenant A can never mint a PAT for tenant B.
- `400` audience absent / unsupported scope · `401` bad/missing internal-auth,
  missing/invalid session · `403` no tenant for session / secret unbound · `405`
  non-POST · `429` per-principal mint throttle · `500` upstream.

Triple-gated: internal-auth (your backend) **AND** a valid user session **AND**
audience match.

---

## Validate the minted PAT — `/introspect` 200 body (you asked)

`POST /internal/v1/auth/introspect` (container route; gated by the **dedicated**
`FABRIC_INTROSPECT_AUTH_KEY`, NOT the shared secret above) →

```json
{ "valid": true, "tenant_id": "<uuid>", "plan": "<tier>" }
```
- Invalid PAT → `200 { "valid": false }` (uniform, no oracle, no tenant_id).
- A `max_concurrency` integer appears only for tenants with a Runners entitlement
  (none today). Backend faults → 503 (fail-CLOSED).

## Status

- PR open on `corelink-server`: `feat/githugr-authz-tenant-lookup-token-exchange`
  (#3 + #1 + tests + CHANGELOG). Container `/introspect` already live in prod.
- Tell me when you've wired the www-server call and I'll confirm the secret is set
  + smoke the 403 cross-tenant path against prod with you.

— CoreLink TL
