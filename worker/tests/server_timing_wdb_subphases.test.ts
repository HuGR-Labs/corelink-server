/**
 * `wdb` sub-phase attribution — does each Server-Timing label measure the
 * operation it claims to?
 *
 * # Why this test exists
 *
 * The aggregate `wdb` phase measured a 118 ms p50 on an authenticated `/cargo`
 * lookup in production (probe run 30916725902, 2026-08-04, 30 samples), against
 * a 311 ms `total` — so the Worker-side reads own roughly a third of the request
 * and the aggregate says nothing about WHICH read. Fixing the wrong one is the
 * default outcome of guessing.
 *
 * The split these tests protect ANSWERED that: measured warm against live prod
 * (n=30, single reused connection), `qmeter` 152/158/163 ms and `qstor`
 * 120/126/130 ms summed to the entire `wdb` of 277/284/302 ms, with `qtier` and
 * `qresid` at 0 — two serial uncached round trips to the ENAM D1 primary and
 * nothing else. Neither reads the other's result, so they now travel as ONE
 * `db.batch` and report as ONE phase, `qbatch`. `wdb` is therefore three awaits
 * today: the tier resolve, the batched round trip, and the residency resolve.
 * The header has to keep describing the round trips that actually happen — it
 * is the instrument the deploy is re-measured with.
 *
 * Instrumentation has a specific failure mode: it looks right while measuring
 * the wrong thing, and nothing goes red, because a plausible number is
 * indistinguishable from a correct one. So these tests do not assert that the
 * labels EXIST — they inject a delay into ONE D1 statement at a time and assert
 * that exactly the matching label absorbs it. A clock wired to the wrong await
 * fails here.
 *
 * They also lock the emission contract, which carries real information:
 *
 *   - a phase that RAN is always emitted, INCLUDING at `dur=0`
 *   - a phase that was SKIPPED is omitted entirely
 *
 * so `qtier;dur=0` means "cache-served, faster than the clock resolves" and a
 * missing `qbatch` means "no D1 round trip was issued at all". Collapsing those
 * two into "absent" is what forced an inference from the `auth` phase appearing
 * on only 3 of 30 responses in that same run: KV-served below the clock, not
 * skipped.
 */

import { describe, it, expect, beforeEach } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";
import { __resetTierCacheForTests } from "../src/lib/tenant_tier_cache.js";
import { __resetTenantResidencyCacheForTest } from "../src/lib/tenant_residency_cache.js";

const TEST_TOKEN_ID = "AAAAAAAAAAAAAAAA"; // 16 Crockford-b32 chars
const TEST_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const TEST_SECRET = "A".repeat(43); // 43 base64url chars
const SIGNING_KEY_HEX = "ab".repeat(32);
const INTERNAL_AUTH_KEY = "cd".repeat(32);

/**
 * Injected delay, in ms. Comfortably above scheduler jitter so the assertion
 * band below cannot be satisfied by noise, and small enough to keep the suite
 * fast.
 */
const DELAY_MS = 150;
/** Lower bound the delayed phase must clear (allows a little clock slack). */
const DELAYED_MIN_MS = 110;
/**
 * Upper bound every NON-delayed phase must stay under. Sized so a GC pause or a
 * loaded CI runner cannot push an un-delayed phase over it — the separation from
 * DELAYED_MIN_MS, not the absolute value, is what makes the attribution sharp.
 */
const UNDELAYED_MAX_MS = 50;
/**
 * How far the sub-phases may fall short of the `wdb` window they decompose.
 * Deliberately a SEPARATE constant from {@link UNDELAYED_MAX_MS}: they answer
 * different questions, and sharing one would mean tightening the per-phase
 * ceiling silently retunes the accounting check.
 *
 * The residual is the un-instrumented sync work between the awaits, which a
 * Worker's coarsened clock reads as 0 but Node does not — under a GC or
 * event-loop stall it accrues real wall time, so this cannot be tightened to
 * nothing without buying a flake. The cost is SENSITIVITY, and it is worth
 * naming: a further uninstrumented await added inside the window later is caught
 * only if it costs MORE than this. A 40 ms one passes silently.
 */
const SUM_RESIDUAL_MAX_MS = 40;

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

/**
 * Which single statement to slow down, so exactly one phase should absorb it.
 * `null` (deliberately NOT a member) means "delay nothing": the no-delay sentinel
 * must live OUTSIDE this union, because `phaseOf` returns `null` for a statement
 * it cannot classify. When the sentinel was a member of this union those two
 * meanings collided and asking for "no delay" delayed every unclassified
 * statement — including `SELECT tier FROM tier_selections` inside the `wdb`
 * window, so the supposedly-quiet env measured a full injected delay.
 */
