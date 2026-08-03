/**
 * `first_cli_authed` producer — onboarding funnel telemetry (PLG §7.1).
 *
 * The customer `/welcome` pane can never light up while this signal has zero
 * producers. These tests pin the properties the design freezes:
 *
 *   (a) DETERMINISTIC ID — `first_cli_authed:<tenant_id>` is byte-stable across
 *       two calls for the same tenant. That stability IS the dedup: the only
 *       uniqueness in `analytics_events` is `PRIMARY KEY (id)`
 *       (apps/analytics-worker/migrations/0001_create_analytics_events.sql:7)
 *       and ingest uses `INSERT OR IGNORE`
 *       (apps/analytics-worker/src/ingest.ts:207).
 *   (b) FAILURE ISOLATION — an RPC that throws, rejects, or never settles
 *       cannot change the `/v1/users/me` status, headers, or body.
 *   (c) waitUntil, NOT inline — the emit promise is handed to `ctx.waitUntil`
 *       (a bare floating promise is CANCELLED on response return, bug #859 —
 *       worker/src/lib/tenant_suspend_gate.ts:115) and the emit helper returns
 *       `void`, so no call-site can await it onto the hot path.
 *   (d) TRANSPORT — the dispatch is the RPC method `ingestServerEvent` on the
 *       `AnalyticsIngest` entrypoint, and NEVER `svc.fetch`. A stub carrying
 *       only `fetch` (i.e. `entrypoint = "AnalyticsIngest"` missing from the
 *       `[[env.prod.services]]` block) must degrade to a silent no-op, not a
 *       `TypeError` on the customer's request path.
 *   (e) SCOPE — only `GET /v1/users/me`, only behind the PAT gate.
 *
 * There is no ingest KEY in any of this any more: a service binding is
 * authenticated by the platform and `ingestServerEvent` passes `trusted = true`
 * on that basis (apps/analytics-worker/src/index.ts:109-153). The absence of a
 * key is asserted below, so re-introducing one fails the suite.
 */

import { describe, it, expect } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import {
  ANALYTICS_EVENT_ID_MAX_LEN,
  ANALYTICS_TENANT_ID_MAX_LEN,
  FIRST_CLI_AUTHED_EVENT,
  emitFirstCliAuthed,
  firstCliAuthedEventId,
  type AnalyticsIngestEnv,
  type AnalyticsIngestResult,
  type AnalyticsServerEvent,
} from "../src/lib/onboarding_events.js";

// ── PAT fixture (same construction as tests/storage_quota_header.test.ts) ─────
const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford-b32 chars
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000042";
const TEST_SECRET = "A".repeat(43); // 43 base64url chars
const SIGNING_KEY_HEX = "ab".repeat(32); // 32-byte key (extractAuth floor)

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

const ACCEPTED: AnalyticsIngestResult = { accepted: 1, rejected: 0, errors: [] };

/** Default RPC behaviour: the analytics worker accepted the event. */
const okRpc = async (): Promise<AnalyticsIngestResult> => ACCEPTED;

/**
 * Worker Env with a stub DO (constant 200) and a stub ANALYTICS_SVC whose RPC
 * behaviour is caller-supplied, so a THROWING ingest can be simulated.
 *
 * The stub ALSO carries a `fetch` that fails the test on sight: the HTTP
 * transport is gone, and any regression back to it is caught here rather than
 * silently working.
 */
function makeEnv(
  rpc?: (event: AnalyticsServerEvent) => Promise<AnalyticsIngestResult>,
): { env: Env; events: () => AnalyticsServerEvent[]; fetchCalls: () => number } {
  const seen: AnalyticsServerEvent[] = [];
  let fetchCalls = 0;
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
    ...(rpc === undefined
      ? {}
      : {
          ANALYTICS_SVC: {
            ingestServerEvent: async (e: AnalyticsServerEvent): Promise<AnalyticsIngestResult> => {
              seen.push(e);
              return rpc(e);
            },
            // Must never be reached — the emitter is RPC-only now.
            fetch: async (): Promise<Response> => {
              fetchCalls += 1;
              return new Response("{}");
            },
          },
        }),
  } as unknown as Env;
  return { env, events: () => seen, fetchCalls: () => fetchCalls };
}

/**
 * Minimal `AnalyticsIngestEnv` (no Worker `Env`) whose service binding records
 * every event it is handed — for the direct unit-level guard tests.
 */
