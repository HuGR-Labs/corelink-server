/**
 * WP-5 — REAL-workerd concurrency proof for the P3 metering Durable Objects.
 *
 * The unit tests (request_meter_*_do.test.ts) mock DurableObjectState with a
 * `blockConcurrencyWhile: fn => fn()` that runs synchronously — it MODELS the
 * single-writer but does not EXERCISE workerd's real input-gate serialization.
 * This suite loads the actual worker bundle into a real workerd (miniflare v4),
 * binds the two metering DOs, and hammers them with genuinely-concurrent fetches,
 * asserting the load-bearing invariants hold under real contention:
 *
 *   - SHARD: N concurrent debits against a balance of B serve EXACTLY B — never
 *     more (a non-serialized read-modify-write could double-spend a token).
 *   - COORDINATOR: concurrent refills from many regions never grant past the cap
 *     (consumed + Σoutstanding ≤ cap) — the over-serve=0 invariant, now proven
 *     against workerd's real blockConcurrencyWhile, not the synchronous mock.
 *
 * Harness mirrors do_miniflare_integration.miniflare.test.ts: always-rebuild the
 * bundle (a stale dist/ silently tests old behaviour), load by scriptPath.
 */

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { execSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

const WORKER_DIR = resolve(__dirname, "..");
const WORKTREE_ROOT = resolve(WORKER_DIR, "..");
const DIST_INDEX = resolve(WORKTREE_ROOT, "dist/index.js");

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let mf: any = null;

beforeAll(async () => {
  console.log("[wp5-conc] Building worker bundle via wrangler dry-run...");
  execSync(
    `${WORKER_DIR}/node_modules/.bin/wrangler deploy --dry-run --outdir dist --containers-rollout=none`,
    { cwd: WORKTREE_ROOT, stdio: "inherit" },
  );
  if (!existsSync(DIST_INDEX)) {
    throw new Error(`[wp5-conc] Worker bundle not found at ${DIST_INDEX}`);
  }

  const { Miniflare } = await import("miniflare");
  mf = new Miniflare({
    scriptPath: DIST_INDEX,
    modulesRoot: resolve(WORKTREE_ROOT, "dist"),
    modules: true,
    durableObjects: {
      // The two P3 metering DOs — exported from worker/src/index.ts.
      REQUEST_METER_COORDINATOR_DO: "RequestMeterCoordinatorDO",
      REQUEST_METER_SHARD_DO: "RequestMeterShardDO",
    },
    compatibilityDate: "2026-04-01",
    compatibilityFlags: ["nodejs_compat"],
    bindings: { ENVIRONMENT: "miniflare-test", PAGERDUTY_ROUTING_KEY: "" },
    d1Databases: { CONFIG_DB: "test-config-db" },
  });

  // The coordinator's SEED reads monthly_request_counts — create it (empty ⇒
  // seed reads 0, the fresh-month baseline these concurrency tests want).
  const d1 = await mf.getD1Database("CONFIG_DB");
  await d1.exec(
    "CREATE TABLE IF NOT EXISTS monthly_request_counts (" +
      "tenant_id TEXT NOT NULL, year_month TEXT NOT NULL, " +
      "request_count INTEGER NOT NULL DEFAULT 0, updated_at_ms INTEGER NOT NULL DEFAULT 0, " +
      "PRIMARY KEY (tenant_id, year_month))",
  );

  // Warm the runtime so the first concurrent burst isn't racing cold-start.
  const warm = await callDO("REQUEST_METER_SHARD_DO", "warm:iad", { op: "read" });
  expect(warm.status).toBe(200);
}, 90_000);

afterAll(async () => {
  if (mf !== null) {
    await mf.dispose();
    mf = null;
  }
});

/** POST one op to a DO instance addressed by (binding, idFromName). */
async function callDO(
  binding: string,
  name: string,
  op: Record<string, unknown>,
): Promise<Response> {
  const ns = await mf.getDurableObjectNamespace(binding);
  const stub = ns.get(ns.idFromName(name));
  return stub.fetch("https://do/meter", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(op),
  });
}
async function callJson<T>(binding: string, name: string, op: Record<string, unknown>): Promise<T> {
  const r = await callDO(binding, name, op);
  return (await r.json()) as T;
}

