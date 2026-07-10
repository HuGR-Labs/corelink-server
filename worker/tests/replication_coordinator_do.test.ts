/**
 * Tests for worker/src/replication_coordinator_do.ts — the production
 * ReplicationCoordinatorDO (WI-MULTI-REGION-V1).
 *
 * Two layers:
 *   1. PURE decision core (ported from crates/corelink-replication-coordinator):
 *      lag/heartbeat SLO gates, evaluate, promote (split-brain + anti-flap),
 *      failback (24 h cool-down), status.
 *   2. The DO shell over a real in-memory storage mock (mirrors
 *      event_log_do.test.ts): register/heartbeat/status/promote, the alarm
 *      scheduled-driver + self-arm, persistence across a simulated restart, and
 *      audit-BEFORE-mutation fail-CLOSED.
 */

import { describe, it, expect } from "vitest";
import {
  ReplicationCoordinatorDO,
  HOT_STANDBY_COOLDOWN_SECONDS,
  HEARTBEAT_STALE_SECONDS,
  REPLICATION_TICK_INTERVAL_MS,
  lagWithinSlo,
  heartbeatFresh,
  evaluateDecision,
  planPromote,
  planFailback,
  buildStatus,
  primaryIn,
  type RoleMap,
  type HeartbeatMap,
  type LagBundle,
} from "../src/replication_coordinator_do.js";
import type { Env } from "../src/index.js";

// ──────────────────────────────────────────────────────────────────────────────
// Fixtures
// ──────────────────────────────────────────────────────────────────────────────

const zeroLag: LagBundle = { r2_s: 0, d1_s: 0, kv_s: 0, neon_s: 0 };

function makeBackedState(store: Map<string, unknown>, alarmRef: { at: number | null }): DurableObjectState {
  const storage = {
    get: async (key: string) => store.get(key),
    put: async (key: string, value: unknown) => {
      store.set(key, value);
    },
    delete: async (key: string) => store.delete(key),
    deleteAll: async () => store.clear(),
    list: async () => new Map(store),
    getAlarm: async () => alarmRef.at,
    setAlarm: async (at: number) => {
      alarmRef.at = at;
    },
    deleteAlarm: async () => {
      alarmRef.at = null;
    },
    transaction: async (fn: (t: DurableObjectTransaction) => Promise<void>) => fn({} as DurableObjectTransaction),
  } as unknown as DurableObjectStorage;

  return {
    id: {
      toString: () => "singleton",
      name: "singleton",
      equals: () => true,
    } as unknown as DurableObjectId,
    storage,
    blockConcurrencyWhile: async <T>(fn: () => Promise<T>) => fn(),
    waitUntil: () => {},
  } as unknown as DurableObjectState;
}

function makeEnv(): Env {
  return { CORELINK_SERVER: {} as DurableObjectNamespace, ENVIRONMENT: "test" } as unknown as Env;
}

function makeDO(): { ddo: ReplicationCoordinatorDO; store: Map<string, unknown>; alarm: { at: number | null } } {
  const store = new Map<string, unknown>();
  const alarm = { at: null as number | null };
  const ddo = new ReplicationCoordinatorDO(makeBackedState(store, alarm), makeEnv());
  return { ddo, store, alarm };
}

function req(path: string, method = "GET", body?: unknown): Request {
  return new Request(`http://do${path}`, {
    method,
    body: body === undefined ? undefined : JSON.stringify(body),
    headers: { "x-request-id": "test-req" },
  });
}

// ──────────────────────────────────────────────────────────────────────────────
// Pure core
// ──────────────────────────────────────────────────────────────────────────────

