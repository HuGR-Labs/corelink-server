/**
 * $-ceiling exemption invariant — a `_public` cache HIT served from the edge
 * MUST bypass the container, and therefore its per-op $-ceiling gate.
 *
 * # Why this test exists
 *
 * ADR `docs/design/2026-08-16-adr-edge-public-cache-invariants.md` Decision 2
 * records an OWNER product decision (2026-08-16): a shared, deduped,
 * content-addressed `_public` GET HIT is served from the Worker edge and is
 * deliberately EXEMPT from the Rust container's unconditional per-request
 * `$`-ceiling charge (`brew.rs:302-332` / `pip.rs:477-511`). Request-count +
 * storage quota still apply (`runQuotaBatch`), only the dollar/spend-ceiling is
 * waived, and only on the cheapest, most-shared traffic class.
 *
 * The ADR claimed this was "asserted by the edge serve tests" — but no such test
 * existed, so the invariant could silently regress into an accidental full
 * bypass (or, inversely, someone could route HITs back through the container and
 * nobody would notice the promise broke). THIS is that test.
 *
 * # The wire-level invariant
 *
 * The container is the ONLY place the $-ceiling is charged. In the Worker the
 * container is reached by exactly one call — `stub.fetch(...)` on the
 * `CORELINK_SERVER` Durable Object. So the invariant reduces to a call count the
 * test can see directly:
 *
 *   - EDGE HIT (flag on)  ⇒ served from the edge, container `stub.fetch` NOT
 *                            called ⇒ the $-ceiling is not charged.
 *   - EDGE MISS (flag on) ⇒ falls through to `stub.fetch` ⇒ container path, the
 *                            $-ceiling applies unchanged.
 *   - FLAG OFF            ⇒ even a would-be HIT goes to `stub.fetch` ⇒ the
 *                            rollback (unset `EDGE_PUBLIC_READ`) is exactly
 *                            today's container behaviour, ceiling and all.
 *
 * Counting `stub.fetch` is the faithful, non-arithmetic proof: the container
 * either ran (and charged) or it did not.
 */

import { describe, it, expect, beforeEach } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import { blake3HexBytes } from "../src/lib/blake3.js";
import { __resetTierCacheForTests } from "../src/lib/tenant_tier_cache.js";
import { __resetTenantResidencyCacheForTest } from "../src/lib/tenant_residency_cache.js";
import { __resetPublicMapCacheForTests } from "../src/lib/edge_public_read.js";

const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford-b32 chars
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const TEST_SECRET = "A".repeat(43); // 43 base64url chars
const SIGNING_KEY_HEX = "ab".repeat(32);
const INTERNAL_AUTH_KEY = "cd".repeat(32);
// A well-formed 32-byte tenant data key (64 hex); derivePublicPrefix HMACs it.
const R2_TDK_HEX = "11".repeat(32);
const R2_CAS_REGION = "iad";

/** The `_public` blob these tests serve, and its real blake3 content hash. */
const BLOB = new TextEncoder().encode("public-bottle-bytes-v1");
let CONTENT_HASH = ""; // computed in beforeEach from BLOB via the SAME hash fn

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

function b64urlNoPad(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

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
  return `corelink_pat_${TEST_TOKEN_ID}.${TEST_SECRET}.${b64urlNoPad(new Uint8Array(macBuf, 0, 16))}`;
}

