/**
 * `first_cli_authed` producer — onboarding funnel telemetry (PLG §7.1).
 *
 * The customer `/welcome` pane can never light up while this signal has zero
 * producers. These tests pin the three properties the design freezes:
 *
 *   (a) DETERMINISTIC ID — `first_cli_authed:<tenant_id>` is byte-stable across
 *       two calls for the same tenant. That stability IS the dedup: the only
 *       uniqueness in `analytics_events` is `PRIMARY KEY (id)`
 *       (apps/analytics-worker/migrations/0001_create_analytics_events.sql:7)
 *       and ingest uses `INSERT OR IGNORE`
 *       (apps/analytics-worker/src/ingest.ts:207).
 *   (b) FAILURE ISOLATION — an ingest that throws, rejects, or returns 5xx
 *       cannot change the `/v1/users/me` status, headers, or body.
 *   (c) waitUntil, NOT inline — the emit promise is handed to `ctx.waitUntil`
 *       (a bare floating promise is CANCELLED on response return, bug #859 —
 *       worker/src/lib/tenant_suspend_gate.ts:115) and the emit helper returns
 *       `void`, so no call-site can await it onto the hot path.
 *
 * Plus the contract details read straight out of the ingest worker: the header
 * name, the batch body shape, the id-length cap, and the server-only-event rule
 * that makes the key mandatory.
 */

import { describe, it, expect } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import {
  ANALYTICS_EVENT_ID_MAX_LEN,
  ANALYTICS_INGEST_KEY_HEADER,
  ANALYTICS_INGEST_URL,
  FIRST_CLI_AUTHED_EVENT,
  emitFirstCliAuthed,
  firstCliAuthedEventId,
  type AnalyticsIngestEnv,
} from "../src/lib/onboarding_events.js";

// ── PAT fixture (same construction as tests/storage_quota_header.test.ts) ─────
const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford-b32 chars
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000042";
const TEST_SECRET = "A".repeat(43); // 43 base64url chars
const SIGNING_KEY_HEX = "ab".repeat(32); // 32-byte key (extractAuth floor)
const INGEST_KEY = "test-ingest-key";

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
  return `corelink_pat_${TEST_TOKEN_ID}.${TEST_SECRET}.${b64urlNoPad(new Uint8Array(macBuf, 0, 16))}`;
}

