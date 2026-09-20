/**
 * Acceptance suite for #1725: scoped, auditable runner installation
 * deprovisioning. The D1 fake below is stateful: reads observe prior writes,
 * batches are atomic, and each assertion checks the rows that remain.
 */
import { describe, expect, it } from "vitest";
import {
  handleInstallationDeprovision,
  type InstallationProvisionEnv,
} from "../src/webhooks/github_provision.js";

const AUTH = "a".repeat(64);
const INSTALLATION_AUTH = "b".repeat(64);
const EVENT = "corelink.runner.installation_deprovision_requested";
const URL = "https://sw/internal/v1/runner/provision-installation";

type Row = Record<string, unknown>;
interface Captured {
  sql: string;
  vals: unknown[];
}
interface AuditRow extends Row {
  id: string;
  tenant_id: string;
  request_id: string;
  event_type: string;
  payload_json: string;
  region: string;
}
interface Seed {
  installations?: Array<{ installation_id: string; tenant_id: string }>;
  repositories?: Array<{ tenant_id: string; repo_full_name: string }>;
  tenants?: Array<{ tenant_id: string; primary_region: string }>;
  audits?: AuditRow[];
}
interface FakeOptions {
  hasBatch?: boolean;
  failMutation?: boolean;
  ignoreRepoDeletes?: boolean;
}
interface FakeDb {
  binding: NonNullable<InstallationProvisionEnv["CONFIG_DB"]>;
  batchCalls: Captured[][];
  mutations: Captured[];
  queries: Captured[];
  snapshot(): {
    installations: Array<{ installation_id: string; tenant_id: string }>;
    repositories: Array<{ tenant_id: string; repo_full_name: string }>;
    audits: AuditRow[];
  };
}

function key(requestId: string, eventType: string): string {
  return `${requestId}\u0000${eventType}`;
}

