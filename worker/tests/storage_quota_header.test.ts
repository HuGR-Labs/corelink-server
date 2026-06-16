/**
 * Storage-quota fail-open fix — Worker-side regression net.
 *
 * The container's byte-accounting reservation seeds a FRESH tenant_storage_state
 * row from a Worker-injected, server-trusted per-tier storage cap header
 * (`x-corelink-storage-quota-bytes`). These tests assert:
 *
 *   1. `storageQuotaHeaderValue` maps a resolved tier to the right cap string
 *      (finite → byte count; unlimited → "0"; D1-error → null/omit).
 *   2. The Worker INJECTS the cap header on the data-plane write-forward path
 *      (the value = the tenant's resolved tier cap).
 *   3. The header is in the client-trust strip-list: a CLIENT-forged value can
 *      NEVER survive — the Worker overwrites it with the server-resolved cap.
 */

import { describe, it, expect } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import {
  QUOTAS,
  storageQuotaHeaderValue,
  STORAGE_QUOTA_HEADER,
  type TierResult,
} from "../src/lib/quota.js";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

// Canonical test PAT: the D1 mock below resolves its token_id to a real tenant
// so the request flows through the PAT gate to the local DO forward — the site
// that injects the cap header. The native plane's sole possession gate is an
// HMAC-SHA256 over `<token_id>.<random_secret>` (16-byte truncated MAC) keyed by
// PAT_SIGNING_KEY, so a valid token must carry a matching signature; we mint one
// below with the same algorithm the Worker verifies with.
const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford-b32 chars
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const TEST_SECRET = "A".repeat(43); // 43 base64url chars
// 32-byte (64-hex) signing key — meets the ≥32-byte floor extractAuth requires.
const SIGNING_KEY_HEX = "ab".repeat(32);

/** Encode bytes to base64url-no-pad (matches the Worker's `base64url`). */
function b64urlNoPad(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** Decode a hex string to bytes. */
function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

/** Mint a canonical PAT whose HMAC sig verifies under SIGNING_KEY_HEX. */
async function mintValidToken(): Promise<string> {
  const preimage = new TextEncoder().encode(`${TEST_TOKEN_ID}.${TEST_SECRET}`);
  const key = await crypto.subtle.importKey(
    "raw",
    hexToBytes(SIGNING_KEY_HEX),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const macBuf = await crypto.subtle.sign("HMAC", key, preimage);
  const sig16 = new Uint8Array(macBuf, 0, 16); // 128-bit truncated MAC
  const sigSeg = b64urlNoPad(sig16); // 22 chars
  return `corelink_pat_${TEST_TOKEN_ID}.${TEST_SECRET}.${sigSeg}`;
}

/**
 * D1 mock: resolves the test token_id → tenant (PAT gate), returns the given
 * `tier` from the tenant.tier lookup, and null for tier_selections / residency
 * (so the tier resolves to `tier` with d1Error=false and the request stays on
 * the local IAD DO path).
 */
function makeTierD1(tier: string): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          // PAT lookup: arg[0] is the token_id.
          if (sql.includes("FROM pat") || sql.includes("token_id")) {
            if (args[0] === TEST_TOKEN_ID) {
              return {
                tenant_id: TEST_TENANT_ID,
                expires_ms: Date.now() + 3_600_000,
                scope: "cas:rw",
              } as T;
            }
            return null as T | null;
          }
          // tenant.tier lookup → the configured tier.
          if (sql.includes("FROM tenant WHERE") && sql.includes("tier")) {
            return { tier } as T;
          }
          // tier_selections (no active sub) + residency (no row) → null.
          return null as T | null;
        },
        all: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
        run: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
        raw: async <T>() => [] as T[],
      }),
      first: async <T>() => null as T | null,
      all: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
      run: async <T>() => ({ success: true as const, meta: {} as never, results: [] as T[] }),
      raw: async <T>() => [] as T[],
    }),
    batch: async () => [],
    exec: async () => ({ count: 0, duration: 0 }),
    withSession: () => null as never,
    dump: async () => new ArrayBuffer(0),
  } as unknown as D1Database;
}

