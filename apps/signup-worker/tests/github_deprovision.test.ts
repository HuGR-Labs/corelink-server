import { describe, expect, it } from "vitest";
import { route } from "../src/index.js";
import {
  handleInstallationDeprovision,
  type InstallationProvisionEnv,
} from "../src/webhooks/github_provision.js";

const AUTH = "a".repeat(64);
const PATH = "/internal/v1/runner/provision-installation";
const EVENT = "corelink.runner.installation_deprovision_requested";
type Row = Record<string, unknown>;
type Repo = { tenant_id: string; repo_full_name: string };
type Install = { installation_id: string; tenant_id: string };
type Audit = { id: string; tenant_id: string; request_id: string; event_type: string; payload_json: string; region: string };
type RecordSql = { sql: string; vals: unknown[] };
type Seed = { installs?: Install[]; repos?: Repo[]; regions?: Record<string, string>; audits?: Audit[] };
type Options = { failAtMutation?: number; ignoreRepoDelete?: boolean; noBatch?: boolean };
type Db = {
  binding: NonNullable<InstallationProvisionEnv["CONFIG_DB"]>;
  mutations: RecordSql[];
  batches: RecordSql[][];
  state(): { installs: Install[]; repos: Repo[]; audits: Audit[] };
};

const pairKey = (requestId: string, event: string) => `${requestId}\0${event}`;
const norm = (sql: string) => sql.replace(/\s+/g, " ").trim().toLowerCase();
function param(rec: RecordSql, column: string): string {
  const sql = norm(rec.sql);
  const re = new RegExp(`\\b${column}\\s*=\\s*\\?(\\d*)`, "i");
  const match = re.exec(sql);
  if (!match) throw new Error(`Unscoped SQL missing ${column}: ${rec.sql}`);
  const index = match[1]
    ? Number(match[1]) - 1
    : (sql.slice(0, match.index).match(/\?(?:\d+)?/g) ?? []).length;
  return String(rec.vals[index] ?? "");
}