describe("pure SLO gates", () => {
  it("lagWithinSlo: r2/d1/kv gated, neon informational", () => {
    expect(lagWithinSlo(zeroLag)).toBe(true);
    expect(lagWithinSlo({ r2_s: 61, d1_s: 0, kv_s: 0, neon_s: 0 })).toBe(false);
    expect(lagWithinSlo({ r2_s: 0, d1_s: 61, kv_s: 0, neon_s: 0 })).toBe(false);
    expect(lagWithinSlo({ r2_s: 0, d1_s: 0, kv_s: 61, neon_s: 0 })).toBe(false);
    // Neon over its soft ceiling does NOT breach promotion-readiness SLO.
    expect(lagWithinSlo({ r2_s: 0, d1_s: 0, kv_s: 0, neon_s: 9999 })).toBe(true);
  });

  it("heartbeatFresh: age threshold + future clock-skew guard", () => {
    const now = 1_000_000;
    expect(heartbeatFresh({ ts_ms: now, lag: zeroLag }, now)).toBe(true);
    expect(heartbeatFresh({ ts_ms: now - (HEARTBEAT_STALE_SECONDS - 1) * 1000, lag: zeroLag }, now)).toBe(true);
    expect(heartbeatFresh({ ts_ms: now - HEARTBEAT_STALE_SECONDS * 1000, lag: zeroLag }, now)).toBe(false);
    // Future heartbeat → stale.
    expect(heartbeatFresh({ ts_ms: now + 5000, lag: zeroLag }, now)).toBe(false);
  });
});

describe("evaluateDecision", () => {
  const roles: RoleMap = {
    wnam: { role: "primary", cooldown_started_ms: null },
    enam: { role: "replica", cooldown_started_ms: null },
  };

  it("keeps a healthy primary", () => {
    const hbs: HeartbeatMap = { wnam: { ts_ms: 1_000_000, lag: zeroLag } };
    expect(evaluateDecision(roles, hbs, "wnam", 1_001_000)).toEqual({ kind: "keep_primary" });
  });

  it("promotes a healthy replica when the primary is stale", () => {
    const now = 1_000_001_000;
    const hbs: HeartbeatMap = {
      wnam: { ts_ms: 0, lag: zeroLag },
      enam: { ts_ms: 1_000_000_000, lag: zeroLag },
    };
    expect(evaluateDecision(roles, hbs, "wnam", now)).toEqual({ kind: "promote_replica", replica: "enam" });
  });

  it("returns no_eligible_replica when every region breaches SLO", () => {
    const hbs: HeartbeatMap = { wnam: { ts_ms: 0, lag: zeroLag }, enam: { ts_ms: 0, lag: zeroLag } };
    expect(evaluateDecision(roles, hbs, "wnam", 999_999_999)).toEqual({ kind: "no_eligible_replica" });
  });
});

describe("planPromote", () => {
  const base: RoleMap = {
    wnam: { role: "primary", cooldown_started_ms: null },
    enam: { role: "replica", cooldown_started_ms: null },
  };

  it("emits demote+promote audits and flips roles", () => {
    const now = 1_000_001_000;
    const hbs: HeartbeatMap = { wnam: { ts_ms: 0, lag: zeroLag }, enam: { ts_ms: 1_000_000_000, lag: zeroLag } };
    const plan = planPromote(base, hbs, "wnam", "enam", now);
    expect(plan.ok).toBe(true);
    if (!plan.ok) return;
    expect(plan.audits.map((a) => a.event_type)).toEqual([
      "corelink.failover.region_demoted.v1",
      "corelink.failover.region_promoted.v1",
    ]);
    expect(plan.roles.wnam).toEqual({ role: "hot_standby", cooldown_started_ms: now });
    expect(plan.roles.enam).toEqual({ role: "primary", cooldown_started_ms: null });
  });

  it("anti-flap: refuses when the old primary is still eligible", () => {
    const hbs: HeartbeatMap = {
      wnam: { ts_ms: 1_000_000_000, lag: zeroLag },
      enam: { ts_ms: 1_000_000_000, lag: zeroLag },
    };
    const plan = planPromote(base, hbs, "wnam", "enam", 1_000_001_000);
    expect(plan.ok).toBe(false);
    if (plan.ok) return;
    expect(plan.error.kind).toBe("primary_still_eligible");
  });

  it("split-brain: refuses when another region already holds primary", () => {
    const roles: RoleMap = {
      wnam: { role: "primary", cooldown_started_ms: null },
      enam: { role: "primary", cooldown_started_ms: null }, // illegal 2nd primary
      weur: { role: "replica", cooldown_started_ms: null },
    };
    const hbs: HeartbeatMap = { wnam: { ts_ms: 0, lag: zeroLag }, weur: { ts_ms: 1_000_000_000, lag: zeroLag } };
    const plan = planPromote(roles, hbs, "wnam", "weur", 1_000_001_000);
    expect(plan.ok).toBe(false);
    if (plan.ok) return;
    expect(plan.error.kind).toBe("split_brain_rejected");
  });
});