/** Minimal stateful D1 model for the four tables touched by this contract. */
function fakeDb(seed: Seed = {}, options: FakeOptions = {}): FakeDb {
  const installations = new Map<string, string>(
    (seed.installations ?? []).map((row) => [row.installation_id, row.tenant_id]),
  );
  let repositories = (seed.repositories ?? []).map((row) => ({ ...row }));
  const tenants = new Map(
    (seed.tenants ?? []).map((row) => [row.tenant_id, row.primary_region]),
  );
  const audits = new Map<string, AuditRow>(
    (seed.audits ?? []).map((row) => [key(row.request_id, row.event_type), { ...row }]),
  );
  const batchCalls: Captured[][] = [];
  const mutations: Captured[] = [];
  const queries: Captured[] = [];
  const norm = (sql: string) => sql.replace(/\s+/g, " ").trim().toLowerCase();

  function selectRows(record: Captured): Row[] {
    const sql = norm(record.sql);
    if (sql.includes("from audit_outbox")) {
      const requestId = String(record.vals[0] ?? "");
      const eventType = String(record.vals[1] ?? "");
      const found = audits.get(key(requestId, eventType));
      return found ? [{ ...found }] : [];
    }
    if (sql.includes("from tenant_gh_installation_map")) {
      const installationId = String(record.vals[0] ?? "");
      const tenantId = installations.get(installationId);
      return tenantId ? [{ installation_id: installationId, tenant_id: tenantId }] : [];
    }
    if (sql.includes("from runner_repo_allowlist")) {
      const tenantId = String(record.vals[0] ?? "");
      let rows = repositories.filter((row) => row.tenant_id === tenantId);
      if (sql.includes("repo_full_name =")) {
        const repo = String(record.vals[1] ?? "");
        rows = rows.filter((row) => row.repo_full_name === repo);
      } else if (sql.includes("repo_full_name in")) {
        const repos = new Set(record.vals.slice(1).map(String));
        rows = rows.filter((row) => repos.has(row.repo_full_name));
      }
      if (/count\s*\(/.test(sql)) return [{ count: rows.length }];
      return rows.map((row) => ({ ...row }));
    }
    if (sql.includes("from tenant")) {
      const tenantId = String(record.vals[0] ?? "");
      const region = tenants.get(tenantId);
      return region ? [{ tenant_id: tenantId, primary_region: region }] : [];
    }
    throw new Error(`Unexpected D1 read in acceptance fake: ${record.sql}`);
  }

  function apply(record: Captured): void {
    const sql = norm(record.sql);
    mutations.push(record);
    if (options.failMutation) throw new Error("injected D1 mutation fault");

    if (sql.startsWith("delete from runner_repo_allowlist")) {
      if (options.ignoreRepoDeletes) return;
      const tenantId = String(record.vals[0] ?? "");
      const names = sql.includes("repo_full_name in")
        ? new Set(record.vals.slice(1).map(String))
        : new Set([String(record.vals[1] ?? "")]);
      repositories = repositories.filter(
        (row) => row.tenant_id !== tenantId || !names.has(row.repo_full_name),
      );
      return;
    }
    if (sql.startsWith("delete from tenant_gh_installation_map")) {
      const installationId = String(record.vals[0] ?? "");
      const tenantId = String(record.vals[1] ?? installations.get(installationId) ?? "");
      const currentTenant = installations.get(installationId);
      const hasResidual = repositories.some((row) => row.tenant_id === tenantId);
      const hasZeroRowsGuard = sql.includes("not exists") || sql.includes("runner_repo_allowlist");
      if (currentTenant === tenantId && (!hasZeroRowsGuard || !hasResidual)) {
        installations.delete(installationId);
      }
      return;
    }
    if (sql.startsWith("insert") && sql.includes("into audit_outbox")) {
      const columnsMatch = record.sql.match(/into\s+audit_outbox\s*\(([^)]+)\)/i);
      if (!columnsMatch) throw new Error("Audit INSERT must name its columns");
      const columns = columnsMatch[1].split(",").map((column) => column.trim());
      const row = Object.fromEntries(columns.map((column, index) => [column, record.vals[index]])) as AuditRow;
      audits.set(key(String(row.request_id), String(row.event_type)), row);
      return;
    }
    throw new Error(`Unexpected D1 mutation in acceptance fake: ${record.sql}`);
  }

  function statement(record: Captured) {
    const sql = norm(record.sql);
    const isRead = sql.startsWith("select");
    return {
      first: async <T = Row>(): Promise<T | null> => {
        queries.push(record);
        return (selectRows(record)[0] as T | undefined) ?? null;
      },
      all: async <T = Row>() => {
        queries.push(record);
        return { success: true, results: selectRows(record) as T[] };
      },
      run: async () => {
        if (isRead) {
          queries.push(record);
          return { success: true, meta: { changes: 0 } };
        }
        apply(record);
        return { success: true, meta: { changes: 1 } };
      },
    };
  }

  const configDb: Record<string, unknown> = {
    prepare(sql: string) {
      return {
        bind(...vals: unknown[]) {
          return statement({ sql, vals });
        },
      };
    },
  };
  if (options.hasBatch !== false) {
    configDb.batch = async (statements: Array<{ run(): Promise<unknown> }>) => {
      const beforeInstallations = new Map(installations);
      const beforeRepositories = repositories.map((row) => ({ ...row }));
      const beforeAudits = new Map(
        [...audits.entries()].map(([auditKey, row]) => [auditKey, { ...row }]),
      );
      const batch: Captured[] = [];
      // D1PreparedStatement is opaque at runtime. The fake's bound statements
      // expose their captured record as a non-enumerable test-only property.
      for (const item of statements as Array<{ __captured?: Captured; run(): Promise<unknown> }>) {
        if (item.__captured) batch.push(item.__captured);
      }
      batchCalls.push(batch);
      try {
        const results: unknown[] = [];
        for (const item of statements) results.push(await item.run());
        return results;
      } catch (error) {
        installations.clear();
        for (const [installationId, tenantId] of beforeInstallations) installations.set(installationId, tenantId);
        repositories = beforeRepositories;
        audits.clear();
        for (const [auditKey, row] of beforeAudits) audits.set(auditKey, row);
        throw error;
      }
    };
  }
  // Add metadata without changing the Worker-facing D1 method shape.
  const originalPrepare = configDb.prepare as (sql: string) => { bind(...vals: unknown[]): object };
  configDb.prepare = (sql: string) => {
    const prepared = originalPrepare(sql);
    const originalBind = prepared.bind;
    prepared.bind = (...vals: unknown[]) => {
      const bound = originalBind(...vals) as { run(): Promise<unknown> };
      Object.defineProperty(bound, "__captured", {
        value: { sql, vals },
        enumerable: false,
      });
      return bound;
    };
    return prepared;
  };

  return {
    binding: configDb as unknown as NonNullable<InstallationProvisionEnv["CONFIG_DB"]>,
    batchCalls,
    mutations,
    queries,
    snapshot: () => ({
      installations: [...installations].map(([installation_id, tenant_id]) => ({ installation_id, tenant_id })),
      repositories: repositories.map((row) => ({ ...row })),
      audits: [...audits.values()].map((row) => ({ ...row })),
    }),
  };
}

