# CoreLink JavaScript/TypeScript SDK (`@corelink/client`)

> Thin, dependency-light HTTP client for the wired CoreLink cache surface
> (CAS + Action Cache). BLAKE3 digests are computed and verified in **pure
> JavaScript** via [`@noble/hashes`](https://github.com/paulmillr/noble-hashes)
> — no WASM toolchain, no native addon.
> Client-verify is default-on per CTRL-CAS-002.
>
> Source: [`sdks/js/`](../../sdks/js). Ships ESM + `.d.ts` (TypeScript
> declarations), targets Node.js 18+, modern bundlers, and Cloudflare Workers.

## Installation

```sh
npm install @corelink/client
```

## Quick Start

```typescript
import { CoreLinkClient } from "@corelink/client";

const client = new CoreLinkClient({
  pat: process.env.CORELINK_PAT!, // or omit to read CORELINK_PAT from the env
  tenantId: "acme-corp",
  // clientVerify: true  // default per CTRL-CAS-002
});

// Put
const data = new TextEncoder().encode("hello world");
const digest = await client.put(data);
console.log(`Uploaded: blake3:${digest}`);

// Get (BLAKE3 client-verify default-on)
const downloaded = await client.get(digest);
console.log(`Downloaded: ${downloaded.length} bytes`);

// Stat
const stat = await client.stat(digest);
console.log(`size=${stat.sizeBytes} exists=${stat.exists}`);

await client.close();
```

## API Reference

### `new CoreLinkClient(config: ClientConfig)`

```typescript
interface ClientConfig {
  pat?: string;           // falls back to CORELINK_PAT env var
  tenantId: string;       // required
  baseUrl?: string;       // default: https://corelink-api.humangr.com
  clientVerify?: boolean; // default: true
  timeoutMs?: number;     // default: 30000
  retry?: Partial<RetryConfig>;
  fetch?: typeof fetch;   // inject a custom fetch (tests / custom runtime)
}
```

| Field | Default | Description |
|---|---|---|
| `pat` | `CORELINK_PAT` env | Personal Access Token. Required via arg or env. |
| `tenantId` | required | Tenant scope — the sole isolation key server-side. |
| `baseUrl` | `https://corelink-api.humangr.com` | Override for staging / local dev. |
| `clientVerify` | `true` | BLAKE3 verify after `get()`. Opt-out logs a warning. |
| `timeoutMs` | `30000` | Per-request timeout. |
| `retry` | `{ maxAttempts: 3, baseDelayMs: 200, maxDelayMs: 10000 }` | 429/503 + transport retry (exp backoff + jitter). |
| `fetch` | global `fetch` | Injectable `fetch` implementation. |

### `client.put(data: Uint8Array, opts?: PutOptions): Promise<string>`

Upload bytes; returns the 64-char BLAKE3 hex digest computed locally (the
address the blob lives at). Idempotent server-side.

```typescript
const digest = await client.put(buffer);
const same = await client.put(buffer, { expectedDigest: digest }); // asserts locally
```

### `client.get(digest: string, opts?: { verify?: boolean }): Promise<Uint8Array>`

Download a blob. Throws `DigestMismatchError` (`COR_CAS_DIGEST_MISMATCH`) if the
returned bytes fail the BLAKE3 verify.

```typescript
try {
  const data = await client.get("6b86b273ff34fc...");
} catch (err) {
  // DigestMismatchError.message contains COR_CAS_DIGEST_MISMATCH
}
```

### `client.stat(digest: string): Promise<StatResult>`

```typescript
interface StatResult {
  digest: string;
  sizeBytes: number;
  exists: boolean;
}
```

`stat` performs a `GET` (the container exposes no lighter single-object probe),
so `sizeBytes` is the exact byte length; a 404/410 resolves to
`{ exists: false, sizeBytes: 0 }`.

### `client.actionCache`

Action Cache (AC) sub-API over `/v1/ac/{tenant}/{action_digest}`. CoreLink
stores an **opaque `ActionResult` byte payload** keyed by a 64-hex action
digest — see [how-to: Action Cache](../../apps/docs/docs/how-to/sdk-js/04-action-cache.mdx).

- `actionCache.get(actionDigest): Promise<Uint8Array>` — miss throws `ActionCacheMiss`.
- `actionCache.put(actionDigest, result: Uint8Array): Promise<string>` — content-immutable; a divergent overwrite throws `ConflictError`.

### `client.close(): Promise<void>`

No-op today (the SDK uses the platform `fetch` and holds no persistent pool);
provided for lifecycle symmetry and forward-compatibility.

### `client._clientVerifyEnabled: boolean`

Test inspection: `expect(client._clientVerifyEnabled).toBe(true)`.

## Client-Verify Opt-Out

```typescript
const client = new CoreLinkClient({
  pat: process.env.CORELINK_PAT!,
  tenantId: "acme-corp",
  clientVerify: false,  // "DISABLE NOT RECOMMENDED" warning logged
});
expect(client._clientVerifyEnabled).toBe(false);
```

Per-call override: `await client.get(digest, { verify: false })`.

## TypeScript Declarations

The package ships `.d.ts` declarations (emitted by `tsc`). Full IDE intellisense
is available out of the box (VS Code, WebStorm). Exported types: `ClientConfig`,
`PutOptions`, `GetOptions`, `StatResult`, `RetryConfig`, `BlobDigest`,
`TenantId`, the full error hierarchy, and the `blake3Hex` / `isCanonicalDigest`
digest helpers.

## Error Reference

All errors extend `CoreLinkError`, each mapped to a wired HTTP status:

| Class | Status | Meaning |
|---|---|---|
| `AuthError` | 401 | PAT missing / revoked / wrong tenant. |
| `QuotaError` | 402 | Over storage cap or spend ceiling. |
| `ForbiddenError` | 403 | Cross-tenant or insufficient scope. |
| `NotFoundError` | 404 | Digest not in tenant CAS. |
| `ActionCacheMiss` | 404 | No `ActionResult` for the action (subclass of `NotFoundError`). |
| `ConflictError` | 409 | Immutable entry already exists. |
| `GoneError` | 410 | Artifact erased (GDPR/DSR); do not resurrect. |
| `DigestMismatchError` | 422 / verify fail | Bytes did not match the digest; discard them. |
| `RateLimitError` | 429 | Rate/concurrency limit (auto-retried). |
| `ServerError` | 5xx | Server or storage/audit backend unavailable. |
| `ConnectError` | — | Transport failure (no HTTP response). |

## Cross-Runtime Support

| Runtime | Status |
|---|---|
| Node.js 18–22 | Supported (uses global `fetch`; Node 18+ ships it). |
| Browser (ESM) | Supported (never embed a PAT — proxy via your backend). |
| Cloudflare Workers | Supported (global `fetch`). |
| Bundlers (Vite, webpack, esbuild) | Supported (ESM). |

> **ESM-by-default.** The `@corelink/client` package exports ESM only; CommonJS
> `require()` is not supported. All canonical examples use `import` syntax.

## Build from Source

```sh
cd sdks/js
npm install
npm test        # vitest — fetch is stubbed, no network
npm run build   # tsc -> dist/ (ESM + .d.ts)
npm run typecheck
```

## Design

Digests are a single pure-JS truth: `blake3` from `@noble/hashes` computes the
address on `put` and re-verifies it on `get`. No WASM build step and no native
dependency, so the package installs and runs anywhere the platform provides
`fetch`. See `specs/_decisions/ADR-0016-ffi-vs-native-http.md` for the
native-HTTP vs FFI decision context.