describe("planFailback 24h cool-down", () => {
  const demotedAt = 1_000_000_000;
  const roles: RoleMap = {
    wnam: { role: "hot_standby", cooldown_started_ms: demotedAt },
    enam: { role: "primary", cooldown_started_ms: null },
  };

  it("blocks before 24h and emits a failback_blocked audit", () => {
    const plan = planFailback(roles, "wnam", demotedAt + (HOT_STANDBY_COOLDOWN_SECONDS - 1) * 1000);
    expect(plan.ok).toBe(false);
    if (plan.ok) return;
    expect(plan.error.kind).toBe("cooldown_not_elapsed");
    expect(plan.blockedAudit?.event_type).toBe("corelink.failover.failback_blocked.v1");
  });

  it("commits at exactly 24h, demoting the current primary to replica", () => {
    const plan = planFailback(roles, "wnam", demotedAt + HOT_STANDBY_COOLDOWN_SECONDS * 1000);
    expect(plan.ok).toBe(true);
    if (!plan.ok) return;
    expect(plan.audits[0].event_type).toBe("corelink.failover.failback_committed.v1");
    expect(plan.roles.wnam.role).toBe("primary");
    expect(plan.roles.enam.role).toBe("replica");
  });
});

describe("buildStatus", () => {
  it("healthy iff exactly one fresh, within-SLO primary", () => {
    const roles: RoleMap = {
      wnam: { role: "primary", cooldown_started_ms: null },
      enam: { role: "replica", cooldown_started_ms: null },
    };
    const now = 1_000_001_000;
    let s = buildStatus(roles, { wnam: { ts_ms: 1_000_000_000, lag: zeroLag } }, now);
    expect(s.healthy).toBe(true);
    expect(s.regions).toHaveLength(2);
    // Break the primary heartbeat.
    s = buildStatus(roles, { wnam: { ts_ms: 0, lag: zeroLag } }, now);
    expect(s.healthy).toBe(false);
  });
});

// ──────────────────────────────────────────────────────────────────────────────
// DO shell + alarm driver
// ──────────────────────────────────────────────────────────────────────────────