function body(overrides: Record<string, unknown> = {}) {
  return {
    request_id: "deprov-req-01",
    installation_id: "installation-01",
    tenant_id: "tenant-01",
    repositories: ["acme/web"],
    remove_installation: false,
    ...overrides,
  };
}

function request(
  input: unknown,
  options: { method?: string; auth?: string | null; raw?: boolean } = {},
): Request {
  const headers = new Headers({ "content-type": "application/json" });
  if (options.auth !== null) headers.set("authorization", `Bearer ${options.auth ?? AUTH}`);
  const method = options.method ?? "DELETE";
  return new Request(URL, {
    method,
    headers,
    body: method === "GET" || method === "HEAD"
      ? undefined
      : options.raw
        ? String(input)
        : JSON.stringify(input),
  });
}

function env(db: FakeDb, auth = AUTH): InstallationProvisionEnv {
  return {
    CONFIG_DB: db.binding,
    CORELINK_RUNNER_PROVISION_AUTH_KEY: auth,
  };
}

const BASE_SEED: Seed = {
  installations: [{ installation_id: "installation-01", tenant_id: "tenant-01" }],
  repositories: [{ tenant_id: "tenant-01", repo_full_name: "acme/web" }],
  tenants: [{ tenant_id: "tenant-01", primary_region: "sam" }],
};

