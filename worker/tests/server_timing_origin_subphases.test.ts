/**
 * `origin` sub-phase attribution — does the Worker↔container split reconcile?
 *
 * # Why this test exists
 *
 * `origin` measured 281/300/329 ms of a 443/457/547 ms `total` on live prod
 * (2026-08-04, warm memo-hit steady state, n=30, one reused connection,
 * authenticated `/cargo` 404 miss) — 66 % of the request, in ONE opaque block
 * spanning the DO dispatch, the container's per-request D1 `pat` read, the
 * `$`-ceiling accrue, the storage lookup and everything else. Splitting `wdb`
 * the same way (#1036) is what showed its cost was two serial D1 round trips
 * and nothing else; guessing first would have optimised the wrong thing twice.
 *
 * The split spans a process boundary, which gives it a failure mode the `wdb`
 * split did not have: the container reports its own phases and the Worker
 * derives the rest by SUBTRACTION, so a report the Worker mis-reads does not
 * go red — it silently reattributes the container's milliseconds to the network
 * and reads perfectly plausible. These tests therefore assert the ARITHMETIC,
 * not the presence of labels:
 *
 *   - the sub-phases sum EXACTLY to `origin` (`RECONCILES`, below);
 *   - a delay charged to a container phase lands in `ohop` and nowhere else
 *     when the container does NOT report, and in the container phase when it
 *     does;
 *   - a report that cannot be reconciled is REFUSED whole and says so on the
 *     wire, rather than being published as a split that does not add up.
 *
 * They also lock the same emission contract the `wdb` sub-phases carry: a phase
 * that RAN is emitted INCLUDING at `dur=0`; only a phase that did not run is
 * omitted.
 */

import { describe, it, expect, beforeEach } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import workerHandler, { originSubPhases } from "../src/index.js";
import type { Env } from "../src/index.js";
import { __resetTierCacheForTests } from "../src/lib/tenant_tier_cache.js";
import { __resetTenantResidencyCacheForTest } from "../src/lib/tenant_residency_cache.js";

const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford-b32 chars
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const TEST_SECRET = "A".repeat(43); // 43 base64url chars
const SIGNING_KEY_HEX = "ab".repeat(32);
const INTERNAL_AUTH_KEY = "cd".repeat(32);

/** Injected DO-hop delay, well clear of scheduler jitter. */
const DELAY_MS = 150;
/** Lower bound the phase that should absorb {@link DELAY_MS} must clear. */
const DELAYED_MIN_MS = 110;

/** The container-reported members of the `origin` group, in emission order. */
const CONTAINER_PHASES = ["opat", "oquota", "ostore", "oother"] as const;
/** Every member of the `origin` group, including the Worker-derived residue. */
const ORIGIN_PHASES = ["ohop", ...CONTAINER_PHASES] as const;

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

const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