function makeCtx(): ExecutionContext {
  return {
    waitUntil: () => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

/**
 * D1 that resolves the PAT + tier + request-count (so auth + quota pass), and —
 * when {@link mapHit} is true — answers the `adapter_cache_map` `_public` lookup
 * with {@link CONTENT_HASH}. When false, the map read returns null (edge MISS).
 */
function makeD1(mapHit: boolean): D1Database {
  const stmt = (sql: string) => ({
    bind: (...args: unknown[]) => ({
      __sql: sql,
      first: async <T>() => {
        if (sql.includes("adapter_cache_map")) {
          return (mapHit ? ({ content_hash: CONTENT_HASH } as T) : (null as T | null));
        }
        if (sql.includes("INSERT INTO monthly_request_counts")) {
          return { request_count: 1 } as T;
        }
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
        if (sql.includes("SELECT tier FROM tenant")) return { tier: "free" } as T;
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
  });
  return {
    prepare: stmt,
    batch: async (statements: Array<{ first: <T>() => Promise<T | null> }>) =>
      Promise.all(
        statements.map(async (s) => {
          const row = await s.first<unknown>();
          return { success: true as const, meta: {} as never, results: row === null ? [] : [row] };
        }),
      ),
    exec: async () => ({ count: 0, duration: 0 }),
    withSession() {
      return this;
    },
    dump: async () => new ArrayBuffer(0),
  } as unknown as D1Database;
}

/** R2 bucket that returns {@link BLOB} for any key (a HIT), or 404s if absent. */
function makeCasBucket(present: boolean): Env["CAS_BUCKET"] {
  return {
    get: async (_key: string) => {
      if (!present) return null;
      // Fresh copy each call so the returned buffer is never detached.
      const copy = BLOB.slice();
      return {
        arrayBuffer: async () => copy.buffer,
      } as unknown as R2ObjectBody;
    },
  } as unknown as Env["CAS_BUCKET"];
}

/**
 * Build an Env whose container hop (`CORELINK_SERVER` DO `stub.fetch`) is a spy.
 * `calls.n` is the number of times the container was reached — the $-ceiling is
 * charged iff the container ran, so `calls.n === 0` proves the exemption held.
 */
function makeEnv(opts: {
  flag: string | undefined;
  mapHit: boolean;
  blobPresent?: boolean;
}): { env: Env; calls: { n: number } } {
  const calls = { n: 0 };
  const stub = {
    fetch: async (): Promise<Response> => {
      calls.n += 1;
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
    CONFIG_DB: makeD1(opts.mapHit),
    PAT_SIGNING_KEY: SIGNING_KEY_HEX,
    CORELINK_INTERNAL_AUTH_KEY: INTERNAL_AUTH_KEY,
    CAS_BUCKET: makeCasBucket(opts.blobPresent ?? true),
    R2_TDK_HEX,
    R2_CAS_REGION,
    ...(opts.flag !== undefined ? { EDGE_PUBLIC_READ: opts.flag } : {}),
  } as unknown as Env;
  return { env, calls };
}

async function brewGet(env: Env, token: string): Promise<Response> {
  return workerHandler.fetch!(
    new Request(`http://localhost/brew/${TEST_TENANT_ID}/tree`, {
      method: "GET",
      headers: { Authorization: `Bearer ${token}` },
    }),
    env,
    makeCtx(),
  );
}

describe("$-ceiling exemption: edge `_public` HIT bypasses the container (ADR Decision 2)", () => {
  beforeEach(async () => {
    __resetTierCacheForTests();
    __resetTenantResidencyCacheForTest();
    __resetPublicMapCacheForTests();
    CONTENT_HASH = await blake3HexBytes(BLOB);
  });

  it("EDGE_PUBLIC_READ=serve + HIT ⇒ served from edge, container (and its $-ceiling) NOT reached", async () => {
    const { env, calls } = makeEnv({ flag: "serve", mapHit: true });
    const resp = await brewGet(env, await mintValidToken());

    expect(resp.status).toBe(200);
    expect(resp.headers.get("x-cache")).toBe("HIT");
    // THE invariant: the container is the only $-ceiling charge site, and it was
    // never reached. A regression that routed this HIT through the container
    // would make this non-zero and (correctly) fail.
    expect(
      calls.n,
      `container stub.fetch was called ${calls.n}× on an edge HIT — the $-ceiling ` +
        `exemption regressed (the public HIT is going through the container again)`,
    ).toBe(0);
    // The served bytes are the public blob, verbatim.
    expect(new Uint8Array(await resp.arrayBuffer())).toEqual(BLOB);
  });

  it("EDGE_PUBLIC_READ=serve + map MISS ⇒ falls through to the container (ceiling applies there)", async () => {
    const { env, calls } = makeEnv({ flag: "serve", mapHit: false });
    const resp = await brewGet(env, await mintValidToken());

    expect(resp.status).toBe(200);
    // A miss must reach the container exactly once — the edge never swallows a
    // request it cannot serve.
    expect(calls.n).toBe(1);
    expect(resp.headers.get("x-cache")).not.toBe("HIT");
  });

  it("EDGE_PUBLIC_READ=serve + blob absent in R2 ⇒ falls through to the container", async () => {
    // Map says HIT but the bytes are gone (e.g. erased between map read and R2
    // read): re-hash/absence ⇒ MISS ⇒ container, never a partial edge serve.
    const { env, calls } = makeEnv({ flag: "serve", mapHit: true, blobPresent: false });
    const resp = await brewGet(env, await mintValidToken());

    expect(resp.status).toBe(200);
    expect(calls.n).toBe(1);
    expect(resp.headers.get("x-cache")).not.toBe("HIT");
  });

  it("flag UNSET ⇒ even a would-be HIT goes through the container (rollback = today's ceiling path)", async () => {
    const { env, calls } = makeEnv({ flag: undefined, mapHit: true });
    const resp = await brewGet(env, await mintValidToken());

    expect(resp.status).toBe(200);
    // The rollback lever: with EDGE_PUBLIC_READ unset the edge branch is inert,
    // so the container serves and charges the $-ceiling exactly as before F3.3.
    expect(calls.n).toBe(1);
    expect(resp.headers.get("x-cache")).not.toBe("HIT");
  });
});