/** Stateful D1 fake. Mutation SQL is scope-checked before binds are applied. */
function fake(seed: Seed = {}, opts: Options = {}): Db {
  let installs = new Map((seed.installs ?? []).map((r) => [r.installation_id, r.tenant_id]));
  let repos = (seed.repos ?? []).map((r) => ({ ...r }));
  let audits = new Map((seed.audits ?? []).map((r) => [pairKey(r.request_id, r.event_type), { ...r }]));
  const regions = new Map(Object.entries(seed.regions ?? {}));
  const mutations: RecordSql[] = [];
  const batches: RecordSql[][] = [];

  function rows(rec: RecordSql): Row[] {
    const sql = norm(rec.sql);
    if (sql.includes("from audit_outbox")) {
      const row = audits.get(pairKey(param(rec, "request_id"), param(rec, "event_type")));
      return row ? [{ ...row }] : [];
    }
    if (sql.includes("from tenant_gh_installation_map")) {
      const id = param(rec, "installation_id");
      const tenant = installs.get(id);
      return tenant ? [{ installation_id: id, tenant_id: tenant }] : [];
    }
    if (sql.includes("from runner_repo_allowlist")) {
      const tenant = param(rec, "tenant_id");
      let found = repos.filter((r) => r.tenant_id === tenant);
      if (/\brepo_full_name\s*=/.test(sql)) found = found.filter((r) => r.repo_full_name === param(rec, "repo_full_name"));
      if (/\bcount\s*\(/.test(sql)) return [{ count: found.length }];
      return found.map((r) => ({ ...r }));
    }
    if (sql.includes("from tenant")) {
      const tenant = param(rec, "tenant_id");
      const region = regions.get(tenant);
      return region ? [{ tenant_id: tenant, primary_region: region }] : [];
    }
    throw new Error(`Unexpected read: ${rec.sql}`);
  }

  function mutate(rec: RecordSql, nth: number): void {
    const sql = norm(rec.sql);
    mutations.push(rec);
    if (opts.failAtMutation === nth) throw new Error(`injected failure at ${nth}`);
    if (sql.startsWith("delete from runner_repo_allowlist")) {
      const where = sql.slice(sql.indexOf("where"));
      if (!/\btenant_id\s*=\s*\?\d*/.test(where) || !/\band\b/.test(where) || !/\brepo_full_name\s*=\s*\?\d*/.test(where)) {
        throw new Error(`Unscoped repo delete: ${rec.sql}`);
      }
      if (opts.ignoreRepoDelete) return;
      const tenant = param(rec, "tenant_id");
      const repo = param(rec, "repo_full_name");
      repos = repos.filter((r) => r.tenant_id !== tenant || r.repo_full_name !== repo);
      return;
    }
    if (sql.startsWith("delete from tenant_gh_installation_map")) {
      const where = sql.slice(sql.indexOf("where"));
      if (!/\binstallation_id\s*=\s*\?\d*/.test(where) || !/\btenant_id\s*=\s*\?\d*/.test(where) || !/not\s+exists/.test(where) || !where.includes("runner_repo_allowlist")) {
        throw new Error(`Unscoped/unguarded map delete: ${rec.sql}`);
      }
      const id = param(rec, "installation_id");
      const tenant = param(rec, "tenant_id");
      if ([...installs].some(([key, value]) => key === id && value === tenant) && !repos.some((r) => r.tenant_id === tenant)) installs.delete(id);
      return;
    }
    if (sql.startsWith("insert") && sql.includes("into audit_outbox")) {
      const columns = rec.sql.match(/into\s+audit_outbox\s*\(([^)]+)\)/i)?.[1];
      if (!columns) throw new Error(`Audit insert must name columns: ${rec.sql}`);
      const row = Object.fromEntries(columns.split(",").map((c, i) => [c.trim(), rec.vals[i]])) as Audit;
      audits.set(pairKey(row.request_id, row.event_type), row);
      return;
    }
    throw new Error(`Unexpected mutation: ${rec.sql}`);
  }

  const raw: Record<string, unknown> = {
    prepare(sql: string) {
      return { bind(...vals: unknown[]) {
        const rec = { sql, vals };
        const q = norm(sql).startsWith("select");
        const stmt = {
          first: async <T = Row>() => (rows(rec)[0] as T | undefined) ?? null,
          all: async <T = Row>() => ({ results: rows(rec) as T[] }),
          run: async () => { if (!q) mutate(rec, 1); return { success: true, meta: { changes: q ? 0 : 1 } }; },
        };
        Object.defineProperty(stmt, "record", { value: rec });
        return stmt;
      } };
    },
  };
  if (!opts.noBatch) {
    raw.batch = async (statements: Array<{ run(): Promise<unknown>; record?: RecordSql }>) => {
      const before = { installs: new Map(installs), repos: repos.map((r) => ({ ...r })), audits: new Map([...audits].map(([k, v]) => [k, { ...v }])) };
      batches.push(statements.map((s) => s.record!).filter(Boolean));
      try {
        const result = [];
        for (let i = 0; i < statements.length; i++) {
          const st = statements[i];
          // D1 batch runs prepared statements sequentially in one transaction.
          if (st.record && !norm(st.record.sql).startsWith("select")) mutate(st.record, i + 1);
          else result.push(await st.run());
          result.push({ success: true });
        }
        return result;
      } catch (error) {
        installs = before.installs; repos = before.repos; audits = before.audits;
        throw error;
      }
    };
  }
  return {
    binding: raw as unknown as Db["binding"], mutations, batches,
    state: () => ({
      installs: [...installs].map(([installation_id, tenant_id]) => ({ installation_id, tenant_id })),
      repos: repos.map((r) => ({ ...r })), audits: [...audits.values()].map((r) => ({ ...r })),
    }),
  };
}

