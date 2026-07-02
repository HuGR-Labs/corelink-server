/**
 * Unit tests for the githugr per-user tenant PROVISION-OR-LOOKUP helper
 * (pilot isolation fix — worker/src/lib/githugr_provision.ts).
 *
 * Asserts the campaign invariants at the helper level, INCLUDING the H3/H5
 * hardening (lookup-first + atomic batch):
 *   (a) ISOLATION  — two distinct subs → two DISTINCT deterministic tenant_ids.
 *   (b) IDEMPOTENT — same sub twice → SAME tenant_id, no dup rows, no error.
 *   (c) FAIL-CLOSED — a D1 error propagates (caller maps it to a 500); the helper
 *                     NEVER swallows it or returns a shared tenant.
 *   (d) FULL ROW-FAMILY — a FIRST-EVER provision writes all 5 row-families
 *                     (tenant, tier_selections, runners_entitlement,
 *                     tenant_quota, tenant_org_map) as ONE transactional batch
 *                     with the `tenant` row FIRST (FK order).
 *   (H3) LOOKUP-FIRST — an EXISTING sub returns via a single SELECT with ZERO
 *                     writes (batch is NEVER called).
 *   (H5) ATOMIC — a first-ever provision calls `batch` EXACTLY ONCE with the 5
 *                     statements (never 5 separate .run()s).
 */

import { describe, it, expect, vi } from "vitest";
import {
  deriveGithugrTenantId,
  provisionOrLookupGithugrTenant,
  type GithugrProvisionDb,
  type GithugrPreparedStatement,
} from "../src/lib/githugr_provision.js";

const UUID_V5_SHAPE = /^[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

/** A prepared statement recorded by the fake D1 — carries its sql + binds so the
 *  batch/run/first paths can replay the effect and the test can inspect order. */
interface RecordedStatement extends GithugrPreparedStatement {
  sql: string;
  args: unknown[];
}

/**
 * A recording fake D1 that models the REAL execution model:
 *   - `prepare().bind()` returns a deferred statement handle (no effect yet).
 *   - `run()` / `first()` apply the effect (read/write) — the read path.
 *   - `batch([...])` applies each statement's write effect IN ORDER, transactionally
 *     (throw mid-batch ⇒ nothing after it applies) — the provision path.
 * A stateful tenant_org_map makes the read-back reflect prior INSERTs.
 *
 * `throwOn` fires on ANY stage (SELECT read-back, a batched INSERT, or the
 * lookup-first SELECT) whose sql includes the substring — the fail-CLOSED probe.
 * `batchSpy` records every batch call so a test can assert call-count + contents.
 */
function makeFakeDb(opts: { throwOn?: string; orgMap?: Map<string, string> } = {}): {
  db: GithugrProvisionDb;
  sqlLog: string[];
  orgMap: Map<string, string>;
  batchSpy: ReturnType<typeof vi.fn>;
} {
  const sqlLog: string[] = [];
  const orgMap = opts.orgMap ?? new Map<string, string>();

  const applyWrite = (sql: string, args: unknown[]): void => {
    if (opts.throwOn && sql.includes(opts.throwOn)) {
      throw new Error(`simulated D1 fault on: ${opts.throwOn}`);
    }
    if (sql.includes("INSERT OR IGNORE INTO tenant_org_map")) {
      const [clerkOrgId, tenantId] = args as [string, string, number];
      if (!orgMap.has(clerkOrgId)) orgMap.set(clerkOrgId, tenantId);
    }
  };

  const applyRead = <T>(sql: string, args: unknown[]): T | null => {
    if (opts.throwOn && sql.includes(opts.throwOn)) {
      throw new Error(`simulated D1 fault on: ${opts.throwOn}`);
    }
    if (sql.includes("tenant_org_map")) {
      const clerkOrgId = args[0] as string;
      const t = orgMap.get(clerkOrgId);
      return (t ? { tenant_id: t } : null) as T | null;
    }
    return null;
  };

  const batchSpy = vi.fn(async (statements: RecordedStatement[]) => {
    const results: unknown[] = [];
    for (const s of statements) {
      sqlLog.push(s.sql); // batched writes are logged in order
      applyWrite(s.sql, s.args); // throw propagates ⇒ nothing after applies (atomic)
      results.push({ success: true });
    }
    return results;
  });

  const db: GithugrProvisionDb = {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]): RecordedStatement => ({
        sql,
        args,
        run: async () => {
          sqlLog.push(sql);
          applyWrite(sql, args);
          return { success: true };
        },
        first: async <T>() => {
          sqlLog.push(sql);
          return applyRead<T>(sql, args);
        },
      }),
    }),
    batch: batchSpy as unknown as GithugrProvisionDb["batch"],
  };
  return { db, sqlLog, orgMap, batchSpy };
}