type SlowTarget = "meter" | "tier" | "storage" | "residency";

/**
 * Classify a statement by the phase it belongs to. Deliberately matched on the
 * distinguishing fragment of each real query (see `lib/quota.ts` and
 * `lib/tenant_residency_cache.ts`) — the tier and residency reads BOTH select
 * `FROM tenant`, so the discriminator is the projected column, not the table.
 *
 * Note the tier resolve issues TWO statements — `SELECT tier FROM tier_selections`
 * (the canonical active-subscription read) and then `SELECT tier FROM tenant` —
 * and only the second is classified here. That is deliberate and harmless: the
 * delay still lands inside the `qtier` window either way, and matching one gives
 * a single, unambiguous injection point per phase.
 */
function phaseOf(sql: string): SlowTarget | null {
  if (sql.includes("INSERT INTO monthly_request_counts")) return "meter";
  if (sql.includes("SUM(bytes_used)")) return "storage";
  if (sql.includes("SELECT tier FROM tenant")) return "tier";
  if (sql.includes("SELECT primary_region FROM tenant")) return "residency";
  return null;
}

/** Every statement handed to `db.batch()`, in submission order, per call. */
type BatchLog = string[][];

/**
 * D1 mock that resolves the PAT and the tier, delays exactly one statement class
 * by {@link DELAY_MS}, and records the composition of every `db.batch()` call.
 *
 * `batch` resolves each statement through the SAME `first()` routing the serial
 * path uses (so the two cannot answer differently) and therefore also absorbs
 * the injected delay — which is the point: a delay injected into either BATCHED
 * statement must surface in the single `qbatch` phase.
 */