const payload = (x: Record<string, unknown> = {}) => ({
  request_id: "req-1725", installation_id: "inst-1", tenant_id: "tenant-1",
  repositories: ["acme/web"], remove_installation: false, ...x,
});
function request(body: unknown, method = "DELETE", auth: string | null = AUTH): Request {
  const headers = new Headers({ "content-type": "application/json" });
  if (auth !== null) headers.set("authorization", `Bearer ${auth}`);
  return new Request(`https://signup.test${PATH}`, {
    method, headers,
    body: method === "GET" || method === "HEAD" ? undefined : JSON.stringify(body),
  });
}
const env = (db: Db, auth = AUTH): InstallationProvisionEnv => ({ CONFIG_DB: db.binding, CORELINK_RUNNER_PROVISION_AUTH_KEY: auth });
const seed: Seed = {
  installs: [{ installation_id: "inst-1", tenant_id: "tenant-1" }, { installation_id: "inst-2", tenant_id: "tenant-1" }],
  repos: [
    { tenant_id: "tenant-1", repo_full_name: "acme/api" },
    { tenant_id: "tenant-1", repo_full_name: "acme/web" },
    { tenant_id: "tenant-2", repo_full_name: "acme/api" },
  ],
  regions: { "tenant-1": "sam", "tenant-2": "weur" },
};
async function sha256(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

describe("#1725 installation deprovision contract", () => {
  it("routes DELETE on the canonical path through the exported Worker route", async () => {
    const db = fake(seed);
    const response = await route(request(payload()), env(db), {} as ExecutionContext);
    expect(response.status).toBe(200);
    expect(db.state().repos).not.toContainEqual({ tenant_id: "tenant-1", repo_full_name: "acme/web" });
  });

  it("pins method, auth and database fail-closed statuses", async () => {
    const db = fake(seed);
    const wrongMethod = await handleInstallationDeprovision(request(payload(), "POST", null), env(db));
    expect(wrongMethod.status).toBe(405);
    const missing = await handleInstallationDeprovision(request(payload(), "DELETE", null), env(db));
    expect(missing.status).toBe(401);
    expect(await missing.json()).toEqual({ error: "unauthorized" });
    const wrong = await handleInstallationDeprovision(request(payload(), "DELETE", "bad"), env(db));
    expect(wrong.status).toBe(401);
    const wrongText = await wrong.text();
    expect(wrongText).toBe(JSON.stringify({ error: "unauthorized" }));
    expect(wrongText).not.toContain(AUTH);
    expect((await handleInstallationDeprovision(request(payload()), { CONFIG_DB: db.binding })).status).toBe(503);
    expect((await handleInstallationDeprovision(request(payload()), env(db, "short"))).status).toBe(503);
    expect((await handleInstallationDeprovision(request(payload()), { CORELINK_RUNNER_PROVISION_AUTH_KEY: AUTH })).status).toBe(500);
    const noBatch = fake(seed, { noBatch: true });
    expect((await handleInstallationDeprovision(request(payload()), env(noBatch))).status).toBe(500);
    expect(noBatch.mutations).toHaveLength(0);
    expect(db.mutations).toHaveLength(0);
  });

  it("rejects malformed or non-canonical bodies before writes", async () => {
    const invalid = [
      { ...payload(), request_id: "" }, { ...payload(), request_id: "bad id" },
      { ...payload(), request_id: "r".repeat(129) }, { ...payload(), installation_id: "" },
      { ...payload(), tenant_id: "" }, { ...payload(), repositories: "acme/web" },
      { ...payload(), repositories: ["acme/web", "acme/web"] },
      { ...payload(), repositories: ["acme/team/repo"] }, { ...payload(), repositories: [] },
      { ...payload(), remove_installation: "true" }, { ...payload(), extra: 1 },
      (({ remove_installation: _remove, ...rest }) => { void _remove; return rest; })(payload()),
    ];
    for (const input of invalid) {
      const db = fake(seed);
      expect((await handleInstallationDeprovision(request(input), env(db))).status).toBe(400);
      expect(db.mutations).toHaveLength(0);
      expect(db.batches).toHaveLength(0);
    }
    const empty = fake({ installs: seed.installs, regions: seed.regions });
    expect((await handleInstallationDeprovision(request(payload({ repositories: [], remove_installation: true })), env(empty))).status).toBe(200);
  });

  it("deletes exact tenant/repository pairs and rejects cross-tenant map access", async () => {
    const db = fake(seed);
    const response = await handleInstallationDeprovision(request(payload({ repositories: ["acme/api"] })), env(db));
    expect(response.status).toBe(200);
    expect(db.state().repos).toEqual([
      { tenant_id: "tenant-1", repo_full_name: "acme/web" },
      { tenant_id: "tenant-2", repo_full_name: "acme/api" },
    ]);
    expect(db.state().installs).toContainEqual({ installation_id: "inst-2", tenant_id: "tenant-1" });
    expect(db.batches[0].filter((r) => /delete from runner_repo_allowlist/i.test(r.sql))).toHaveLength(1);
    const beforeWrongTenant = db.state();
    const wrongTenant = await handleInstallationDeprovision(request(payload({ tenant_id: "tenant-2" })), env(db));
    expect(wrongTenant.status).toBe(409);
    expect(db.state()).toEqual(beforeWrongTenant);
    expect(db.batches).toHaveLength(1);
  });

  it("keeps maps by default; explicit removal requires zero residual rows and is installation-scoped", async () => {
    const db = fake(seed);
    const before = db.state();
    const blocked = await handleInstallationDeprovision(request(payload({ remove_installation: true })), env(db));
    expect(blocked.status).toBe(409);
    expect(db.state()).toEqual(before);
    expect(db.batches).toHaveLength(0);
    expect(db.mutations).toHaveLength(0);
    const cleared = fake({
      installs: [{ installation_id: "inst-1", tenant_id: "tenant-1" }, { installation_id: "inst-2", tenant_id: "tenant-1" }],
      repos: [{ tenant_id: "tenant-1", repo_full_name: "acme/web" }], regions: seed.regions,
    });
    const done = await handleInstallationDeprovision(request(payload({ remove_installation: true })), env(cleared));
    expect(done.status).toBe(200);
    expect(cleared.state().installs).toEqual([{ installation_id: "inst-2", tenant_id: "tenant-1" }]);
    const mapDelete = cleared.batches[0].find((r) => /delete from tenant_gh_installation_map/i.test(r.sql));
    expect(mapDelete?.sql).toMatch(/where[^;]*installation_id\s*=\s*\?\d*[^;]*and[^;]*tenant_id\s*=\s*\?\d*[^;]*not\s+exists[^;]*runner_repo_allowlist/i);
    expect(param(mapDelete!, "installation_id")).toBe("inst-1");
    expect(param(mapDelete!, "tenant_id")).toBe("tenant-1");
  });

  it("writes one atomic audit row with the canonical payload and tenant region", async () => {
    const db = fake({ ...seed, repos: [
      { tenant_id: "tenant-1", repo_full_name: "zeta/api" },
      { tenant_id: "tenant-1", repo_full_name: "alpha/web" },
      { tenant_id: "tenant-2", repo_full_name: "alpha/web" },
    ] });
    const repos = ["zeta/api", "alpha/web"];
    expect((await handleInstallationDeprovision(request(payload({ repositories: repos })), env(db))).status).toBe(200);
    expect(db.batches).toHaveLength(1);
    const deletes = db.batches[0].filter((r) => /delete from runner_repo_allowlist/i.test(r.sql));
    expect(deletes).toHaveLength(2);
    for (const d of deletes) {
      expect(d.sql).toMatch(/where[^;]*tenant_id\s*=\s*\?\d*[^;]*and[^;]*repo_full_name\s*=\s*\?\d*/i);
      expect(param(d, "tenant_id")).toBe("tenant-1");
      expect(repos).toContain(param(d, "repo_full_name"));
    }
    const [audit] = db.state().audits;
    expect(audit.event_type).toBe(EVENT);
    expect(audit.request_id).toBe("req-1725");
    expect(audit.tenant_id).toBe("tenant-1");
    expect(audit.region).toBe("sam");
    const parsed = JSON.parse(audit.payload_json) as Record<string, unknown>;
    expect(Object.keys(parsed).sort()).toEqual(["installation_id", "remove_installation", "repositories_sha256", "repository_count"].sort());
    expect(parsed).toEqual({
      installation_id: "inst-1", repositories_sha256: await sha256(JSON.stringify(["alpha/web", "zeta/api"])),
      repository_count: 2, remove_installation: false,
    });
    expect(audit.payload_json).not.toMatch(/acme|alpha|zeta|tenant-1|Bearer|authorization/i);
    expect(audit.payload_json).not.toContain("req-1725");
    expect(audit.payload_json).not.toContain(AUTH);
  });

  it("rolls back every mutation when batch statement two fails", async () => {
    const db = fake(seed, { failAtMutation: 2 });
    const before = db.state();
    const response = await handleInstallationDeprovision(request(payload({ repositories: ["acme/api", "acme/web"], remove_installation: true })), env(db));
    expect(response.status).toBe(500);
    expect(db.state()).toEqual(before);
    expect(db.mutations).toHaveLength(2);
    expect(db.batches).toHaveLength(1);
  });

  it("fails verification when a deleted allowlist row remains after the batch", async () => {
    const db = fake(seed, { ignoreRepoDelete: true });
    const response = await handleInstallationDeprovision(request(payload()), env(db));
    expect(response.status).toBe(500);
    expect(await response.json()).toEqual({ error: "deprovision_verify_failed" });
    expect(db.state().repos).toContainEqual({ tenant_id: "tenant-1", repo_full_name: "acme/web" });
  });

  it("replays canonical digest identity once, rejects divergent payload, and requires audit for absent maps", async () => {
    const db = fake({ ...seed, repos: [
      { tenant_id: "tenant-1", repo_full_name: "zeta/api" },
      { tenant_id: "tenant-1", repo_full_name: "alpha/web" },
    ] });
    const firstBody = payload({ repositories: ["zeta/api", "alpha/web"], remove_installation: true });
    expect((await handleInstallationDeprovision(request(firstBody), env(db))).status).toBe(200);
    const replay = await handleInstallationDeprovision(request({ ...firstBody, repositories: [...firstBody.repositories].reverse() }), env(db));
    expect(replay.status).toBe(200);
    expect((await replay.json()).replayed).toBe(true);
    expect(db.batches).toHaveLength(1);
    const conflict = await handleInstallationDeprovision(request(payload({ repositories: ["other/repo"], remove_installation: true })), env(db));
    expect(conflict.status).toBe(409);
    const absent = fake({ regions: seed.regions });
    expect((await handleInstallationDeprovision(request(payload()), env(absent))).status).toBe(404);
    expect(absent.mutations).toHaveLength(0);
  });
});
