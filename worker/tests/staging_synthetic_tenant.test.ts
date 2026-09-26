import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Env } from "../src/index_common.js";
import { matchRoute } from "../src/route_match.js";
import {
  computeSyntheticTenantRef,
  handleStagingSyntheticTenant,
  provisionSyntheticByokTenant,
} from "../src/lib/staging_synthetic_tenant.js";

const KEY = "01234567890123456789012345678901";
const SHA = "a".repeat(40);
const REQUEST_ID = "b".repeat(32);
const NOW = Date.now();

function fakeDb(opts: {
  runSha?: string | null;
  failBatch?: boolean;
  preexisting?: "tenant" | "byok" | "gate";
  collideTenantId?: string;
} = {}) {
  const batches: Array<Array<{ sql: string; values: unknown[] }>> = [];
  const admittedRuns = new Set<string>();
  const db = {
    prepare(sql: string) {
      return {
        bind(...values: unknown[]) {
          return {
            sql,
            values,
            async first() {
              return opts.runSha === null ? null : { target_deployment_sha: opts.runSha ?? SHA };
            },
          };
        },
      };
    },
    async batch(statements: Array<{ sql: string; values: unknown[] }>) {
      batches.push(statements);
      if (opts.failBatch) throw new Error("simulated atomic batch failure");
      if (admittedRuns.has("123456") && statements[0]?.sql.includes("tenant_already_provisioned")) {
        throw new Error("synthetic tenant marker replay rejected");
      }
      if (opts.preexisting === "tenant" && statements[1]?.sql.includes("EXISTS (SELECT 1 FROM tenant WHERE tenant_id")) {
        throw new Error("preexisting tenant guard rejected provision");
      }
      if (opts.preexisting === "byok" && statements[1]?.sql.includes("tenant_byok_config WHERE tenant_id")) {
        throw new Error("preexisting BYOK material guard rejected provision");
      }
      if (opts.preexisting === "gate" && statements.at(-1)?.sql.includes("current_generation = 0")) {
        throw new Error("generation-zero readback guard rejected provision");
      }
      const tenantInsert = statements.find(statement => statement.sql.startsWith("INSERT INTO tenant ("));
      if (tenantInsert?.values[0] === opts.collideTenantId) throw new Error("tenant primary key collision");
      admittedRuns.add("123456");
      return statements.map(() => ({ success: true, meta: { changes: 1 } }));
    },
  };
  return { db: db as unknown as Env["CONFIG_DB"], batches };
}