function makeCtx(): ExecutionContext {
  return {
    waitUntil: () => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

/** Env whose DO stub captures the headers the Worker forwarded to the container. */
function makeCapturingEnv(tier: string): { env: Env; captured: () => Headers | null } {
  let capturedHeaders: Headers | null = null;
  const stub = {
    fetch: async (req: Request): Promise<Response> => {
      capturedHeaders = new Headers(req.headers);
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    },
  };
  const env = {
    CORELINK_SERVER: {
      idFromName: (_n: string) => ({ toString: () => "id" }),
      get: (_id: unknown) => stub,
      idFromString: (_s: string) => ({ toString: () => "id" }),
      newUniqueId: () => ({ toString: () => "unique-id" }),
      jurisdiction: (_j: string) => ({}) as unknown,
    } as unknown as Env["CORELINK_SERVER"],
    ENVIRONMENT: "test",
    CONFIG_DB: makeTierD1(tier),
    // The native-plane possession gate (extractAuth) requires a ≥32-byte key.
    PAT_SIGNING_KEY: SIGNING_KEY_HEX,
    // Disable the monthly request-count counter write in this unit test.
    REQUEST_QUOTA_DISABLED: "true",
  } as unknown as Env;
  return { env, captured: () => capturedHeaders };
}

describe("storageQuotaHeaderValue", () => {
  it("maps a finite tier to its byte-count cap string", () => {
    const free: TierResult = { tier: "free", d1Error: false };
    expect(storageQuotaHeaderValue(free)).toBe(String(QUOTAS.free.storageBytesMax));
    const pro: TierResult = { tier: "pro", d1Error: false };
    expect(storageQuotaHeaderValue(pro)).toBe(String(QUOTAS.pro.storageBytesMax));
  });

  it("maps a genuinely-unlimited tier (MAX_SAFE_INTEGER cap) to the '0' sentinel", () => {
    // Only `enterprise` is genuinely uncapped in QUOTAS; `team` carries a finite
    // 1 TiB cap, so it must map to its byte count, NOT the unlimited sentinel.
    const ent: TierResult = { tier: "enterprise", d1Error: false };
    expect(storageQuotaHeaderValue(ent)).toBe("0");
    const team: TierResult = { tier: "team", d1Error: false };
    expect(storageQuotaHeaderValue(team)).toBe(String(QUOTAS.team.storageBytesMax));
    expect(storageQuotaHeaderValue(team)).not.toBe("0");
  });

  it("returns null (omit header) when the tier was derived from a D1 error", () => {
    const errored: TierResult = { tier: "free", d1Error: true };
    expect(storageQuotaHeaderValue(errored)).toBeNull();
  });
});

describe("Worker injects the server-trusted storage-quota cap header", () => {
  it("forwards x-corelink-storage-quota-bytes = the resolved tier cap on a PUT write", async () => {
    const { env, captured } = makeCapturingEnv("free");
    const token = await mintValidToken();
    const resp = await workerHandler.fetch!(
      new Request(
        `http://localhost/v1/cas/${TEST_TENANT_ID}/` + "a".repeat(64),
        {
          method: "PUT",
          headers: { Authorization: `Bearer ${token}` },
          body: "hello",
        },
      ),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
    const h = captured();
    expect(h).not.toBeNull();
    expect(h!.get(STORAGE_QUOTA_HEADER)).toBe(String(QUOTAS.free.storageBytesMax));
  });

  it("STRIPS a client-forged cap header and overwrites it with the server cap", async () => {
    const { env, captured } = makeCapturingEnv("free");
    const token = await mintValidToken();
    const resp = await workerHandler.fetch!(
      new Request(
        `http://localhost/v1/cas/${TEST_TENANT_ID}/` + "b".repeat(64),
        {
          method: "PUT",
          headers: {
            Authorization: `Bearer ${token}`,
            // A malicious client tries to smuggle an unlimited ("0") cap.
            [STORAGE_QUOTA_HEADER]: "0",
          },
          body: "hello",
        },
      ),
      env,
      makeCtx(),
    );
    expect(resp.status).toBe(200);
    const h = captured();
    expect(h).not.toBeNull();
    // The forged "0" (unlimited) must NOT survive — the Worker overwrote it with
    // the real free-tier cap (the client can never widen its own quota).
    expect(h!.get(STORAGE_QUOTA_HEADER)).toBe(String(QUOTAS.free.storageBytesMax));
    expect(h!.get(STORAGE_QUOTA_HEADER)).not.toBe("0");
  });
});