const YM = "2026-08";

describe("WP-5 workerd concurrency — P3 metering DOs", () => {
  it("SHARD: N concurrent debits against balance B serve EXACTLY B (no double-spend)", async () => {
    const name = "tenant-shard-1:iad";
    const B = 25;
    const N = 200; // far more concurrent debits than tokens

    await callJson(`REQUEST_METER_SHARD_DO`, name, { op: "applyRefill", yearMonth: YM, newBalance: B });

    // Fire N debits truly concurrently; lowWater 0 so none of them refill
    // (there is no coordinator hop here — a bare shard with a fixed balance).
    const results = await Promise.all(
      Array.from({ length: N }, () =>
        callJson<{ served: boolean }>(`REQUEST_METER_SHARD_DO`, name, {
          op: "debit",
          yearMonth: YM,
          lowWater: 0,
        }),
      ),
    );
    const served = results.filter((r) => r.served).length;
    expect(served).toBe(B); // real input-gate serialization ⇒ never > B

    const state = await callJson<{ state: { balance: number } }>(
      `REQUEST_METER_SHARD_DO`,
      name,
      { op: "read" },
    );
    expect(state.state.balance).toBe(0);
  });

  it("COORDINATOR: concurrent refills from many regions never grant past the cap", async () => {
    const tenant = "tenant-coord-1";
    const cap = 100;
    const block = 40;
    const regions = ["iad", "lhr", "nrt", "syd", "sam"];

    // Every region concurrently asks for a fresh block from EMPTY. With 5×40=200
    // demanded against cap 100, a non-serialized coordinator could over-grant;
    // real serialization must cap total granted at 100.
    const grants = await Promise.all(
      regions.map((region) =>
        callJson<{ granted: number }>(`REQUEST_METER_COORDINATOR_DO`, tenant, {
          op: "refill",
          cap,
          region,
          yearMonth: YM,
          spentDelta: 0,
          reportedBalance: 0,
          block,
          tenantId: tenant,
        }),
      ),
    );
    const totalGranted = grants.reduce((s, g) => s + g.granted, 0);
    expect(totalGranted).toBeLessThanOrEqual(cap);

    // Authoritative check: consumed + Σoutstanding ≤ cap after the concurrent burst.
    const read = await callJson<{
      state: { consumed: number; outstanding: Record<string, number> };
    }>(`REQUEST_METER_COORDINATOR_DO`, tenant, { op: "read" });
    const outstanding = Object.values(read.state.outstanding).reduce((s, b) => s + b, 0);
    expect(read.state.consumed + outstanding).toBeLessThanOrEqual(cap);
    // And it actually handed out the whole cap (no tokens stranded when demand ≥ cap).
    expect(read.state.consumed + outstanding).toBe(cap);
  });

  it("COORDINATOR: many concurrent single-token refills never exceed the cap", async () => {
    const tenant = "tenant-coord-2";
    const cap = 50;
    // 300 concurrent refills each asking for 1 token, one per pseudo-region, all
    // from empty. Total grantable is bounded by cap regardless of interleaving.
    const N = 300;
    const grants = await Promise.all(
      Array.from({ length: N }, (_, i) =>
        callJson<{ granted: number }>(`REQUEST_METER_COORDINATOR_DO`, tenant, {
          op: "refill",
          cap,
          region: `r${i}`,
          yearMonth: YM,
          spentDelta: 0,
          reportedBalance: 0,
          block: 1,
          tenantId: tenant,
        }),
      ),
    );
    const total = grants.reduce((s, g) => s + g.granted, 0);
    expect(total).toBeLessThanOrEqual(cap);
    expect(total).toBe(cap); // 300 distinct regions each holding ≤1 ⇒ exactly cap handed out
  });
});
