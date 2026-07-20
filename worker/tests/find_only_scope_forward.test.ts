/**
 * ADR-0071 — Worker forwards `x-corelink-scope: find-missing` for a FIND-ONLY PAT.
 *
 * A find-only PAT (`pat.find_only = 1`, migration 0093) stores the CHECK-safe
 * base `scope = 'read-only'`, but the Worker — the SOLE `x-corelink-scope`
 * authority — narrows it to the literal `find-missing` so the container grants
 * ONLY `can_find_missing()` (existence probes), never CAS read/write. A NORMAL
 * PAT (`find_only` NULL/0) forwards its base scope unchanged (no regression).
 *
 * These tests capture the header the Worker forwards to the container (the same
 * local-DO forward site the runner-job / storage-quota headers use).
 */

import { describe, it, expect } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford-b32 chars
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const TEST_SECRET = "A".repeat(43);
const SIGNING_KEY_HEX = "ab".repeat(32);

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

/** D1 mock: token → tenant, with the given `scope` + `find_only` on the pat row. */
function makeD1(scope: string, findOnly: number | null): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          if (sql.includes("FROM pat") || sql.includes("token_id")) {
            if (args[0] === TEST_TOKEN_ID) {
              return {
                tenant_id: TEST_TENANT_ID,
                expires_ms: Date.now() + 3_600_000,
                scope,
                runner_job_ac_key: null,
                find_only: findOnly,
              } as T;
            }
            return null as T | null;
          }
          if (sql.includes("FROM tenant WHERE") && sql.includes("tier")) {
            return { tier: "free" } as T;
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
    batch: async () => [],
    exec: async () => ({ count: 0, duration: 0 }),
    withSession() { return this; },
    dump: async () => new ArrayBuffer(0),
  } as unknown as D1Database;
}

function makeCtx(): ExecutionContext {
  return { waitUntil: () => {}, passThroughOnException: () => {} } as unknown as ExecutionContext;
}

function makeCapturingEnv(scope: string, findOnly: number | null): { env: Env; captured: () => Headers | null } {
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
    CONFIG_DB: makeD1(scope, findOnly),
    PAT_SIGNING_KEY: SIGNING_KEY_HEX,
    REQUEST_QUOTA_DISABLED: "true",
  } as unknown as Env;
  return { env, captured: () => capturedHeaders };
}

async function getCas(env: Env): Promise<Response> {
  const token = await mintValidToken();
  return workerHandler.fetch!(
    new Request(`http://localhost/v1/cas/${TEST_TENANT_ID}/${"a".repeat(64)}`, {
      method: "GET",
      headers: { Authorization: `Bearer ${token}` },
    }),
    env,
    makeCtx(),
  );
}

describe("ADR-0071 — Worker narrows a find-only PAT to x-corelink-scope: find-missing", () => {
  it("find_only=1 → forwards x-corelink-scope: find-missing (NOT the base read-only)", async () => {
    const { env, captured } = makeCapturingEnv("read-only", 1);
    await getCas(env);
    const h = captured();
    expect(h).not.toBeNull();
    expect(h!.get("x-corelink-scope")).toBe("find-missing");
  });

  it("NORMAL PAT (find_only NULL) → forwards the base scope unchanged (no regression)", async () => {
    const { env, captured } = makeCapturingEnv("read-write", null);
    await getCas(env);
    expect(captured()!.get("x-corelink-scope")).toBe("read-write");
  });

  it("find_only=0 (explicit non-find) → forwards the base scope unchanged", async () => {
    const { env, captured } = makeCapturingEnv("read-only", 0);
    await getCas(env);
    expect(captured()!.get("x-corelink-scope")).toBe("read-only");
  });
});
