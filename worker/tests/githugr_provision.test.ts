/**
 * Unit tests for the githugr per-user tenant PROVISION-OR-LOOKUP helper
 * (pilot isolation fix — worker/src/lib/githugr_provision.ts).
 *
 * Asserts the four campaign invariants at the helper level:
 *   (a) ISOLATION  — two distinct subs → two DISTINCT deterministic tenant_ids.
 *   (b) IDEMPOTENT — same sub twice → SAME tenant_id, no dup rows, no error.
 *   (c) FAIL-CLOSED — a D1 error propagates (caller maps it to a 500); the helper
 *                     NEVER swallows it or returns a shared tenant.
 *   (d) FULL ROW-FAMILY — the provision writes all 5 row-families (tenant,
 *                     tier_selections, runners_entitlement, tenant_quota,
 *                     tenant_org_map) with the `tenant` row FIRST (FK order).
 */

import { describe, it, expect } from "vitest";
import {
  deriveGithugrTenantId,
  provisionOrLookupGithugrTenant,
  type GithugrProvisionDb,
} from "../src/lib/githugr_provision.js";

const UUID_V5_SHAPE = /^[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

/** A recording fake D1: captures the SQL of each statement (in order) and keeps a
 *  stateful tenant_org_map so the read-back reflects prior INSERTs. */
function makeFakeDb(opts: { throwOn?: string; orgMap?: Map<string, string> } = {}): {
  db: GithugrProvisionDb;
  sqlLog: string[];
  orgMap: Map<string, string>;
} {
  const sqlLog: string[] = [];
  const orgMap = opts.orgMap ?? new Map<string, string>();
  const db: GithugrProvisionDb = {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        run: async () => {
          sqlLog.push(sql);
          if (opts.throwOn && sql.includes(opts.throwOn)) {
            throw new Error(`simulated D1 fault on: ${opts.throwOn}`);
          }
          if (sql.includes("INSERT OR IGNORE INTO tenant_org_map")) {
            const [clerkOrgId, tenantId] = args as [string, string, number];
            if (!orgMap.has(clerkOrgId)) orgMap.set(clerkOrgId, tenantId);
          }
          return { success: true };
        },
        first: async <T>() => {
          sqlLog.push(sql);
          if (opts.throwOn && sql.includes(opts.throwOn)) {
            throw new Error(`simulated D1 fault on: ${opts.throwOn}`);
          }
          if (sql.includes("tenant_org_map")) {
            const clerkOrgId = args[0] as string;
            const t = orgMap.get(clerkOrgId);
            return (t ? { tenant_id: t } : null) as T | null;
          }
          return null;
        },
      }),
    }),
  };
  return { db, sqlLog, orgMap };
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
  it("(d) FULL ROW-FAMILY: writes all 5 families with `tenant` FIRST (FK order)", async () => {
    const { db, sqlLog } = makeFakeDb();
    const resolved = await provisionOrLookupGithugrTenant(db, "user_prov", 1781568000000);

    expect(resolved).toMatch(UUID_V5_SHAPE);
    expect(resolved).toBe(await deriveGithugrTenantId("user_prov"));

    const inserts = sqlLog.filter((s) => s.startsWith("INSERT OR IGNORE"));
    // FK order: tenant is the first write.
    expect(inserts[0]).toContain("INTO tenant ");
    // All 5 families present.
    expect(sqlLog.some((s) => s.includes("INSERT OR IGNORE INTO tenant "))).toBe(true);
    expect(sqlLog.some((s) => s.includes("INSERT OR IGNORE INTO tier_selections"))).toBe(true);
    expect(sqlLog.some((s) => s.includes("INSERT OR IGNORE INTO runners_entitlement"))).toBe(true);
    expect(sqlLog.some((s) => s.includes("INSERT OR IGNORE INTO tenant_quota"))).toBe(true);
    expect(sqlLog.some((s) => s.includes("INSERT OR IGNORE INTO tenant_org_map"))).toBe(true);
    // Read-back of the authoritative mapping.
    expect(sqlLog.some((s) => s.includes("SELECT tenant_id FROM tenant_org_map"))).toBe(true);
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

  it("(c) FAIL-CLOSED: a D1 fault on the tenant INSERT PROPAGATES (never a silent shared tenant)", async () => {
    const { db } = makeFakeDb({ throwOn: "INTO tenant " });
    await expect(provisionOrLookupGithugrTenant(db, "user_err", 1)).rejects.toThrow();
  });

  it("(c) FAIL-CLOSED: a D1 fault on the read-back SELECT PROPAGATES", async () => {
    const { db } = makeFakeDb({ throwOn: "SELECT tenant_id FROM tenant_org_map" });
    await expect(provisionOrLookupGithugrTenant(db, "user_err2", 1)).rejects.toThrow();
  });

  it("(c) FAIL-CLOSED: a missing read-back row THROWS (no row after provision ⇒ error, not a guess)", async () => {
    // A db whose org-map INSERT is a no-op AND read-back returns null.
    const db: GithugrProvisionDb = {
      prepare: (_sql: string) => ({
        bind: (..._args: unknown[]) => ({
          run: async () => ({ success: true }),
          first: async <T>() => null as T | null, // read-back miss
        }),
      }),
    };
    await expect(provisionOrLookupGithugrTenant(db, "user_missing", 1)).rejects.toThrow();
  });
});