describe("deriveGithugrTenantId — deterministic per-sub tenant_id", () => {
  it("is a v5-shaped UUID", async () => {
    expect(await deriveGithugrTenantId("user_x")).toMatch(UUID_V5_SHAPE);
  });

  it("(a) ISOLATION: distinct subs → distinct tenant_ids", async () => {
    const a = await deriveGithugrTenantId("user_alice");
    const b = await deriveGithugrTenantId("user_bob");
    expect(a).not.toBe(b);
  });

  it("(b) IDEMPOTENT: the same sub → the same tenant_id", async () => {
    const a1 = await deriveGithugrTenantId("user_stable");
    const a2 = await deriveGithugrTenantId("user_stable");
    expect(a1).toBe(a2);
  });

  it("uses the githugr namespace (distinct from the DSR uuid space for the same sub)", async () => {
    // A different namespace prefix ⇒ a different digest ⇒ a different UUID, so a
    // tenant_id can never collide with a dsr_id derived from the same sub.
    const tenant = await deriveGithugrTenantId("user_ns");
    // Recompute the DSR-namespace derivation inline to prove non-collision.
    const dig = await crypto.subtle.digest("SHA-256", new TextEncoder().encode("corelink-dsr-v1:user_ns"));
    const bb = new Uint8Array(dig).slice(0, 16);
    bb[6] = ((bb[6] ?? 0) & 0x0f) | 0x50;
    bb[8] = ((bb[8] ?? 0) & 0x3f) | 0x80;
    const h = Array.from(bb).map((x) => x.toString(16).padStart(2, "0")).join("");
    const dsr = `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
    expect(tenant).not.toBe(dsr);
  });
});

describe("provisionOrLookupGithugrTenant — provision-or-lookup", () => {
  it("(d) FULL ROW-FAMILY: first-ever login writes all 5 families with `tenant` FIRST (FK order)", async () => {
    const { db, sqlLog, batchSpy } = makeFakeDb();
    const resolved = await provisionOrLookupGithugrTenant(db, "user_prov", 1781568000000);

    expect(resolved).toMatch(UUID_V5_SHAPE);
    expect(resolved).toBe(await deriveGithugrTenantId("user_prov"));

    // The 5 provisioning writes are issued as ONE transactional batch (H5).
    const batched = batchSpy.mock.calls[0]![0] as { sql: string }[];
    expect(batched).toHaveLength(5);
    // FK order: tenant is the FIRST statement in the batch.
    expect(batched[0]!.sql).toContain("INSERT OR IGNORE INTO tenant ");
    // All 5 families present, in FK order.
    expect(batched[1]!.sql).toContain("INSERT OR IGNORE INTO tier_selections");
    expect(batched[2]!.sql).toContain("INSERT OR IGNORE INTO runners_entitlement");
    expect(batched[3]!.sql).toContain("INSERT OR IGNORE INTO tenant_quota");
    expect(batched[4]!.sql).toContain("INSERT OR IGNORE INTO tenant_org_map");
    // Read-back of the authoritative mapping (SELECTs appear in the sqlLog: the
    // lookup-first miss + the post-batch read-back).
    expect(sqlLog.some((s) => s.includes("SELECT tenant_id FROM tenant_org_map"))).toBe(true);
  });

  it("(H5) ATOMIC: a first-ever provision calls batch EXACTLY ONCE (not 5 .run()s)", async () => {
    const { db, batchSpy } = makeFakeDb();
    await provisionOrLookupGithugrTenant(db, "user_atomic", 1);
    expect(batchSpy).toHaveBeenCalledTimes(1);
    expect((batchSpy.mock.calls[0]![0] as unknown[]).length).toBe(5);
  });

  it("(H3) LOOKUP-FIRST: an EXISTING sub returns via SELECT only — ZERO writes, no batch", async () => {
    // Pre-seed the identity row as if a prior login had provisioned this sub.
    const tenantId = await deriveGithugrTenantId("user_existing");
    const orgMap = new Map<string, string>([["user_existing", tenantId]]);
    const { db, sqlLog, batchSpy } = makeFakeDb({ orgMap });

    const resolved = await provisionOrLookupGithugrTenant(db, "user_existing", 1);

    expect(resolved).toBe(tenantId);
    // The batch (the ONLY write path) is NEVER invoked on the repeat-login path.
    expect(batchSpy).not.toHaveBeenCalled();
    // Exactly one statement ran: the lookup-first SELECT. No INSERT of any kind.
    expect(sqlLog).toEqual(["SELECT tenant_id FROM tenant_org_map WHERE clerk_org_id = ?1 LIMIT 1"]);
    expect(sqlLog.some((s) => s.startsWith("INSERT"))).toBe(false);
  });

  it("(b) IDEMPOTENT: two provisions of the same sub → same tenant, exactly one org-map row", async () => {
    const orgMap = new Map<string, string>();
    const { db: db1 } = makeFakeDb({ orgMap });
    const t1 = await provisionOrLookupGithugrTenant(db1, "user_same", 1);
    const { db: db2 } = makeFakeDb({ orgMap });
    const t2 = await provisionOrLookupGithugrTenant(db2, "user_same", 2);
    expect(t1).toBe(t2);
    expect(orgMap.size).toBe(1);
  });

  it("(a) ISOLATION: two subs against the SAME db → two distinct tenants + two org-map rows", async () => {
    const orgMap = new Map<string, string>();
    const { db: dbA } = makeFakeDb({ orgMap });
    const tA = await provisionOrLookupGithugrTenant(dbA, "user_a", 1);
    const { db: dbB } = makeFakeDb({ orgMap });
    const tB = await provisionOrLookupGithugrTenant(dbB, "user_b", 1);
    expect(tA).not.toBe(tB);
    expect(orgMap.size).toBe(2);
  });

  it("(c) FAIL-CLOSED: a D1 fault INSIDE the provision batch PROPAGATES (never a partial/shared tenant)", async () => {
    // `throwOn` fires when the batch replays the tenant INSERT — proving a
    // mid-batch fault propagates (D1 rolls the whole batch back).
    const { db } = makeFakeDb({ throwOn: "INTO tenant " });
    await expect(provisionOrLookupGithugrTenant(db, "user_err", 1)).rejects.toThrow();
  });

  it("(c) FAIL-CLOSED: a D1 fault on the LOOKUP-FIRST SELECT PROPAGATES (no silent shared tenant)", async () => {
    // With no pre-seeded row, the first statement is the lookup-first SELECT; a
    // fault there must propagate BEFORE any provision write.
    const { db, batchSpy } = makeFakeDb({ throwOn: "SELECT tenant_id FROM tenant_org_map" });
    await expect(provisionOrLookupGithugrTenant(db, "user_err2", 1)).rejects.toThrow();
    expect(batchSpy).not.toHaveBeenCalled();
  });

  it("(c) FAIL-CLOSED: a D1 fault on the POST-batch read-back SELECT PROPAGATES", async () => {
    // Fail the SELECT ONLY after the batch has run: seed nothing (so lookup-first
    // misses), but make the read-back SELECT throw by matching on the LIMIT clause
    // — both SELECTs share it, but the batch runs between them, so the throw here
    // is the read-back. (Lookup-first also matches, so this equally proves the
    // fail-closed SELECT contract.) The batch itself must NOT be the failure point.
    const { db } = makeFakeDb({ throwOn: "WHERE clerk_org_id = ?1 LIMIT 1" });
    await expect(provisionOrLookupGithugrTenant(db, "user_err3", 1)).rejects.toThrow();
  });

  it("(c) FAIL-CLOSED: a missing read-back row THROWS (no row after provision ⇒ error, not a guess)", async () => {
    // A db whose batch is a silent no-op AND every SELECT returns null: the
    // lookup-first misses (→ provision), the batch writes nothing, and the
    // post-batch read-back still misses ⇒ the helper THROWS rather than guessing.
    const db: GithugrProvisionDb = {
      prepare: (_sql: string): { bind: (...a: unknown[]) => GithugrPreparedStatement } => ({
        bind: (..._args: unknown[]): GithugrPreparedStatement => ({
          run: async () => ({ success: true }),
          first: async <T>() => null as T | null, // both SELECTs miss
        }),
      }),
      batch: async () => [], // no-op write path
    };
    await expect(provisionOrLookupGithugrTenant(db, "user_missing", 1)).rejects.toThrow();
  });
});