function makeD1(slow: SlowTarget | null, batchLog: BatchLog = []): D1Database {
  const stmt = (sql: string) => ({
    bind: (...args: unknown[]) => ({
      __sql: sql,
      first: async <T>() => {
        if (slow !== null && phaseOf(sql) === slow) await sleep(DELAY_MS);
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
        if (sql.includes("SELECT tier FROM tenant")) {
          return { tier: "free" } as T;
        }
        // Storage SUM and residency both resolve empty: 0 bytes used (within
        // cap) and no pinned region (the IAD-local fall-through).
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
    batch: async (statements: Array<{ __sql: string; first: <T>() => Promise<T | null> }>) => {
      batchLog.push(statements.map((s) => s.__sql));
      return Promise.all(
        statements.map(async (s) => {
          const row = await s.first<unknown>();
          return { success: true as const, meta: {} as never, results: row === null ? [] : [row] };
        }),
      );
    },
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

function makeEnv(slow: SlowTarget | null, batchLog: BatchLog = []): Env {
  const stub = {
    fetch: async (): Promise<Response> =>
      new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
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
    CONFIG_DB: makeD1(slow, batchLog),
    PAT_SIGNING_KEY: SIGNING_KEY_HEX,
    CORELINK_INTERNAL_AUTH_KEY: INTERNAL_AUTH_KEY,
    // METADATA_KV deliberately UNBOUND: with no L2, every L1 miss goes to D1,
    // which is what makes the injected per-statement delay observable.
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

async function requestOnce(
  env: Env,
  token: string,
  blobChar: string,
  extraHeaders: Record<string, string> = {},
): Promise<Record<string, number>> {
  const resp = await workerHandler.fetch!(
    new Request(`http://localhost/v1/cas/${TEST_TENANT_ID}/` + blobChar.repeat(64), {
      method: "PUT",
      headers: { Authorization: `Bearer ${token}`, ...extraHeaders },
      body: "hello",
    }),
    env,
    makeCtx(),
  );
  expect(resp.status).toBe(200);
  return parseServerTiming(resp.headers.get("Server-Timing"));
}

/** The `wdb` sub-phases, in the order they execute. */
const SUBPHASES = ["qtier", "qbatch", "qresid"] as const;

/** Phases the header must NOT carry any more: they no longer exist as round trips. */
const RETIRED_SUBPHASES = ["qmeter", "qstor"] as const;

describe("Server-Timing `wdb` sub-phase attribution", () => {
  beforeEach(() => {
    // Both caches are MODULE-level and would otherwise carry a warm entry for
    // TEST_TENANT_ID across cases, making the delayed statement unreachable and
    // the attribution assertions vacuously green.
    __resetTierCacheForTests();
    __resetTenantResidencyCacheForTest();
  });

  it("issues the metering UPSERT and the storage SUM as ONE batch, not two serial statements", async () => {
    // The whole point of the change: `wdb` was two serial uncached round trips
    // to the ENAM primary (measured 152+126 ms of a 284 ms phase), and neither
    // statement reads the other's result. This asserts the round-trip COUNT and
    // the batch's composition — not just that the labels look plausible.
    const batches: BatchLog = [];
    const st = await requestOnce(makeEnv(null, batches), await mintValidToken(), "h");

    expect(
      batches.length,
      `expected exactly ONE db.batch call carrying both quota statements, saw ${batches.length}. ` +
        `Batches: ${JSON.stringify(batches)}`,
    ).toBe(1);
    const [batch] = batches;
    expect(batch).toHaveLength(2);
    // Order is load-bearing only for result mapping, but pinning it makes a
    // silent re-ordering visible rather than merely surviving.
    expect(batch![0]).toContain("INSERT INTO monthly_request_counts");
    expect(batch![1]).toContain("SUM(bytes_used)");

    // ...and exactly one phase reports it.
    expect(st).toHaveProperty("qbatch");
    for (const p of RETIRED_SUBPHASES) {
      expect(
        st,
        `${p} is still emitted, but it is no longer a round trip of its own — the header ` +
          `would be describing a shape the code stopped having. Full split: ${JSON.stringify(st)}`,
      ).not.toHaveProperty(p);
    }
  });

  it("emits all three sub-phases on a normal authed request", async () => {
    const st = await requestOnce(makeEnv(null), await mintValidToken(), "a");
    for (const p of SUBPHASES) {
      expect(st, `${p} missing from Server-Timing`).toHaveProperty(p);
    }
    // The aggregate stays, unchanged, so existing probes keep working.
    expect(st).toHaveProperty("wdb");

    // Accounting with NOTHING injected: `wdb` and the sum are both near zero, so
    // an un-instrumented await inside the window shows up here at its own cost.
    // NOT more sensitively than the delayed cases — an injected delay cancels out
    // of `wdb - sum` and SUM_RESIDUAL_MAX_MS is the same constant either way. A
    // redundant net, kept because it is the only case with no delay in it at all.
    const sum = SUBPHASES.reduce((a, p) => a + (st[p] ?? 0), 0);
    expect(
      Math.abs(st["wdb"]! - sum),
      `wdb is ${st["wdb"]}ms but its three sub-phases sum to ${sum}ms with no delay ` +
        `injected — the window contains work nothing is measuring. Full split: ${JSON.stringify(st)}`,
    ).toBeLessThanOrEqual(SUM_RESIDUAL_MAX_MS);
  });

  it.each([
    ["meter", "qbatch"],
    ["tier", "qtier"],
    ["storage", "qbatch"],
    ["residency", "qresid"],
  ] as const)(
    "a delay injected into the %s statement is absorbed by %s and by no other phase",
    async (slow, expectedPhase) => {
      const st = await requestOnce(makeEnv(slow), await mintValidToken(), "b");

      expect(
        st[expectedPhase],
        `${expectedPhase} did not absorb the ${DELAY_MS}ms delay injected into the ${slow} ` +
          `statement — this clock is wired to the wrong await. Full split: ${JSON.stringify(st)}`,
      ).toBeGreaterThanOrEqual(DELAYED_MIN_MS);

      for (const p of SUBPHASES) {
        if (p === expectedPhase) continue;
        expect(
          st[p],
          `${p} also absorbed the delay meant for ${expectedPhase} — the sub-phase clocks ` +
            `overlap instead of partitioning ${'`wdb`'}. Full split: ${JSON.stringify(st)}`,
        ).toBeLessThan(UNDELAYED_MAX_MS);
      }

      // The aggregate must still contain the delay: a sub-phase that reports a
      // cost `wdb` does not also contain would mean the split is measuring
      // something outside the phase it claims to decompose.
      expect(st["wdb"]).toBeGreaterThanOrEqual(DELAYED_MIN_MS);

      // ...and the three must ACCOUNT for `wdb`, not merely sit inside it. Without
      // this, a clock that double-counts an await (or one that silently measures
      // nothing) still satisfies every assertion above. The three are the only
      // awaits in the window, so the unattributed remainder is pure CPU, which a
      // Worker's coarsened clock reads as ~0.
      const sum = SUBPHASES.reduce((a, p) => a + (st[p] ?? 0), 0);
      expect(
        sum,
        `the three sub-phases sum to ${sum}ms but wdb is ${st["wdb"]}ms — they do not ` +
          `account for the phase they decompose. Full split: ${JSON.stringify(st)}`,
      ).toBeGreaterThanOrEqual(st["wdb"]! - SUM_RESIDUAL_MAX_MS);
      expect(
        sum,
        `the three sub-phases sum to ${sum}ms, MORE than the ${st["wdb"]}ms wdb window ` +
          `that contains them — an await is being counted twice. Full split: ${JSON.stringify(st)}`,
      ).toBeLessThanOrEqual(st["wdb"]! + SUM_RESIDUAL_MAX_MS);
    },
  );

  it("emits the parent phases at dur=0 too, not just the sub-phases", async () => {
    // `auth`, `wdb` and `origin` were historically emitted on a strict `>`, so a
    // sub-millisecond phase vanished from the header — the exact "was it fast or
    // was it skipped?" ambiguity the sub-phase sentinel exists to remove. Fixing
    // it only for the children would leave the parent `wdb` capable of
    // disappearing while all of its own sub-phases report 0.
    const env = makeEnv(null);
    const token = await mintValidToken();
    await requestOnce(env, token, "f"); // warm the caches so the reads cost ~0
    const st = await requestOnce(env, token, "g");

    for (const p of ["auth", "wdb", "origin"] as const) {
      expect(
        st,
        `${p} vanished from Server-Timing on a fully cache-warm request. A phase that ` +
          `ran must be reported even at dur=0. Full split: ${JSON.stringify(st)}`,
      ).toHaveProperty(p);
    }
  });

  it("emits a phase that ran but cost nothing as dur=0 rather than omitting it", async () => {
    // Second request on the same isolate: the tier is L1-cached (5 s TTL), so
    // `resolveTenantTierCached` returns without any I/O and the clock reads 0.
    // The phase RAN, so it MUST still be emitted — otherwise "cache-served" and
    // "never executed" become the same observation on the wire.
    // Delay the tier read specifically, so a cache MISS costs DELAY_MS here.
    // Against `makeEnv(null)` an L1 hit and a real D1 read both measure ~0 in this
    // harness, and the bound below would hold no matter what the cache did —
    // decoration rather than a check.
    const env = makeEnv("tier");
    const token = await mintValidToken();
    await requestOnce(env, token, "c"); // cold: pays the injected tier delay
    const st = await requestOnce(env, token, "d"); // warm: L1 hit, no D1 read

    expect(
      st,
      "qtier vanished once the tier was cache-served — a 0ms phase must be reported, " +
        "not suppressed, or a fast phase is indistinguishable from a skipped one",
    ).toHaveProperty("qtier");
    // NOT `toBe(0)`. That was a strict equality on a wall-clock read with no band
    // at all, and it flaked 1-in-20 under CPU contention ("expected 1 to be +0")
    // in a PR-BLOCKING gate — a scheduler hiccup between `tierStart` and the
    // assignment is enough. The assertion that carries this case's meaning is the
    // `toHaveProperty` above (the phase is PRESENT, not suppressed); the value
    // check only needs to show it was cache-served rather than a real D1 read.
    expect(st["qtier"]).toBeLessThan(UNDELAYED_MAX_MS);
  });

  it("drops the metering statement from the batch on a genuine fan-out sub-request", async () => {
    // A fan-out sub-request carries the server secret and skips the UPSERT
    // (#11 — it was already metered by the primary Worker). The skip is now a
    // statement missing from the batch, not a phase missing from the header: the
    // round trip still happens for the storage read, so `qbatch` is still
    // reported. That is also what retires the confirmation oracle the old
    // `qmeter` omission handed a fan-out caller — the header no longer says
    // which statements were in the batch.
    const batches: BatchLog = [];
    const st = await requestOnce(makeEnv(null, batches), await mintValidToken(), "e", {
      "x-corelink-fanout-from": INTERNAL_AUTH_KEY,
    });

    expect(batches).toHaveLength(1);
    expect(
      batches[0]!.some((sql) => sql.includes("INSERT INTO monthly_request_counts")),
      "a genuine fan-out sub-request re-metered the monthly counter — #11 over-count regression",
    ).toBe(false);
    expect(batches[0]!.some((sql) => sql.includes("SUM(bytes_used)"))).toBe(true);

    // Every phase still reports, INCLUDING qbatch: a round trip happened.
    for (const p of SUBPHASES) {
      expect(
        st,
        `${p} vanished on a fan-out sub-request. Full split: ${JSON.stringify(st)}`,
      ).toHaveProperty(p);
    }
  });
});