/** D1 mock: resolves the PAT → tenant, free tier, no residency row (IAD path). */
function makeD1(): D1Database {
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

/** ExecutionContext whose waitUntil records + settles every handed-off promise. */
function makeRecordingCtx(): { ctx: ExecutionContext; pending: Promise<unknown>[] } {
  const pending: Promise<unknown>[] = [];
  const ctx = {
    waitUntil: (p: Promise<unknown>) => { pending.push(p); },
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
  return { ctx, pending };
}

/**
 * Worker Env with a stub DO (constant 200) and a stub ANALYTICS_SVC whose
 * behaviour is caller-supplied, so a THROWING ingest can be simulated.
 */
function makeEnv(
  analyticsFetch?: (req: Request) => Promise<Response>,
): { env: Env; ingest: () => Request[] } {
  const seen: Request[] = [];
  const stub = {
    fetch: async (): Promise<Response> =>
      new Response(JSON.stringify({ ok: true, users_me: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
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
    CONFIG_DB: makeD1(),
    PAT_SIGNING_KEY: SIGNING_KEY_HEX,
    REQUEST_QUOTA_DISABLED: "true",
    ANALYTICS_INGEST_KEY: INGEST_KEY,
    ...(analyticsFetch === undefined
      ? {}
      : {
          ANALYTICS_SVC: {
            fetch: async (req: Request): Promise<Response> => {
              seen.push(req.clone());
              return analyticsFetch(req);
            },
          },
        }),
  } as unknown as Env;
  return { env, ingest: () => seen };
}

/**
 * Minimal `AnalyticsIngestEnv` (no Worker `Env`) whose service binding records
 * every request it is handed — for the direct unit-level guard tests.
 * `key: undefined` omits `ANALYTICS_INGEST_KEY` entirely.
 */
function makeSpyEnv(key?: string): { env: AnalyticsIngestEnv; calls: Request[] } {
  const calls: Request[] = [];
  const env: AnalyticsIngestEnv = {
    ...(key === undefined ? {} : { ANALYTICS_INGEST_KEY: key }),
    ANALYTICS_SVC: {
      fetch: (async (r: Request): Promise<Response> => {
        calls.push(r);
        return new Response("{}");
      }) as unknown as typeof fetch,
    },
  };
  return { env, calls };
}

const okIngest = async (): Promise<Response> =>
  new Response(JSON.stringify({ accepted: 1, rejected: 0 }), { status: 200 });

async function whoami(env: Env, ctx: ExecutionContext): Promise<Response> {
  const token = await mintValidToken();
  return workerHandler.fetch!(
    new Request("http://localhost/v1/users/me", {
      method: "GET",
      headers: { Authorization: `Bearer ${token}` },
    }),
    env,
    ctx,
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// (a) Deterministic id — the dedup mechanism
// ─────────────────────────────────────────────────────────────────────────────
describe("(a) deterministic event id", () => {
  it("is byte-identical across two calls for the same tenant", () => {
    const a = firstCliAuthedEventId(TEST_TENANT_ID);
    const b = firstCliAuthedEventId(TEST_TENANT_ID);
    expect(a).toBe(b);
    expect(a).toBe(`first_cli_authed:${TEST_TENANT_ID}`);
  });

  it("differs per tenant (the PK lock is per-tenant, not global)", () => {
    expect(firstCliAuthedEventId("t-a")).not.toBe(firstCliAuthedEventId("t-b"));
  });

  it("fits the ingest id cap of 64 chars for a UUID tenant", () => {
    // apps/analytics-worker/src/ingest.ts:112 — `id.length > 64` ⇒ invalid_id.
    expect(firstCliAuthedEventId(TEST_TENANT_ID).length).toBeLessThanOrEqual(
      ANALYTICS_EVENT_ID_MAX_LEN,
    );
  });

  it("skips the emit entirely when the id would exceed the 64-char cap", () => {
    const { env, calls } = makeSpyEnv(INGEST_KEY);
    emitFirstCliAuthed(env, "x".repeat(60));
    expect(calls).toHaveLength(0);
  });

  it("TWO whoami calls send the SAME id, so ingest's INSERT OR IGNORE coalesces", async () => {
    const { env, ingest } = makeEnv(okIngest);
    const { ctx, pending } = makeRecordingCtx();
    await whoami(env, ctx);
    await whoami(env, ctx);
    await Promise.all(pending);
    const bodies = await Promise.all(ingest().map(r => r.json() as Promise<{ events: { id: string }[] }>));
    expect(bodies).toHaveLength(2);
    expect(bodies[0]!.events[0]!.id).toBe(bodies[1]!.events[0]!.id);
    expect(bodies[0]!.events[0]!.id).toBe(`first_cli_authed:${TEST_TENANT_ID}`);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// (b) A failing / throwing ingest cannot affect the /v1/users/me response
// ─────────────────────────────────────────────────────────────────────────────
describe("(b) failure isolation — the emit cannot break the request", () => {
  const baseline = async (): Promise<{ status: number; body: string }> => {
    const { env } = makeEnv(okIngest);
    const { ctx, pending } = makeRecordingCtx();
    const resp = await whoami(env, ctx);
    await Promise.all(pending);
    return { status: resp.status, body: await resp.text() };
  };

  it("a THROWING ingest leaves the response byte-identical to the healthy one", async () => {
    const good = await baseline();
    const { env } = makeEnv(async () => { throw new Error("analytics exploded"); });
    const { ctx, pending } = makeRecordingCtx();
    const resp = await whoami(env, ctx);
    // The rejection must be absorbed inside the emit — settling the waitUntil
    // promises must NOT reject (an unhandled rejection would fail the isolate).
    await expect(Promise.all(pending)).resolves.toBeDefined();
    expect(resp.status).toBe(good.status);
    expect(await resp.text()).toBe(good.body);
  });

  it("a 500 from ingest leaves the response byte-identical", async () => {
    const good = await baseline();
    const { env } = makeEnv(async () => new Response("boom", { status: 500 }));
    const { ctx, pending } = makeRecordingCtx();
    const resp = await whoami(env, ctx);
    await Promise.all(pending);
    expect(resp.status).toBe(good.status);
    expect(await resp.text()).toBe(good.body);
  });

  it("an ingest that never settles does not hold up the response", async () => {
    const { env } = makeEnv(() => new Promise<Response>(() => { /* never resolves */ }));
    const { ctx } = makeRecordingCtx();
    // If the emit were awaited inline this would hang forever; the test's own
    // timeout is the assertion. `pending` is deliberately left un-awaited.
    const resp = await whoami(env, ctx);
    expect(resp.status).toBe(200);
  });

  it("no ANALYTICS_SVC binding at all ⇒ silent no-op, response unaffected", async () => {
    const good = await baseline();
    const { env, ingest } = makeEnv(); // binding omitted entirely
    const { ctx, pending } = makeRecordingCtx();
    const resp = await whoami(env, ctx);
    await Promise.all(pending);
    expect(ingest()).toHaveLength(0);
    expect(resp.status).toBe(good.status);
    expect(await resp.text()).toBe(good.body);
  });

  it("emitFirstCliAuthed never throws even when the binding itself throws synchronously", () => {
    const env = {
      ANALYTICS_INGEST_KEY: INGEST_KEY,
      ANALYTICS_SVC: {
        fetch: () => { throw new Error("sync boom"); },
      },
    } as unknown as Parameters<typeof emitFirstCliAuthed>[0];
    expect(() => emitFirstCliAuthed(env, TEST_TENANT_ID)).not.toThrow();
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// (c) Handed to waitUntil, never awaited inline
// ─────────────────────────────────────────────────────────────────────────────
describe("(c) fire-and-forget via ctx.waitUntil", () => {
  it("hands EXACTLY ONE ADDITIONAL promise to waitUntil on a /v1/users/me hit", async () => {
    // The route has other pre-existing waitUntil users (the PAT-verify cache /
    // tier / residency write-behinds), so the assertion is on the DELTA: with
    // the analytics binding bound, exactly one more promise is handed off than
    // without it — and that extra one is the emit (proved by the ingest call).
    const withoutBinding = makeEnv();
    const ctxA = makeRecordingCtx();
    await whoami(withoutBinding.env, ctxA.ctx);
    await Promise.all(ctxA.pending);

    const withBinding = makeEnv(okIngest);
    const ctxB = makeRecordingCtx();
    await whoami(withBinding.env, ctxB.ctx);

    expect(ctxB.pending.length).toBe(ctxA.pending.length + 1);
    expect(ctxB.pending[ctxB.pending.length - 1]).toBeInstanceOf(Promise);
    expect(withBinding.ingest()).toHaveLength(1);
    await Promise.all(ctxB.pending);
  });

  it("returns void — structurally impossible to await onto the hot path", () => {
    const { env } = makeEnv(okIngest);
    const ret = emitFirstCliAuthed(env as never, TEST_TENANT_ID, { waitUntil: () => {} });
    expect(ret).toBeUndefined();
  });

  it("the POST is in flight BEFORE the promise handed to waitUntil settles", async () => {
    let released!: () => void;
    const gate = new Promise<void>(res => { released = res; });
    const { env, ingest } = makeEnv(async () => {
      await gate;
      return new Response("{}");
    });
    const { ctx, pending } = makeRecordingCtx();
    const resp = await whoami(env, ctx);
    // Response has already returned while the ingest POST is still blocked on `gate`.
    expect(resp.status).toBe(200);
    expect(ingest()).toHaveLength(1);
    released();
    await Promise.all(pending);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Ingest contract — every value read out of apps/analytics-worker/src/ingest.ts
// ─────────────────────────────────────────────────────────────────────────────
describe("ingest contract", () => {
  it("POSTs to /v1/event with the trusted-key header and the batch envelope", async () => {
    const { env, ingest } = makeEnv(okIngest);
    const { ctx, pending } = makeRecordingCtx();
    await whoami(env, ctx);
    await Promise.all(pending);

    const req = ingest()[0]!;
    expect(req.method).toBe("POST");
    // Path is what matters; the service binding routes internally (never the
    // public edge — a Worker→Worker custom-domain fetch is 1014-rejected).
    expect(new URL(req.url).pathname).toBe("/v1/event");
    expect(req.url).toBe(ANALYTICS_INGEST_URL);
    // apps/analytics-worker/src/ingest.ts:160 — the trusted-server auth header.
    expect(req.headers.get(ANALYTICS_INGEST_KEY_HEADER)).toBe(INGEST_KEY);
    expect(req.headers.get("Content-Type")).toBe("application/json");

    const body = (await req.json()) as {
      events: Array<{
        id: string;
        event_name: string;
        tenant_id: string;
        properties: Record<string, unknown>;
        created_at?: string;
      }>;
    };
    // Batch envelope — ingest.ts:184. Batch of 1 ≤ trusted cap of 100 (ingest.ts:191).
    expect(Array.isArray(body.events)).toBe(true);
    expect(body.events).toHaveLength(1);
    const evt = body.events[0]!;
    expect(evt.event_name).toBe(FIRST_CLI_AUTHED_EVENT);
    expect(evt.tenant_id).toBe(TEST_TENANT_ID);
    expect(evt.id).toBe(`first_cli_authed:${TEST_TENANT_ID}`);
    // created_at omitted ⇒ the ingest worker stamps its own clock (ingest.ts:209).
    expect(evt.created_at).toBeUndefined();
    // Privacy gate (ingest.ts:134): properties may not carry email/ip.
    for (const forbidden of ["email", "ip", "ip_address", "remote_addr"]) {
      expect(forbidden in evt.properties).toBe(false);
    }
  });

  it("skips the emit when ANALYTICS_INGEST_KEY is unset (server-only event would 403)", () => {
    const { env, calls } = makeSpyEnv(); // binding present, key absent
    // `first_cli_authed` is on SERVER_ONLY_EVENT_NAMES (ingest.ts:51), so an
    // unkeyed POST is a guaranteed reject — do not burn a subrequest on it.
    emitFirstCliAuthed(env, TEST_TENANT_ID);
    expect(calls).toHaveLength(0);
  });

  it("never emits for the synthetic tenant sentinels", () => {
    const { env, calls } = makeSpyEnv(INGEST_KEY);
    for (const sentinel of ["_anonymous", "_system", "_pending", ""]) {
      emitFirstCliAuthed(env, sentinel);
    }
    expect(calls).toHaveLength(0);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Scope — only /v1/users/me, and only GET
// ─────────────────────────────────────────────────────────────────────────────
describe("emit scope", () => {
  it("does NOT fire on a different authenticated /v1/* route", async () => {
    const { env, ingest } = makeEnv(okIngest);
    const { ctx, pending } = makeRecordingCtx();
    const token = await mintValidToken();
    await workerHandler.fetch!(
      new Request(`http://localhost/v1/cas/${TEST_TENANT_ID}/${"a".repeat(64)}`, {
        method: "GET",
        headers: { Authorization: `Bearer ${token}` },
      }),
      env,
      ctx,
    );
    await Promise.all(pending);
    expect(ingest()).toHaveLength(0);
  });

  it("does NOT fire when the PAT is rejected (401 — no authed tenant)", async () => {
    const { env, ingest } = makeEnv(okIngest);
    const { ctx, pending } = makeRecordingCtx();
    const resp = await workerHandler.fetch!(
      new Request("http://localhost/v1/users/me", {
        method: "GET",
        headers: { Authorization: "Bearer corelink_pat_BBBBBBBBBBBBBBBB.x.y" },
      }),
      env,
      ctx,
    );
    await Promise.all(pending);
    expect(resp.status).toBe(401);
    expect(ingest()).toHaveLength(0);
  });
});
