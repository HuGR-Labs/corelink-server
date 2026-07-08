/**
 * cf-multitenant WP5a — Worker-side NARROWED runner-job PAT header forward.
 *
 * When a PAT's D1 row carries a non-NULL `runner_job_ac_key`, the Worker
 * forwards two server-trusted headers to the container so it (WP5b) can ENFORCE
 * the narrowing (deny-DELETE + optional exact-key):
 *
 *   - `x-corelink-runner-job: 1`
 *   - `x-corelink-ac-key-allow: <value>`  (value = "*" or a BLAKE3 hex)
 *
 * These tests assert (on the local-DO data-plane forward — the same site the
 * storage-quota cap header is injected at):
 *   1. a runner-job PAT ("*") → BOTH headers forwarded with the D1 value.
 *   2. a runner-job PAT (BLAKE3 hex) → x-corelink-ac-key-allow = that hex.
 *   3. a NORMAL PAT (runner_job_ac_key NULL) → NEITHER header set (no regression).
 *   4. a client-forged copy of either header NEVER survives (strip-then-set): on
 *      a normal PAT it is dropped entirely; on a runner-job PAT it is overwritten
 *      with the server-resolved D1 value.
 */

import { describe, it, expect } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford-b32 chars
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const TEST_SECRET = "A".repeat(43); // 43 base64url chars
const SIGNING_KEY_HEX = "ab".repeat(32); // 32-byte key (≥ the extractAuth floor)

// A representative BLAKE3 hex (blake3("clw/ref/runner/v1/build-out")) — the
// exact-key narrowing value; pinned in worker/tests/blake3.test.ts.
const AC_KEY_HEX = "bbd4ce224ed20e213837318117052855a885949a210ab760e1e075402e826adb";

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
 * D1 mock: resolves the test token_id → tenant, and returns the given
 * `runnerJobAcKey` (null = a normal PAT) as the pat.runner_job_ac_key column.
 * tenant.tier resolves to free (finite cap) so the request stays on the local
 * IAD DO path (the forward site under test).
 */
function makeD1(runnerJobAcKey: string | null): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          if (sql.includes("FROM pat") || sql.includes("token_id")) {
            if (args[0] === TEST_TOKEN_ID) {
              return {
                tenant_id: TEST_TENANT_ID,
                expires_ms: Date.now() + 3_600_000,
                scope: "cas:rw",
                runner_job_ac_key: runnerJobAcKey,
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
    withSession: () => null as never,
    dump: async () => new ArrayBuffer(0),
  } as unknown as D1Database;
}

function makeCtx(): ExecutionContext {
  return { waitUntil: () => {}, passThroughOnException: () => {} } as unknown as ExecutionContext;
}

function makeCapturingEnv(runnerJobAcKey: string | null): { env: Env; captured: () => Headers | null } {
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
    CONFIG_DB: makeD1(runnerJobAcKey),
    PAT_SIGNING_KEY: SIGNING_KEY_HEX,
    REQUEST_QUOTA_DISABLED: "true",
  } as unknown as Env;
  return { env, captured: () => capturedHeaders };
}

async function putCas(env: Env, key: string, extraHeaders: Record<string, string> = {}): Promise<Response> {
  const token = await mintValidToken();
  return workerHandler.fetch!(
    new Request(`http://localhost/v1/cas/${TEST_TENANT_ID}/${key}`, {
      method: "PUT",
      headers: { Authorization: `Bearer ${token}`, ...extraHeaders },
      body: "hello",
    }),
    env,
    makeCtx(),
  );
}

describe("WP5a — Worker forwards the narrowed runner-job headers", () => {
  it("runner-job PAT (\"*\") → forwards x-corelink-runner-job:1 + x-corelink-ac-key-allow:*", async () => {
    const { env, captured } = makeCapturingEnv("*");
    const resp = await putCas(env, "a".repeat(64));
    expect(resp.status).toBe(200);
    const h = captured();
    expect(h).not.toBeNull();
    expect(h!.get("x-corelink-runner-job")).toBe("1");
    expect(h!.get("x-corelink-ac-key-allow")).toBe("*");
    // anti AC-squat: every runner-job cred is create-only (deny-overwrite).
    expect(h!.get("x-corelink-ac-create-only")).toBe("1");
  });

  it("runner-job PAT (BLAKE3 hex) → x-corelink-ac-key-allow = that hex", async () => {
    const { env, captured } = makeCapturingEnv(AC_KEY_HEX);
    const resp = await putCas(env, "c".repeat(64));
    expect(resp.status).toBe(200);
    const h = captured();
    expect(h!.get("x-corelink-runner-job")).toBe("1");
    expect(h!.get("x-corelink-ac-key-allow")).toBe(AC_KEY_HEX);
  });

  it("NORMAL PAT (runner_job_ac_key NULL) → NEITHER header set (no regression)", async () => {
    const { env, captured } = makeCapturingEnv(null);
    const resp = await putCas(env, "d".repeat(64));
    expect(resp.status).toBe(200);
    const h = captured();
    expect(h!.get("x-corelink-runner-job")).toBeNull();
    expect(h!.get("x-corelink-ac-key-allow")).toBeNull();
    // A normal PAT is NOT create-only either (no regression).
    expect(h!.get("x-corelink-ac-create-only")).toBeNull();
  });

  it("STRIPS a client-forged runner-job header on a NORMAL PAT (never survives)", async () => {
    const { env, captured } = makeCapturingEnv(null);
    const resp = await putCas(env, "e".repeat(64), {
      // A malicious client tries to smuggle a forged narrowing (or a forged
      // widen — pointing enforcement at an attacker-chosen key), plus a forged
      // create-only marker.
      "x-corelink-runner-job": "1",
      "x-corelink-ac-key-allow": "attacker-controlled-key",
      "x-corelink-ac-create-only": "1",
    });
    expect(resp.status).toBe(200);
    const h = captured();
    // A normal PAT sets NONE of the runner-job headers, and the client's forged
    // copies were structurally stripped — so nothing reaches the container.
    expect(h!.get("x-corelink-runner-job")).toBeNull();
    expect(h!.get("x-corelink-ac-key-allow")).toBeNull();
    expect(h!.get("x-corelink-ac-create-only")).toBeNull();
  });

  it("OVERWRITES a client-forged ac-key-allow with the server D1 value on a runner-job PAT", async () => {
    const { env, captured } = makeCapturingEnv(AC_KEY_HEX);
    const resp = await putCas(env, "f".repeat(64), {
      "x-corelink-runner-job": "1",
      "x-corelink-ac-key-allow": "attacker-controlled-key",
    });
    expect(resp.status).toBe(200);
    const h = captured();
    // The forged value must NOT survive — the Worker overwrote it with the
    // D1-resolved narrowing value (the client can never redirect enforcement).
    expect(h!.get("x-corelink-ac-key-allow")).toBe(AC_KEY_HEX);
    expect(h!.get("x-corelink-ac-key-allow")).not.toBe("attacker-controlled-key");
  });
});