function makeSpyEnv(): { env: AnalyticsIngestEnv; calls: AnalyticsServerEvent[] } {
  const calls: AnalyticsServerEvent[] = [];
  const env: AnalyticsIngestEnv = {
    ANALYTICS_SVC: {
      ingestServerEvent: async (e: AnalyticsServerEvent): Promise<AnalyticsIngestResult> => {
        calls.push(e);
        return ACCEPTED;
      },
    },
  };
  return { env, calls };
}

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

/** Capture console.warn for the duration of `fn`. */
function captureWarn(fn: () => void): string[] {
  const warns: string[] = [];
  const original = console.warn;
  console.warn = (...args: unknown[]) => { warns.push(String(args[0])); };
  try {
    fn();
  } finally {
    console.warn = original;
  }
  return warns;
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
    const { env, calls } = makeSpyEnv();
    // 60 chars + "first_cli_authed:" (17) = 77 > 64.
    emitFirstCliAuthed(env, "x".repeat(60));
    expect(calls).toHaveLength(0);
  });

  it("TWO whoami calls send the SAME id, so ingest's INSERT OR IGNORE coalesces", async () => {
    const { env, events } = makeEnv(okRpc);
    const { ctx, pending } = makeRecordingCtx();
    await whoami(env, ctx);
    await whoami(env, ctx);
    await Promise.all(pending);
    expect(events()).toHaveLength(2);
    expect(events()[0]!.id).toBe(events()[1]!.id);
    expect(events()[0]!.id).toBe(`first_cli_authed:${TEST_TENANT_ID}`);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// (b) A failing / throwing RPC cannot affect the /v1/users/me response
// ─────────────────────────────────────────────────────────────────────────────
describe("(b) failure isolation — the emit cannot break the request", () => {
  const baseline = async (): Promise<{ status: number; body: string }> => {
    const { env } = makeEnv(okRpc);
    const { ctx, pending } = makeRecordingCtx();
    const resp = await whoami(env, ctx);
    await Promise.all(pending);
    return { status: resp.status, body: await resp.text() };
  };

  it("a THROWING RPC leaves the response byte-identical to the healthy one", async () => {
    const good = await baseline();
    const { env, events } = makeEnv(async () => { throw new Error("analytics exploded"); });
    const { ctx, pending } = makeRecordingCtx();
    const resp = await whoami(env, ctx);
    // The rejection must be absorbed inside the emit — settling the waitUntil
    // promises must NOT reject (an unhandled rejection would fail the isolate).
    await expect(Promise.all(pending)).resolves.toBeDefined();
    // The emit was genuinely ATTEMPTED (guards against a vacuous pass).
    expect(events()).toHaveLength(1);
    expect(resp.status).toBe(good.status);
    expect(await resp.text()).toBe(good.body);
  });

  it("an RPC stub whose method throws SYNCHRONOUSLY cannot surface to the caller", () => {
    const env = {
      ANALYTICS_SVC: {
        ingestServerEvent: () => { throw new Error("sync boom"); },
      },
    } as unknown as AnalyticsIngestEnv;
    expect(() => emitFirstCliAuthed(env, TEST_TENANT_ID)).not.toThrow();
  });

  it("a REJECTED event (accepted:0) leaves the response byte-identical", async () => {
    const good = await baseline();
    const { env, events } = makeEnv(async () => ({
      accepted: 0,
      rejected: 1,
      errors: [{ reason: "unknown_event_name" }],
    }));
    const { ctx, pending } = makeRecordingCtx();
    const resp = await whoami(env, ctx);
    await Promise.all(pending);
    expect(events()).toHaveLength(1);
    expect(resp.status).toBe(good.status);
    expect(await resp.text()).toBe(good.body);
  });

  it("an RPC that NEVER SETTLES does not hold up the response", async () => {
    const { env, events } = makeEnv(
      () => new Promise<AnalyticsIngestResult>(() => { /* never resolves */ }),
    );
    const { ctx } = makeRecordingCtx();
    // If the emit were awaited inline this would hang forever; the test's own
    // timeout is the assertion. `pending` is deliberately left un-awaited.
    const resp = await whoami(env, ctx);
    expect(resp.status).toBe(200);
    // …and the never-settling call really was dispatched.
    expect(events()).toHaveLength(1);
  });

  it("no ANALYTICS_SVC binding at all ⇒ silent no-op, response unaffected", async () => {
    const good = await baseline();
    const { env, events } = makeEnv(); // binding omitted entirely
    const { ctx, pending } = makeRecordingCtx();
    const resp = await whoami(env, ctx);
    await Promise.all(pending);
    expect(events()).toHaveLength(0);
    expect(resp.status).toBe(good.status);
    expect(await resp.text()).toBe(good.body);
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
    // without it — and that extra one is the emit (proved by the RPC call).
    const withoutBinding = makeEnv();
    const ctxA = makeRecordingCtx();
    await whoami(withoutBinding.env, ctxA.ctx);
    await Promise.all(ctxA.pending);

    const withBinding = makeEnv(okRpc);
    const ctxB = makeRecordingCtx();
    await whoami(withBinding.env, ctxB.ctx);

    expect(ctxB.pending.length).toBe(ctxA.pending.length + 1);
    expect(ctxB.pending[ctxB.pending.length - 1]).toBeInstanceOf(Promise);
    expect(withBinding.events()).toHaveLength(1);
    await Promise.all(ctxB.pending);
  });

  it("returns void — structurally impossible to await onto the hot path", () => {
    const { env } = makeEnv(okRpc);
    const ret = emitFirstCliAuthed(env as never, TEST_TENANT_ID, { waitUntil: () => {} });
    expect(ret).toBeUndefined();
  });

  it("the RPC is in flight BEFORE the promise handed to waitUntil settles", async () => {
    let released!: () => void;
    const gate = new Promise<void>(res => { released = res; });
    const { env, events } = makeEnv(async () => {
      await gate;
      return ACCEPTED;
    });
    const { ctx, pending } = makeRecordingCtx();
    const resp = await whoami(env, ctx);
    // Response has already returned while the ingest RPC is still blocked on `gate`.
    expect(resp.status).toBe(200);
    expect(events()).toHaveLength(1);
    released();
    await Promise.all(pending);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// (d) Transport — RPC entrypoint, never `fetch`, never a key
// ─────────────────────────────────────────────────────────────────────────────
describe("(d) transport is the AnalyticsIngest RPC entrypoint", () => {
  it("calls ingestServerEvent and NEVER svc.fetch", async () => {
    const { env, events, fetchCalls } = makeEnv(okRpc);
    const { ctx, pending } = makeRecordingCtx();
    await whoami(env, ctx);
    await Promise.all(pending);
    expect(events()).toHaveLength(1);
    expect(fetchCalls()).toBe(0);
  });

  it("sends exactly the EventPayload shape ingestServerEvent expects", async () => {
    const { env, events } = makeEnv(okRpc);
    const { ctx, pending } = makeRecordingCtx();
    await whoami(env, ctx);
    await Promise.all(pending);

    const evt = events()[0]!;
    // ONE event, NOT the `{ events: [...] }` batch envelope the HTTP route takes:
    // `ingestServerEvent(event)` is single-event (apps/analytics-worker/src/index.ts:170).
    expect(Array.isArray(evt)).toBe(false);
    expect((evt as { events?: unknown }).events).toBeUndefined();
    expect(evt.event_name).toBe(FIRST_CLI_AUTHED_EVENT);
    expect(evt.tenant_id).toBe(TEST_TENANT_ID);
    expect(evt.id).toBe(`first_cli_authed:${TEST_TENANT_ID}`);
    expect(evt.properties).toEqual({ surface: "cli" });
    // created_at omitted ⇒ the ingest worker stamps its own clock (ingest.ts:209).
    expect((evt as { created_at?: string }).created_at).toBeUndefined();
    // Privacy gate (ingest.ts:134): properties may not carry email/ip.
    for (const forbidden of ["email", "ip", "ip_address", "remote_addr"]) {
      expect(forbidden in (evt.properties ?? {})).toBe(false);
    }
  });

  it("carries NO ingest key — not in the event, not on the env interface", async () => {
    const { env, events } = makeEnv(okRpc);
    const { ctx, pending } = makeRecordingCtx();
    await whoami(env, ctx);
    await Promise.all(pending);
    // The RPC is trusted by construction; a key here would mean the HTTP-path
    // coupling came back. Assert on the payload AND on the module's exports.
    const evt = events()[0]! as Record<string, unknown>;
    for (const k of Object.keys(evt)) {
      expect(k.toLowerCase()).not.toContain("key");
    }
    const mod = await import("../src/lib/onboarding_events.js");
    expect(Object.keys(mod).some(k => k.includes("KEY"))).toBe(false);
    expect(Object.keys(mod).some(k => k.includes("URL"))).toBe(false);
  });

  it("a stub with ONLY fetch (entrypoint= missing) is a silent no-op, not a TypeError", () => {
    // This is the shape of a deploy where `[[env.prod.services]]` binds
    // ANALYTICS_SVC but omits `entrypoint = "AnalyticsIngest"`: the stub is the
    // target's DEFAULT fetch export and has no RPC methods at all.
    let fetched = 0;
    const env = {
      ANALYTICS_SVC: { fetch: async () => { fetched += 1; return new Response("{}"); } },
    } as unknown as AnalyticsIngestEnv;
    const warns = captureWarn(() => {
      expect(() => emitFirstCliAuthed(env, TEST_TENANT_ID)).not.toThrow();
    });
    expect(fetched).toBe(0);
    expect(warns).toHaveLength(0);
  });

  it("emits for a real tenant with no operator-set secret in the env at all", () => {
    // The whole point of WP-7: the env carries ONLY the binding. Under the old
    // key-gated emitter this env produced zero calls and one warning.
    const { env, calls } = makeSpyEnv();
    const warns = captureWarn(() => { emitFirstCliAuthed(env, TEST_TENANT_ID); });
    expect(calls).toHaveLength(1);
    expect(calls[0]!.id).toBe(`first_cli_authed:${TEST_TENANT_ID}`);
    expect(warns).toHaveLength(0);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Tenant guards
// ─────────────────────────────────────────────────────────────────────────────
describe("tenant guards", () => {
  it("never emits for the synthetic tenant sentinels", () => {
    const { env, calls } = makeSpyEnv();
    for (const sentinel of ["_anonymous", "_system", "_pending", ""]) {
      emitFirstCliAuthed(env, sentinel);
    }
    expect(calls).toHaveLength(0);
  });

  it("never emits for an over-long tenant id", () => {
    const { env, calls } = makeSpyEnv();
    emitFirstCliAuthed(env, "T".repeat(ANALYTICS_TENANT_ID_MAX_LEN + 1));
    expect(calls).toHaveLength(0);
  });

  it("every skip path stays SILENT — no binding, sentinel, oversized", () => {
    const { env } = makeSpyEnv();
    const warns = captureWarn(() => {
      emitFirstCliAuthed({} as AnalyticsIngestEnv, TEST_TENANT_ID);
      for (const sentinel of ["_anonymous", "_system", "_pending", ""]) {
        emitFirstCliAuthed(env, sentinel);
      }
      emitFirstCliAuthed(env, "T".repeat(ANALYTICS_TENANT_ID_MAX_LEN + 1));
    });
    expect(warns).toHaveLength(0);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// (e) Scope — only /v1/users/me, only GET, only behind the PAT gate
// ─────────────────────────────────────────────────────────────────────────────
describe("(e) emit scope", () => {
  it("does NOT fire on a different authenticated /v1/* route", async () => {
    const { env, events } = makeEnv(okRpc);
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
    expect(events()).toHaveLength(0);
  });

  it("does NOT fire on a NON-GET /v1/users/me", async () => {
    const { env, events } = makeEnv(okRpc);
    const { ctx, pending } = makeRecordingCtx();
    const token = await mintValidToken();
    await workerHandler.fetch!(
      new Request("http://localhost/v1/users/me", {
        method: "POST",
        headers: { Authorization: `Bearer ${token}` },
      }),
      env,
      ctx,
    );
    await Promise.all(pending);
    expect(events()).toHaveLength(0);
  });

  it("does NOT fire when the PAT is rejected (401 — no authed tenant)", async () => {
    const { env, events } = makeEnv(okRpc);
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
    expect(events()).toHaveLength(0);
  });

  it("does NOT fire with NO Authorization header at all", async () => {
    const { env, events } = makeEnv(okRpc);
    const { ctx, pending } = makeRecordingCtx();
    await workerHandler.fetch!(
      new Request("http://localhost/v1/users/me", { method: "GET" }),
      env,
      ctx,
    );
    await Promise.all(pending);
    expect(events()).toHaveLength(0);
  });
});