describe("handleInstallationDeprovision — fail-closed gates", () => {
  it("accepts DELETE only and preserves the provision route's method/auth statuses", async () => {
    const methodDb = fakeDb(BASE_SEED);
    const method = await handleInstallationDeprovision(
      request(body(), { method: "POST", auth: null }),
      env(methodDb),
    );
    expect(method.status).toBe(405);
    expect(await method.json()).toEqual({ error: "method_not_allowed" });
    expect(methodDb.mutations).toHaveLength(0);
    expect(methodDb.queries).toHaveLength(0);

    const absent = fakeDb(BASE_SEED);
    const missing = await handleInstallationDeprovision(request(body(), { auth: null }), env(absent));
    expect(missing.status).toBe(401);
    expect(await missing.json()).toEqual({ error: "unauthorized" });
    const wrong = await handleInstallationDeprovision(
      request(body(), { auth: "wrong-bearer-value" }),
      env(absent),
    );
    expect(wrong.status).toBe(401);
    const wrongBody = await wrong.text();
    expect(wrongBody).toBe(JSON.stringify({ error: "unauthorized" }));
    expect(wrongBody).not.toContain(AUTH);
    expect(absent.mutations).toHaveLength(0);

    const unbound = fakeDb(BASE_SEED);
    const unavailable = await handleInstallationDeprovision(
      request(body()),
      { CONFIG_DB: unbound.binding },
    );
    expect(unavailable.status).toBe(503);
    expect(await unavailable.json()).toEqual({ error: "unavailable" });
    const short = await handleInstallationDeprovision(
      request(body()),
      env(unbound, "too-short"),
    );
    expect(short.status).toBe(503);
    expect(await short.json()).toEqual({ error: "unavailable" });
    expect(unbound.mutations).toHaveLength(0);
  });

  it("rejects malformed identifiers, request ids, repository scope, duplicates, and extra body fields", async () => {
    const invalidBodies: unknown[] = [
      { ...body(), request_id: "" },
      { ...body(), request_id: "bad request" },
      { ...body(), request_id: "x".repeat(129) },
      { ...body(), installation_id: "" },
      { ...body(), tenant_id: "" },
      { ...body(), repositories: "acme/web" },
      { ...body(), repositories: ["acme/web", "acme/web"] },
      { ...body(), repositories: ["https://github.com/acme/web"] },
      { ...body(), repositories: ["acme/team/web"] },
      { ...body(), repositories: ["acme/"] },
      { ...body(), repositories: [] },
      { ...body(), remove_installation: "true" },
      { ...body(), unexpected: "field" },
      { ...body(), repositories: [], remove_installation: true, unexpected: "field" },
    ];
    for (const invalid of invalidBodies) {
      const db = fakeDb(BASE_SEED);
      const response = await handleInstallationDeprovision(request(invalid), env(db));
      expect(response.status, JSON.stringify(invalid)).toBe(400);
      expect(db.mutations, JSON.stringify(invalid)).toHaveLength(0);
      expect(db.batchCalls, JSON.stringify(invalid)).toHaveLength(0);
    }

    const badJsonDb = fakeDb(BASE_SEED);
    const badJson = await handleInstallationDeprovision(
      request("{broken", { raw: true }),
      env(badJsonDb),
    );
    expect(badJson.status).toBe(400);
    expect(badJsonDb.mutations).toHaveLength(0);
  });

  it("rejects a cross-tenant installation before any write", async () => {
    const db = fakeDb({
      installations: [{ installation_id: "installation-01", tenant_id: "tenant-owner" }],
      repositories: [
        { tenant_id: "tenant-owner", repo_full_name: "acme/web" },
        { tenant_id: "tenant-attacker", repo_full_name: "acme/web" },
      ],
      tenants: [
        { tenant_id: "tenant-owner", primary_region: "sam" },
        { tenant_id: "tenant-attacker", primary_region: "weur" },
      ],
    });
    const before = db.snapshot();
    const response = await handleInstallationDeprovision(
      request(body({ tenant_id: "tenant-attacker" })),
      env(db),
    );
    expect(response.status).toBe(409);
    expect(db.snapshot()).toEqual(before);
    expect(db.mutations).toHaveLength(0);
    expect(db.batchCalls).toHaveLength(0);
  });

  it("requires CONFIG_DB and its atomic batch capability for destructive work", async () => {
    const noBinding = await handleInstallationDeprovision(
      request(body()),
      { CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH },
    );
    expect(noBinding.status).toBe(500);

    const withoutBatch = fakeDb(BASE_SEED, { hasBatch: false });
    const response = await handleInstallationDeprovision(
      request(body()),
      env(withoutBatch),
    );
    expect(response.status).toBe(500);
    expect(withoutBatch.snapshot()).toEqual({
      installations: BASE_SEED.installations,
      repositories: BASE_SEED.repositories,
      audits: [],
    });
    expect(withoutBatch.mutations).toHaveLength(0);
  });
});