/** Minimal D1 that resolves the PAT + tier and nothing else — no injected delay. */
function makeD1(): D1Database {
  const stmt = (sql: string) => ({
    bind: (...args: unknown[]) => ({
      __sql: sql,
      first: async <T>() => {
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

function makeCtx(): ExecutionContext {
  return {
    waitUntil: () => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

/**
 * Env whose DO stub stands in for the real hop: it sleeps `hopDelayMs` (time the
 * container never sees — the hop) and answers with `containerTiming` as its
 * `Server-Timing`, exactly as the Rust `origin_timing_layer` would.
 */
function makeEnv(opts: { containerTiming?: string | null; hopDelayMs?: number } = {}): Env {
  const stub = {
    fetch: async (): Promise<Response> => {
      if (opts.hopDelayMs !== undefined) await sleep(opts.hopDelayMs);
      const headers: Record<string, string> = { "Content-Type": "application/json" };
      if (opts.containerTiming !== undefined && opts.containerTiming !== null) {
        headers["Server-Timing"] = opts.containerTiming;
      }
      return new Response(JSON.stringify({ ok: true }), { status: 200, headers });
    },
  };
  return {
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
    CORELINK_INTERNAL_AUTH_KEY: INTERNAL_AUTH_KEY,
  } as unknown as Env;
}

/** Parse a `Server-Timing` header into `{ metric: durationMs }`. */
function parseServerTiming(header: string | null): Record<string, number> {
  const out: Record<string, number> = {};
  if (header === null) return out;
  for (const part of header.split(",")) {
    const m = /^\s*([A-Za-z0-9_-]+)\s*;\s*dur=(\d+)/.exec(part);
    if (m !== null) out[m[1]!] = Number(m[2]);
  }
  return out;
}

async function requestOnce(env: Env, token: string, blobChar: string): Promise<Response> {
  const resp = await workerHandler.fetch!(
    new Request(`http://localhost/v1/cas/${TEST_TENANT_ID}/` + blobChar.repeat(64), {
      method: "PUT",
      headers: { Authorization: `Bearer ${token}` },
      body: "hello",
    }),
    env,
    makeCtx(),
  );
  expect(resp.status).toBe(200);
  return resp;
}

async function splitOnce(env: Env, token: string, blobChar: string): Promise<Record<string, number>> {
  const resp = await requestOnce(env, token, blobChar);
  return parseServerTiming(resp.headers.get("Server-Timing"));
}

describe("Server-Timing `origin` sub-phase attribution", () => {
  beforeEach(() => {
    __resetTierCacheForTests();
    __resetTenantResidencyCacheForTest();
  });

  it("emits every origin sub-phase and they sum EXACTLY to origin", async () => {
    // THE reconciliation assertion. A split that does not add up is worse than
    // no split: it invites a confident wrong conclusion about which tier to
    // attack, which is the entire failure this instrumentation exists to
    // prevent. Exact equality, not a band — the residue `ohop` is DERIVED by
    // subtraction, so the identity is arithmetic and any drift is a bug.
    // The stub sleeps DELAY_MS, so `origin` measures ~150 ms; the container's
    // claimed phases have to fit inside that or the merge (correctly) refuses
    // them — see the `unreconciled` case below.
    const env = makeEnv({
      containerTiming: "opat;dur=40, oquota;dur=50, ostore;dur=1, oother;dur=4",
      hopDelayMs: DELAY_MS,
    });
    const st = await splitOnce(env, await mintValidToken(), "a");

    for (const p of ORIGIN_PHASES) {
      expect(st, `${p} missing from Server-Timing. Full split: ${JSON.stringify(st)}`).toHaveProperty(
        p,
      );
    }
    const sum = ORIGIN_PHASES.reduce((a, p) => a + (st[p] ?? 0), 0);
    expect(
      sum,
      `the origin sub-phases sum to ${sum}ms but origin is ${st["origin"]}ms — the split ` +
        `does not account for the phase it decomposes. Full split: ${JSON.stringify(st)}`,
    ).toBe(st["origin"]);

    // The container's numbers must survive the merge verbatim; only `ohop` is
    // computed. A merge that rescaled or re-rounded them would make the two
    // sides of the boundary disagree about the same milliseconds.
    expect(st["opat"]).toBe(40);
    expect(st["oquota"]).toBe(50);
    expect(st["ostore"]).toBe(1);
    expect(st["oother"]).toBe(4);
  });

  it("charges the hop delay to `ohop` and to no container phase", async () => {
    // The container claims ~0 while the stub sleeps DELAY_MS before answering:
    // that time is, by construction, the hop. If it landed anywhere else the
    // residue would be wired to the wrong end of the boundary.
    const env = makeEnv({
      containerTiming: "opat;dur=0, oquota;dur=0, ostore;dur=0, oother;dur=1",
      hopDelayMs: DELAY_MS,
    });
    const st = await splitOnce(env, await mintValidToken(), "b");

    expect(
      st["ohop"],
      `ohop did not absorb the ${DELAY_MS}ms the container never saw — the residue is ` +
        `wired to the wrong end of the boundary. Full split: ${JSON.stringify(st)}`,
    ).toBeGreaterThanOrEqual(DELAYED_MIN_MS);
    for (const p of CONTAINER_PHASES) {
      expect(st[p]).toBeLessThanOrEqual(1);
    }
    const sum = ORIGIN_PHASES.reduce((a, p) => a + (st[p] ?? 0), 0);
    expect(sum).toBe(st["origin"]);
  });

  it("leaves `origin` undecomposed when the container reports nothing", async () => {
    // A container image older than this change — which is the NORMAL state
    // between the Worker deploy and the container repin — must degrade to
    // exactly the pre-change header, not to a fabricated split.
    const st = await splitOnce(makeEnv({ hopDelayMs: 20 }), await mintValidToken(), "c");
    expect(st).toHaveProperty("origin");
    for (const p of ORIGIN_PHASES) {
      expect(
        st,
        `${p} was emitted with no container report behind it — the Worker invented a ` +
          `split. Full split: ${JSON.stringify(st)}`,
      ).not.toHaveProperty(p);
    }
  });

  it("keeps the wdb split and the totals intact", async () => {
    // Guard against the origin split perturbing the accounting that already
    // reconciles: `total == auth + wdb + origin` held on 30/30 in prod and is
    // what makes the whole header trustworthy.
    const st = await splitOnce(
      makeEnv({ containerTiming: "opat;dur=0, oquota;dur=0, ostore;dur=0, oother;dur=0" }),
      await mintValidToken(),
      "d",
    );
    for (const p of ["auth", "wdb", "qtier", "qbatch", "qresid", "origin", "total"] as const) {
      expect(st, `${p} vanished from Server-Timing`).toHaveProperty(p);
    }
    expect(st["total"]).toBeGreaterThanOrEqual(st["auth"]! + st["wdb"]! + st["origin"]!);
  });

  describe("originSubPhases (the merge itself)", () => {
    it("parses a container header and derives the residue", () => {
      expect(originSubPhases(300, "opat;dur=97, oquota;dur=118, ostore;dur=1, oother;dur=4")).toEqual(
        ["ohop;dur=80", "opat;dur=97", "oquota;dur=118", "ostore;dur=1", "oother;dur=4"],
      );
    });

    it("emits a phase that ran at dur=0 and omits one that did not run", () => {
      // Same contract as the `wdb` sub-phases: `dur=0` means "ran, cheaper than
      // the clock resolves"; ABSENT means "did not run at all" (e.g. a route
      // that performs no PAT verify in the container). Collapsing the two is
      // what forced an inference from `auth` appearing on 3 of 30 responses.
      expect(originSubPhases(10, "oquota;dur=0, oother;dur=2")).toEqual([
        "ohop;dur=8",
        "oquota;dur=0",
        "oother;dur=2",
      ]);
    });

    it("refuses the split when the container's phases exceed origin", () => {
      // Clock skew across the boundary, or a stale/incoherent report. Publishing
      // it would need a negative `ohop`; publishing it clamped would break the
      // sum. Say so instead.
      const out = originSubPhases(50, "opat;dur=40, oquota;dur=30, oother;dur=5");
      expect(out).toEqual(['ohop;dur=50;desc="unreconciled"']);
    });

    it("refuses the split when a phase it consumes has an unreadable duration", () => {
      // The dangerous case: `ostore` is a name we ATTRIBUTE, so dropping it
      // quietly would move its milliseconds into `ohop` and blame the network
      // for the storage layer. The whole report is refused instead.
      const out = originSubPhases(300, "opat;dur=97, ostore;dur=abc, oother;dur=4");
      expect(out).toEqual(['ohop;dur=300;desc="unreconciled"']);
    });

    it("ignores metrics outside the origin group without refusing the split", () => {
      // A future container-side phase must not disarm the merge; it is simply
      // not one of ours, and its cost is already inside `oother`.
      expect(originSubPhases(20, "ofuture;dur=3, oquota;dur=5, oother;dur=5")).toEqual([
        "ohop;dur=10",
        "oquota;dur=5",
        "oother;dur=5",
      ]);
    });

    it("returns no split at all when there is no container header", () => {
      expect(originSubPhases(300, null)).toEqual([]);
      expect(originSubPhases(300, "cache;desc=miss")).toEqual([]);
    });

    it("reconciles for every well-formed report", () => {
      // Property-style sweep: whatever the container says, `ohop` + the phases
      // it reports must equal `origin` — or the split must be refused. There is
      // no third outcome, and this is the invariant a future edit would break.
      for (const originMs of [0, 1, 7, 300, 1000]) {
        for (const [pat, quota, store, other] of [
          [0, 0, 0, 0],
          [1, 2, 3, 4],
          [97, 118, 1, 4],
          [500, 400, 300, 200],
        ]) {
          const header = `opat;dur=${pat}, oquota;dur=${quota}, ostore;dur=${store}, oother;dur=${other}`;
          const out = originSubPhases(originMs, header);
          if (out.length === 1) {
            expect(out[0]).toBe(`ohop;dur=${originMs};desc="unreconciled"`);
            continue;
          }
          const sum = out.reduce((a, s) => a + Number(/dur=(\d+)/.exec(s)?.[1] ?? "0"), 0);
          expect(
            sum,
            `origin=${originMs} with container report "${header}" produced ${JSON.stringify(out)}, ` +
              `which sums to ${sum}`,
          ).toBe(originMs);
        }
      }
    });
  });
});