async function envelope(overrides: Partial<{ scenario: string; sha: string; requestId: string }> = {}) {
  const scenario = overrides.scenario ?? "byok";
  const sha = overrides.sha ?? SHA;
  const requestId = overrides.requestId ?? REQUEST_ID;
  const fields = ["v1", "123456", scenario, "staging", sha, String(NOW - 1_000), String(NOW + 60_000), requestId];
  const key = await crypto.subtle.importKey("raw", new TextEncoder().encode(KEY), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  const tag = new Uint8Array(await crypto.subtle.sign(
    "HMAC", key, new TextEncoder().encode(`corelink/staging-ownership-envelope/v1\0${fields.join(".")}`),
  ));
  return `${fields.join(".")}.${Array.from(tag, byte => byte.toString(16).padStart(2, "0")).join("")}`;
}

async function request(opts: { body?: string; query?: string; scenario?: string; sha?: string; requestId?: string } = {}) {
  const signed = await envelope(opts);
  return new Request(`https://corelink.test/internal/v1/staging/byok/synthetic-tenant${opts.query ?? ""}`, {
    method: "POST",
    ...(opts.body === undefined ? {} : { body: opts.body }),
    headers: {
      "x-corelink-staging-load-admission": signed,
      "x-corelink-staging-request-id": opts.requestId ?? REQUEST_ID,
    },
  });
}

function env(db: Env["CONFIG_DB"], overrides: Partial<Env> = {}): Env {
  return {
    CONFIG_DB: db,
    ENVIRONMENT: "staging",
    CORELINK_ENVIRONMENT: "staging",
    CORELINK_STAGING_LOAD_TEST_ADMISSION_KEY: KEY,
    ...overrides,
  } as Env;
}

describe("staging synthetic BYOK tenant route", () => {
  beforeEach(() => vi.restoreAllMocks());

  it("matches the frozen cross-language tenant_ref framing vector", async () => {
    expect(await computeSyntheticTenantRef(
      "123456", "byok", SHA, "11111111-1111-4111-8111-111111111111",
    )).toBe("0106591dc8ab4f03e23dc578aeb3e20fb7436e93284540e0d457e448e92959f0");
  });

  it("routes only the exact staging provision endpoint to its Worker-owned handler", () => {
    expect(matchRoute(new URL("https://corelink.test/internal/v1/staging/byok/synthetic-tenant")).routeKind)
      .toBe("staging_synthetic_tenant");
    expect(matchRoute(new URL("https://corelink.test/internal/v1/staging/byok/synthetic-tenant/other")).routeKind)
      .not.toBe("staging_synthetic_tenant");
  });

  it.each([
    ["forged signature", async () => (await envelope()).replace(/.$/, char => char === "0" ? "1" : "0")],
    ["wrong scenario", () => envelope({ scenario: "signup" })],
    ["wrong request binding", () => envelope({ requestId: "c".repeat(32) })],
  ])("rejects %s before any D1 write", async (_name, makeCredential) => {
    const { db, batches } = fakeDb();
    const signed = await makeCredential();
    const req = new Request("https://corelink.test/internal/v1/staging/byok/synthetic-tenant", {
      method: "POST",
      headers: { "x-corelink-staging-load-admission": signed, "x-corelink-staging-request-id": REQUEST_ID },
    });
    await expect(provisionSyntheticByokTenant(req, env(db), NOW)).rejects.toThrow();
    expect(batches).toHaveLength(0);
  });

  it.each([
    ["caller body", { body: JSON.stringify({ tenant_id: "customer", region: "weur" }) }],
    ["caller region query", { query: "?region=weur" }],
  ])("rejects %s so caller identity and region cannot be selected", async (_name, opts) => {
    const { db, batches } = fakeDb();
    await expect(provisionSyntheticByokTenant(await request(opts), env(db), NOW)).rejects.toThrow();
    expect(batches).toHaveLength(0);
  });

  it("rejects non-staging Worker configuration and mismatched run deployment", async () => {
    const { db, batches } = fakeDb({ runSha: "c".repeat(40) });
    await expect(provisionSyntheticByokTenant(await request(), env(db), NOW)).rejects.toThrow();
    await expect(provisionSyntheticByokTenant(await request(), env(db, { CORELINK_ENVIRONMENT: "prod" }), NOW))
      .rejects.toThrow();
    expect(batches).toHaveLength(0);
    const missingRun = fakeDb({ runSha: null });
    await expect(provisionSyntheticByokTenant(await request(), env(missingRun.db), NOW)).rejects.toThrow();
    expect(missingRun.batches).toHaveLength(0);
  });

  it("atomically provisions a fresh tenant, generation-zero marker, ownership row, and sealed locator", async () => {
    const { db, batches } = fakeDb();
    const locator = await provisionSyntheticByokTenant(await request(), env(db), NOW);
    expect(locator).toEqual({
      run_id: "123456", scenario: "byok", tenant_ref: expect.stringMatching(/^[0-9a-f]{64}$/),
    });
    expect(JSON.stringify(locator)).not.toMatch(/tenant_id|intent_id|[0-9a-f]{8}-[0-9a-f]{4}/);
    expect(batches).toHaveLength(1);
    const batch = batches[0]!;
    expect(batch).toHaveLength(5);
    expect(batch[0]!.sql).toContain("tenant_already_provisioned");
    expect(batch[1]!.sql).toContain("preexisting_state_detected");
    expect(batch[2]!.sql).toBe("INSERT INTO tenant (tenant_id, primary_region, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?3)");
    expect(batch[2]!.values[1]).toBe("wnam");
    expect(batch[3]!.sql).toContain("generation_zero_empty");
    expect(batch[4]!.sql).toContain("current_generation = 0");
    expect(batch[4]!.sql).toContain("byok_logical_object_generation");
    expect(batch.some(statement => statement.sql.includes("staging_load_test_resources"))).toBe(false);
    expect(batch.some(statement => statement.sql.includes("staging_load_test_teardown_locators"))).toBe(false);
  });

  it("propagates batch failure so partial provisioning is not reported as success", async () => {
    const { db } = fakeDb({ failBatch: true });
    await expect(provisionSyntheticByokTenant(await request(), env(db), NOW)).rejects.toThrow();
  });

  it.each([
    ["preexisting tenant", { preexisting: "tenant" as const }],
    ["prior BYOK material", { preexisting: "byok" as const }],
    ["generation readback mismatch", { preexisting: "gate" as const }],
  ])("fails closed on %s and keeps the write batch uncommitted", async (_name, options) => {
    const { db, batches } = fakeDb(options);
    await expect(provisionSyntheticByokTenant(await request(), env(db), NOW)).rejects.toThrow();
    expect(batches).toHaveLength(1);
  });

  it("uses a plain tenant INSERT so a generated duplicate ID aborts the batch", async () => {
    const tenantId = "11111111-1111-4111-8111-111111111111";
    vi.spyOn(crypto, "randomUUID")
      .mockReturnValueOnce(tenantId)
      .mockReturnValueOnce("22222222222242228222222222222222");
    const { db, batches } = fakeDb({ collideTenantId: tenantId });
    await expect(provisionSyntheticByokTenant(await request(), env(db), NOW)).rejects.toThrow();
    expect(batches[0]?.find(statement => statement.sql.startsWith("INSERT INTO tenant ("))?.sql)
      .not.toContain("OR IGNORE");
  });

  it("rejects replay of the same exact run through the immutable synthetic marker", async () => {
    const { db, batches } = fakeDb();
    const signedRequest = await request();
    await provisionSyntheticByokTenant(signedRequest, env(db), NOW);
    await expect(provisionSyntheticByokTenant(signedRequest, env(db), NOW)).rejects.toThrow();
    expect(batches).toHaveLength(2);
  });

  it("returns no identity or credential on handler success and fixed errors on failure", async () => {
    const { db } = fakeDb();
    const ok = await handleStagingSyntheticTenant(await request(), env(db));
    expect(ok.status).toBe(201);
    const body = await ok.text();
    expect(body).toContain("tenant_ref");
    expect(body).not.toMatch(/tenant_id|intent_id|x-corelink-staging-load-admission/);

    const invalid = await handleStagingSyntheticTenant(new Request("https://corelink.test/internal/v1/staging/byok/synthetic-tenant", { method: "POST" }), env(db));
    expect(invalid.status).toBe(403);
    expect(await invalid.text()).not.toMatch(/tenant_id|intent_id|credential|x-corelink/i);
  });
});