describe("handleInstallationDeprovision — scoped mutation", () => {
  it("deletes only the exact requested tenant/repository pairs", async () => {
    const db = fakeDb({
      installations: [{ installation_id: "installation-01", tenant_id: "tenant-01" }],
      repositories: [
        { tenant_id: "tenant-01", repo_full_name: "acme/api" },
        { tenant_id: "tenant-01", repo_full_name: "acme/web" },
        { tenant_id: "tenant-02", repo_full_name: "acme/api" },
      ],
      tenants: [
        { tenant_id: "tenant-01", primary_region: "sam" },
        { tenant_id: "tenant-02", primary_region: "weur" },
      ],
    });
    const response = await handleInstallationDeprovision(
      request(body({ repositories: ["acme/api"] })),
      env(db),
    );
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({
      installation_id: "installation-01",
      repos_removed: 1,
      installation_removed: false,
      replayed: false,
    });
    expect(db.snapshot().repositories).toEqual([
      { tenant_id: "tenant-01", repo_full_name: "acme/web" },
      { tenant_id: "tenant-02", repo_full_name: "acme/api" },
    ]);
    expect(db.snapshot().installations).toEqual([
      { installation_id: "installation-01", tenant_id: "tenant-01" },
    ]);
  });

  it("retains the installation map and performs zero writes if removal would leave tenant repos", async () => {
    const db = fakeDb({
      installations: [{ installation_id: "installation-01", tenant_id: "tenant-01" }],
      repositories: [
        { tenant_id: "tenant-01", repo_full_name: "acme/api" },
        { tenant_id: "tenant-01", repo_full_name: "acme/web" },
      ],
      tenants: [{ tenant_id: "tenant-01", primary_region: "sam" }],
    });
    const before = db.snapshot();
    const response = await handleInstallationDeprovision(
      request(body({ remove_installation: true })),
      env(db),
    );
    expect(response.status).toBe(409);
    expect(db.snapshot()).toEqual(before);
    expect(db.mutations).toHaveLength(0);
    expect(db.batchCalls).toHaveLength(0);
  });

  it("removes the map only when explicitly requested and no tenant allowlist row remains", async () => {
    const retained = fakeDb(BASE_SEED);
    const keepMap = await handleInstallationDeprovision(
      request(body({ repositories: ["acme/web"], remove_installation: false })),
      env(retained),
    );
    expect(keepMap.status).toBe(200);
    expect(retained.snapshot().installations).toHaveLength(1);
    expect(retained.snapshot().repositories).toHaveLength(0);

    const removeMap = fakeDb({
      installations: [{ installation_id: "installation-01", tenant_id: "tenant-01" }],
      repositories: [
        { tenant_id: "tenant-01", repo_full_name: "acme/web" },
        { tenant_id: "tenant-02", repo_full_name: "other/repo" },
      ],
      tenants: [{ tenant_id: "tenant-01", primary_region: "sam" }],
    });
    const response = await handleInstallationDeprovision(
      request(body({ repositories: ["acme/web"], remove_installation: true })),
      env(removeMap),
    );
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({
      installation_id: "installation-01",
      repos_removed: 1,
      installation_removed: true,
      replayed: false,
    });
    expect(removeMap.snapshot().installations).toHaveLength(0);
    expect(removeMap.snapshot().repositories).toEqual([
      { tenant_id: "tenant-02", repo_full_name: "other/repo" },
    ]);

    const emptyScope = fakeDb({
      installations: [{ installation_id: "installation-01", tenant_id: "tenant-01" }],
      tenants: [{ tenant_id: "tenant-01", primary_region: "sam" }],
    });
    const emptyRemoval = await handleInstallationDeprovision(
      request(body({ repositories: [], remove_installation: true })),
      env(emptyScope),
    );
    expect(emptyRemoval.status).toBe(200);
    expect(await emptyRemoval.json()).toEqual({
      installation_id: "installation-01",
      repos_removed: 0,
      installation_removed: true,
      replayed: false,
    });
    expect(emptyScope.snapshot().installations).toHaveLength(0);
  });

  it("batches exact deletes with one primary-region-bound, deterministic sanitized audit intent", async () => {
    const db = fakeDb({
      installations: [{ installation_id: "installation-01", tenant_id: "tenant-01" }],
      repositories: [
        { tenant_id: "tenant-01", repo_full_name: "zeta/api" },
        { tenant_id: "tenant-01", repo_full_name: "alpha/web" },
        { tenant_id: "tenant-02", repo_full_name: "alpha/web" },
      ],
      tenants: [
        { tenant_id: "tenant-01", primary_region: "sam" },
        { tenant_id: "tenant-02", primary_region: "weur" },
      ],
    });
    const response = await handleInstallationDeprovision(
      request(body({ repositories: ["zeta/api", "alpha/web"] })),
      env(db),
    );
    expect(response.status).toBe(200);
    expect(db.batchCalls).toHaveLength(1);
    expect(db.batchCalls[0].filter((entry) => /delete from runner_repo_allowlist/i.test(entry.sql))).toHaveLength(2);
    expect(db.batchCalls[0].filter((entry) => /insert(?: or ignore)? into audit_outbox/i.test(entry.sql))).toHaveLength(1);
    expect(db.batchCalls[0].some((entry) => /tenant_gh_installation_map/i.test(entry.sql))).toBe(false);

    const audit = db.snapshot().audits;
    expect(audit).toHaveLength(1);
    expect(audit[0].tenant_id).toBe("tenant-01");
    expect(audit[0].request_id).toBe("deprov-req-01");
    expect(audit[0].event_type).toBe(EVENT);
    expect(audit[0].region).toBe("sam");
    const payload = JSON.parse(audit[0].payload_json) as Record<string, unknown>;
    const data = (payload.data ?? payload) as Record<string, unknown>;
    expect(data.installation_id).toBe("installation-01");
    expect(data.repos).toEqual(["alpha/web", "zeta/api"]);
    expect(data.remove_installation).toBe(false);
    expect(audit[0].payload_json).not.toContain(AUTH);
    expect(audit[0].payload_json).not.toContain(INSTALLATION_AUTH);
    expect(db.snapshot().repositories).toEqual([
      { tenant_id: "tenant-02", repo_full_name: "alpha/web" },
    ]);
  });

  it("returns deprovision_verify_failed when post-batch readback contradicts the requested deletes", async () => {
    const db = fakeDb(BASE_SEED, { ignoreRepoDeletes: true });
    const response = await handleInstallationDeprovision(
      request(body()),
      env(db),
    );
    expect(response.status).toBe(500);
    expect(await response.json()).toEqual({ error: "deprovision_verify_failed" });
    expect(db.batchCalls).toHaveLength(1);
    expect(db.snapshot().repositories).toEqual(BASE_SEED.repositories);

    const failedBatch = fakeDb(BASE_SEED, { failMutation: true });
    const before = failedBatch.snapshot();
    const failed = await handleInstallationDeprovision(request(body()), env(failedBatch));
    expect(failed.status).toBe(500);
    expect(failedBatch.snapshot()).toEqual(before);
    expect(failedBatch.batchCalls).toHaveLength(1);
  });
});

