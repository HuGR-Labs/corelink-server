/**
 * #11 — multi-region fan-out request-quota OVER-count regression net
 *        (FORGERY-SAFE: the fan-out marker is a server secret, not a presence bit).
 *
 * A regional Worker invocation that is itself an INTERNAL fan-out sub-request of
 * one logical client request must NOT re-meter the monthly request counter: the
 * primary Worker already counted it once. Metering it again double-charges
 * multi-region tenants (customer-unfavourable).
 *
 * SECURITY (tech-lead review of b5ba30c1): the public edge does NOT ingress-strip
 * `x-corelink-fanout-from`, so the original PRESENCE check let any client forge
 * the header to SKIP metering (a request-quota BYPASS, fail-OPEN). The marker is
 * now the shared server-to-server secret CORELINK_INTERNAL_AUTH_KEY, matched in
 * CONSTANT TIME — a forged value can never match, so it can never bypass metering.
 *
 * These tests assert the EXACT scope of the fix:
 *   1. A normal (non-fan-out) request DOES increment the monthly counter.
 *   2. A GENUINE fan-out request (x-corelink-fanout-from === the real server
 *      secret) does NOT increment it.
 *   3. A FORGED fan-out request (x-corelink-fanout-from set to a WRONG value, e.g.
 *      "prod") STILL increments the counter — proving the bypass is closed.
 *   4. Tier resolution + the server-trusted STORAGE_QUOTA_HEADER forwarding still
 *      happen for ALL of them (the metering gate must not touch the served path).
 */

import { describe, it, expect } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import {
  QUOTAS,
  STORAGE_QUOTA_HEADER,
} from "../src/lib/quota.js";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import { batchViaFirst } from "./d1_batch_mock.js";

const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford-b32 chars
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const TEST_SECRET = "A".repeat(43); // 43 base64url chars
const SIGNING_KEY_HEX = "ab".repeat(32); // 32-byte (64-hex) signing key
// Shared server-to-server fan-out secret (≥32 chars, mirrors `openssl rand -hex 32`).
// The GENUINE fan-out marker; a client cannot know it.
const INTERNAL_AUTH_KEY = "cd".repeat(32); // 64-hex chars (≥ MIN_INTERNAL_AUTH_KEY_LEN)

function b64urlNoPad(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
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
  const sig16 = new Uint8Array(macBuf, 0, 16);
  return `corelink_pat_${TEST_TOKEN_ID}.${TEST_SECRET}.${b64urlNoPad(sig16)}`;
}

/**
 * D1 mock: resolves the PAT, returns `tier` from tenant.tier, null elsewhere
 * (no active sub / no residency row → local IAD path), and increments a shared
 * counter every time the monthly_request_counts UPSERT runs so a test can assert
 * exactly-once / never metering.
 */
function makeTierD1(tier: string, counter: { increments: number }): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          if (sql.includes("INSERT INTO monthly_request_counts")) {
            counter.increments += 1;
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
          if (sql.includes("FROM tenant WHERE") && sql.includes("tier")) {
            return { tier } as T;
          }
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
    // Both quota statements travel as ONE `db.batch` round trip
    // (`runQuotaBatch`); resolve them through this mock's own routing.
    batch: batchViaFirst(),
    exec: async () => ({ count: 0, duration: 0 }),
    withSession() { return this; },
    dump: async () => new ArrayBuffer(0),
  } as unknown as D1Database;
}

function makeCtx(): ExecutionContext {
  return {
    waitUntil: () => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

/**
 * Env whose DO stub captures the forwarded container headers; metering is ENABLED
 * (REQUEST_QUOTA_DISABLED unset) so the counter mock observes real increments.
 */
function makeEnv(tier: string): {
  env: Env;
  captured: () => Headers | null;
  counter: { increments: number };
} {
  let capturedHeaders: Headers | null = null;
  const counter = { increments: 0 };
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
    CONFIG_DB: makeTierD1(tier, counter),
    PAT_SIGNING_KEY: SIGNING_KEY_HEX,
    // The shared server secret the fan-out marker must constant-time match.
    CORELINK_INTERNAL_AUTH_KEY: INTERNAL_AUTH_KEY,
    // Metering ENABLED: REQUEST_QUOTA_DISABLED intentionally unset.
  } as unknown as Env;
  return { env, captured: () => capturedHeaders, counter };
}

async function putOnce(
  env: Env,
  token: string,
  blobChar: string,
  extraHeaders: Record<string, string> = {},
): Promise<Response> {
  return workerHandler.fetch!(
    new Request(`http://localhost/v1/cas/${TEST_TENANT_ID}/` + blobChar.repeat(64), {
      method: "PUT",
      headers: { Authorization: `Bearer ${token}`, ...extraHeaders },
      body: "hello",
    }),
    env,
    makeCtx(),
  );
}

describe("#11 multi-region fan-out request-quota over-count", () => {
  it("a NORMAL request increments the monthly counter once and forwards the storage cap", async () => {
    const { env, captured, counter } = makeEnv("free");
    const token = await mintValidToken();

    const resp = await putOnce(env, token, "a");

    expect(resp.status).toBe(200);
    // Metered exactly once.
    expect(counter.increments).toBe(1);
    // Tier-resolve + storage header still happen.
    const h = captured();
    expect(h).not.toBeNull();
    expect(h!.get(STORAGE_QUOTA_HEADER)).toBe(String(QUOTAS.free.storageBytesMax));
  });

  it("a GENUINE fan-out request (x-corelink-fanout-from === the server secret) does NOT increment the counter but STILL forwards the storage cap", async () => {
    const { env, captured, counter } = makeEnv("free");
    const token = await mintValidToken();

    // The marker carries the REAL shared server secret — exactly what the primary
    // Worker sets over the service binding after stripping client trust headers.
    const resp = await putOnce(env, token, "b", {
      "x-corelink-fanout-from": INTERNAL_AUTH_KEY,
    });

    expect(resp.status).toBe(200);
    // The metering UPSERT must NOT run on a genuine fan-out sub-request.
    expect(counter.increments).toBe(0);
    // Tier-resolve + the server-trusted storage header are UNCONDITIONAL — a
    // fan-out sub-request still needs the resolved cap forwarded to its container.
    const h = captured();
    expect(h).not.toBeNull();
    expect(h!.get(STORAGE_QUOTA_HEADER)).toBe(String(QUOTAS.free.storageBytesMax));
  });

  it("a FORGED fan-out request (x-corelink-fanout-from set to a WRONG value) STILL increments the counter — NO bypass", async () => {
    const { env, captured, counter } = makeEnv("free");
    const token = await mintValidToken();

    // A client forges the header with a guessed/literal value (the old code's
    // marker). It does NOT equal the server secret, so the constant-time match
    // fails and the request MUST be metered like any normal request — proving the
    // request-quota bypass is closed (fail-SAFE, not fail-OPEN).
    const resp = await putOnce(env, token, "c", {
      "x-corelink-fanout-from": "prod",
    });

    expect(resp.status).toBe(200);
    // Forged marker → metered exactly once (no skip).
    expect(counter.increments).toBe(1);
    // Served path unaffected: storage cap still forwarded.
    const h = captured();
    expect(h).not.toBeNull();
    expect(h!.get(STORAGE_QUOTA_HEADER)).toBe(String(QUOTAS.free.storageBytesMax));
  });
});
