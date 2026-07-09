# `@corelink/client`

Official CoreLink JavaScript/TypeScript SDK — a thin, dependency-light client
over the wired CoreLink cache surface: content-addressable storage (**CAS**) and
the **Action Cache** (AC), authenticated with a Personal Access Token (PAT).

BLAKE3 digests are computed and verified in **pure JavaScript** via
[`@noble/hashes`](https://github.com/paulmillr/noble-hashes) — no native
addon, no WASM toolchain. Works on Node.js 18+, modern browsers/bundlers
(ESM), and Cloudflare Workers wherever the platform `fetch` is available.

## Install

```sh
npm install @corelink/client
```

## Quick start

```ts
import { CoreLinkClient } from "@corelink/client";

const client = new CoreLinkClient({
  pat: process.env.CORELINK_PAT!, // or omit to read CORELINK_PAT from the env
  tenantId: "acme-corp",
  // baseUrl: "https://corelink-api.humangr.com", // default
  // clientVerify: true,                          // default — BLAKE3-verify get()
});

// Put — returns the 64-char BLAKE3 hex digest the blob is addressed by.
const data = new TextEncoder().encode("hello world");
const digest = await client.put(data);

// Get — bytes are BLAKE3-verified against `digest` by default; a mismatch throws.
const bytes = await client.get(digest);

// Stat — existence + exact size.
const { exists, sizeBytes } = await client.stat(digest);

await client.close(); // no-op today; provided for lifecycle symmetry
```

## API

### `new CoreLinkClient(config)`

| field | default | notes |
|---|---|---|
| `pat` | `CORELINK_PAT` env | required (arg or env); the client refuses to construct without it |
| `tenantId` | — | required; the sole isolation key server-side |
| `baseUrl` | `https://corelink-api.humangr.com` | override for staging / local dev |
| `clientVerify` | `true` | BLAKE3-verify bytes after `get()`; disabling logs a warning |
| `timeoutMs` | `30000` | per-request timeout |
| `retry` | `{ maxAttempts: 3, baseDelayMs: 200, maxDelayMs: 10000 }` | 429/503 + transport errors, exp backoff + jitter |
| `fetch` | global `fetch` | inject a custom implementation (tests / custom runtime) |

### CAS

- `put(data: Uint8Array, opts?: { expectedDigest?: string }): Promise<string>`
  — uploads bytes to `PUT /v1/cas/{tenant}/{hash}`; computes the BLAKE3 digest
  locally and returns it. Idempotent server-side.
- `get(digest: string, opts?: { verify?: boolean }): Promise<Uint8Array>`
  — `GET /v1/cas/{tenant}/{hash}`; verifies the BLAKE3 root by default.
- `stat(digest: string): Promise<{ digest, exists, sizeBytes }>`.

### Action Cache — `client.actionCache`

CoreLink stores an **opaque `ActionResult` byte payload** keyed by a 64-hex
action digest (e.g. a serialized REAPI `ActionResult`). The SDK is honest about
that shape: it moves bytes; you own the encoding.

- `actionCache.get(actionDigest: string): Promise<Uint8Array>` — `GET /v1/ac/…`;
  a miss throws `ActionCacheMiss`.
- `actionCache.put(actionDigest: string, result: Uint8Array): Promise<string>`
  — `PUT /v1/ac/…`; content-immutable, a divergent overwrite throws
  `ConflictError` (409).

### Errors

All errors extend `CoreLinkError`, each mapped to a wired HTTP status:

| status | class |
|---|---|
| 401 | `AuthError` |
| 402 | `QuotaError` |
| 403 | `ForbiddenError` |
| 404 | `NotFoundError` / `ActionCacheMiss` |
| 409 | `ConflictError` |
| 410 | `GoneError` |
| 422 / client-verify fail | `DigestMismatchError` |
| 429 | `RateLimitError` (auto-retried) |
| 5xx | `ServerError` |
| transport failure | `ConnectError` |

### Digest helpers

`blake3Hex(data)`, `isCanonicalDigest(d)`, and `DIGEST_RE` are exported for
client-side dedup and validation.

## Security

Never embed a PAT in browser-shipped JavaScript. Proxy through your own backend
and forward short-lived, minimally-scoped tokens (see
[`how-to/sdk-js/01-authenticate`](../../apps/docs/docs/how-to/sdk-js/01-authenticate.mdx)).

## Develop

```sh
npm install
npm test        # vitest (fetch is stubbed; no network)
npm run build   # tsc -> dist/ (ESM + .d.ts)
```

Apache-2.0.