describe("handleInstallationDeprovision — idempotency", () => {
  it("replays the same request id and canonical payload without a second batch", async () => {
    const db = fakeDb(BASE_SEED);
    const original = body({ repositories: ["acme/web"] });
    const first = await handleInstallationDeprovision(request(original), env(db));
    expect(first.status).toBe(200);
    expect(await first.json()).toEqual({
      installation_id: "installation-01",
      repos_removed: 1,
      installation_removed: false,
      replayed: false,
    });
    const auditPayload = db.snapshot().audits[0].payload_json;

    // The map is intentionally retained by the first request. An exact replay
    // proves the immutable outbox entry governs idempotency, not a second D1 write.
    const replay = await handleInstallationDeprovision(request(original), env(db));
    expect(replay.status).toBe(200);
    expect(await replay.json()).toEqual({
      installation_id: "installation-01",
      repos_removed: 1,
      installation_removed: false,
      replayed: true,
    });
    expect(db.batchCalls).toHaveLength(1);
    expect(db.snapshot().audits).toHaveLength(1);
    expect(db.snapshot().audits[0].payload_json).toBe(auditPayload);

    const removed = fakeDb(BASE_SEED);
    const removing = body({ remove_installation: true });
    const removedFirst = await handleInstallationDeprovision(request(removing), env(removed));
    expect(removedFirst.status).toBe(200);
    expect(removed.snapshot().installations).toHaveLength(0);
    const removedReplay = await handleInstallationDeprovision(request(removing), env(removed));
    expect(removedReplay.status).toBe(200);
    expect(await removedReplay.json()).toEqual({
      installation_id: "installation-01",
      repos_removed: 1,
      installation_removed: true,
      replayed: true,
    });
    expect(removed.batchCalls).toHaveLength(1);
  });

  it("rejects a divergent request id payload and returns 404 for an unrecorded absent map", async () => {
    const db = fakeDb(BASE_SEED);
    const original = body({ remove_installation: true });
    const first = await handleInstallationDeprovision(request(original), env(db));
    expect(first.status).toBe(200);
    expect(db.snapshot().installations).toHaveLength(0);

    const beforeDivergent = db.snapshot();
    const divergent = await handleInstallationDeprovision(
      request(body({ repositories: ["other/repo"], remove_installation: true })),
      env(db),
    );
    expect(divergent.status).toBe(409);
    expect(db.snapshot()).toEqual(beforeDivergent);
    expect(db.batchCalls).toHaveLength(1);
    const absentMap = fakeDb({ tenants: [{ tenant_id: "tenant-01", primary_region: "sam" }] });
    const response = await handleInstallationDeprovision(request(body()), env(absentMap));
    expect(response.status).toBe(404);
    expect(absentMap.mutations).toHaveLength(0);
    expect(absentMap.batchCalls).toHaveLength(0);
    expect(absentMap.snapshot().audits).toHaveLength(0);
  });
});