describe("ReplicationCoordinatorDO shell", () => {
  it("register + heartbeat + status round-trip; alarm self-arms", async () => {
    const { ddo, alarm } = makeDO();
    await ddo.fetch(req("/_repl/register", "POST", { region: "wnam", role: "primary" }));
    await ddo.fetch(req("/_repl/register", "POST", { region: "enam", role: "replica" }));
    // Self-armed on the first register.
    expect(alarm.at).not.toBeNull();

    await ddo.fetch(
      req("/_repl/heartbeat", "POST", { region: "wnam", ts_ms: 1_000_000, lag: zeroLag }),
    );
    const resp = await ddo.fetch(req("/_repl/status"));
    expect(resp.status).toBe(200);
    const body = (await resp.json()) as { regions: unknown[] };
    expect(body.regions).toHaveLength(2);
  });

  it("register rejects a second, different primary (split-brain 409)", async () => {
    const { ddo } = makeDO();
    await ddo.fetch(req("/_repl/register", "POST", { region: "wnam", role: "primary" }));
    const resp = await ddo.fetch(req("/_repl/register", "POST", { region: "enam", role: "primary" }));
    expect(resp.status).toBe(409);
  });

  it("alarm() drives promotion when the primary is stale + re-arms", async () => {
    const { ddo, store, alarm } = makeDO();
    await ddo.fetch(req("/_repl/register", "POST", { region: "wnam", role: "primary" }));
    await ddo.fetch(req("/_repl/register", "POST", { region: "enam", role: "replica" }));
    // Primary stale, replica fresh (relative to Date.now()).
    const now = Date.now();
    await ddo.fetch(req("/_repl/heartbeat", "POST", { region: "wnam", ts_ms: 0, lag: zeroLag }));
    await ddo.fetch(req("/_repl/heartbeat", "POST", { region: "enam", ts_ms: now, lag: zeroLag }));

    alarm.at = null;
    await ddo.alarm();

    const roles = store.get("roles") as RoleMap;
    expect(roles.enam.role).toBe("primary");
    expect(roles.wnam.role).toBe("hot_standby");
    // Driver re-armed itself.
    expect(alarm.at).not.toBeNull();
    expect(primaryIn(roles)).toBe("enam");
  });

  it("state survives a simulated DO restart (new instance, same storage)", async () => {
    const store = new Map<string, unknown>();
    const alarm = { at: null as number | null };
    const first = new ReplicationCoordinatorDO(makeBackedState(store, alarm), makeEnv());
    await first.fetch(req("/_repl/register", "POST", { region: "wnam", role: "primary" }));

    // Fresh instance over the SAME storage (wake after eviction).
    const second = new ReplicationCoordinatorDO(makeBackedState(store, alarm), makeEnv());
    const resp = await second.fetch(req("/_repl/status"));
    const body = (await resp.json()) as { regions: { region: string; role: string }[] };
    expect(body.regions).toEqual([
      expect.objectContaining({ region: "wnam", role: "primary" }),
    ]);
  });

  it("audit-BEFORE-mutation is fail-CLOSED (state unchanged when audit throws)", async () => {
    const { ddo, store } = makeDO();
    await ddo.fetch(req("/_repl/register", "POST", { region: "wnam", role: "primary" }));
    await ddo.fetch(req("/_repl/register", "POST", { region: "enam", role: "replica" }));
    const now = Date.now();
    await ddo.fetch(req("/_repl/heartbeat", "POST", { region: "wnam", ts_ms: 0, lag: zeroLag }));
    await ddo.fetch(req("/_repl/heartbeat", "POST", { region: "enam", ts_ms: now, lag: zeroLag }));

    // Force the audit sink to throw → the promote MUST NOT mutate roles.
    (ddo as unknown as { emitAudit: () => void }).emitAudit = () => {
      throw new Error("audit sink down");
    };
    await expect(ddo.fetch(req("/_repl/promote", "POST", { primary: "wnam", replica: "enam" }))).rejects.toThrow();

    const roles = store.get("roles") as RoleMap;
    expect(roles.wnam.role).toBe("primary");
    expect(roles.enam.role).toBe("replica");
  });

  it("uses the documented tick interval when arming the alarm", async () => {
    const { ddo, alarm } = makeDO();
    const before = Date.now();
    await ddo.fetch(req("/_repl/arm", "POST"));
    expect(alarm.at).not.toBeNull();
    expect(alarm.at!).toBeGreaterThanOrEqual(before + REPLICATION_TICK_INTERVAL_MS - 50);
  });

  it("health endpoint responds without auth", async () => {
    const { ddo } = makeDO();
    const resp = await ddo.fetch(req("/_do/health"));
    expect(resp.status).toBe(200);
  });
});
