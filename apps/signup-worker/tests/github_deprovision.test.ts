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
type Options = { failAtMutation?: number; ignoreRepoDelete?: boolean; ignoreMapDelete?: boolean; noBatch?: boolean; provisionRepoBeforeBatch?: Repo };
type Db = {
  binding: NonNullable<InstallationProvisionEnv["CONFIG_DB"]>;
  mutations: RecordSql[];
  batches: RecordSql[][];
  provision(repo: Repo): void;
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
  let activeBatch: RecordSql[] = [];
  let injected = false;

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
      const legacySql = "delete from runner_repo_allowlist where tenant_id = ?1 and repo_full_name = ?2";
      const keepSql = "delete from runner_repo_allowlist where tenant_id = ?1 and repo_full_name = ?2 and exists (select 1 from tenant_gh_installation_map where installation_id = ?3 and tenant_id = ?1)";
      const removeSql = "delete from runner_repo_allowlist where tenant_id = ?1 and repo_full_name = ?2 and not exists (select 1 from tenant_gh_installation_map where installation_id = ?3 and tenant_id = ?1)";
      const removeMapIndex = activeBatch.findIndex((r) => norm(r.sql).startsWith("delete from tenant_gh_installation_map"));
      if (sql !== legacySql && sql !== keepSql && sql !== removeSql) throw new Error(`Unscoped repo delete: ${rec.sql}`);
      const guardedMapDelete = removeMapIndex >= 0 && norm(activeBatch[removeMapIndex]!.sql).includes("repo_full_name not in");
      if (guardedMapDelete && activeBatch.indexOf(rec) < removeMapIndex) throw new Error("Map delete must precede repo deletes");
      if (opts.ignoreRepoDelete) return;
      const tenant = param(rec, "tenant_id");
      const repo = param(rec, "repo_full_name");
      const install = String(rec.vals[2] ?? "");
      const mapped = installs.get(install) === tenant;
      if (sql !== legacySql && ((sql === keepSql && !mapped) || (sql === removeSql && mapped))) return;
      repos = repos.filter((r) => r.tenant_id !== tenant || r.repo_full_name !== repo);
      return;
    }
    if (sql.startsWith("delete from tenant_gh_installation_map")) {
      const requested = sql.includes("json_each(?3)")
        ? (JSON.parse(String(rec.vals[2])) as string[])
        : rec.vals.slice(2).map(String);
      const notIn = requested.length ? ` and repo_full_name not in (${requested.map((_, i) => `?${i + 3}`).join(", ")})` : "";
      const legacySql = "delete from tenant_gh_installation_map where installation_id = ?1 and tenant_id = ?2 and not exists (select 1 from runner_repo_allowlist where tenant_id = ?2)";
      const guardedSql = `delete from tenant_gh_installation_map where installation_id = ?1 and tenant_id = ?2 and not exists (select 1 from runner_repo_allowlist where tenant_id = ?2${notIn})`;
      const jsonGuardedSql = "delete from tenant_gh_installation_map where installation_id = ?1 and tenant_id = ?2 and not exists (select 1 from runner_repo_allowlist where tenant_id = ?2 and repo_full_name not in (select cast(value as text) from json_each(?3)))";
      if (sql !== legacySql && sql !== guardedSql && sql !== jsonGuardedSql) throw new Error(`Unscoped/unguarded map delete: ${rec.sql}`);
      if (opts.ignoreMapDelete) return;
      const id = param(rec, "installation_id");
      const tenant = param(rec, "tenant_id");
      const requestedSet = new Set(requested);
      const residual = repos.some((r) => r.tenant_id === tenant && (sql === legacySql || !requestedSet.has(r.repo_full_name)));
      if (installs.get(id) === tenant && !residual) installs.delete(id);
      return;
    }
    if (sql.startsWith("insert") && sql.includes("into audit_outbox")) {
      const guardedAudit = /^insert into audit_outbox\s*\([^)]*\) select /.test(sql) && sql.includes(" where ");
      const legacyAudit = /^insert into audit_outbox\s*\([^)]*\) values\s*\(/.test(sql);
      if (sql.startsWith("insert or ignore") || (!guardedAudit && !legacyAudit)) throw new Error(`Unexpected audit insert: ${rec.sql}`);
      if (guardedAudit) {
        const predicate = sql.slice(sql.indexOf(" where "));
        if (!predicate.includes("tenant_gh_installation_map") || !predicate.includes("runner_repo_allowlist")) throw new Error(`Audit lacks live-state guards: ${rec.sql}`);
      }
      const columns = rec.sql.match(/into\s+audit_outbox\s*\(([^)]+)\)/i)?.[1];
      if (!columns) throw new Error(`Audit insert must name columns: ${rec.sql}`);
      const row = Object.fromEntries(columns.split(",").map((c, i) => [c.trim(), rec.vals[i]])) as Audit;
      const mapDelete = activeBatch.find((r) => norm(r.sql).startsWith("delete from tenant_gh_installation_map"));
      const remove = Boolean(mapDelete);
      const install = String(mapDelete?.vals[0] ?? activeBatch.find((r) => norm(r.sql).startsWith("delete from runner_repo_allowlist"))?.vals[2] ?? "");
      const tenant = String(row.tenant_id);
      const requested = activeBatch.filter((r) => norm(r.sql).startsWith("delete from runner_repo_allowlist") && String(r.vals[0]) === tenant).map((r) => String(r.vals[1]));
      const requestedRemain = requested.some((repo) => repos.some((r) => r.tenant_id === tenant && r.repo_full_name === repo));
      const mapRemains = installs.get(install) === tenant;
      const tenantResidue = repos.some((r) => r.tenant_id === tenant);
      if (guardedAudit && (requestedRemain || (remove ? mapRemains || tenantResidue : !mapRemains))) return;
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
      if (statements.some((statement) => (statement.record?.vals.length ?? 0) > 100)) {
        throw new Error("D1 bound-parameter limit exceeded");
      }
      if (!injected && opts.provisionRepoBeforeBatch) { repos.push({ ...opts.provisionRepoBeforeBatch }); injected = true; }
      const before = { installs: new Map(installs), repos: repos.map((r) => ({ ...r })), audits: new Map([...audits].map(([k, v]) => [k, { ...v }])) };
      activeBatch = statements.map((s) => s.record!).filter(Boolean);
      batches.push(activeBatch);
      try {
        const result = [];
        for (let i = 0; i < statements.length; i++) {
          const st = statements[i]!;
          // D1 batch runs prepared statements sequentially in one transaction.
          if (st.record && !norm(st.record.sql).startsWith("select")) mutate(st.record, i + 1);
          else result.push(await st.run());
          result.push({ success: true });
        }
        return result;
      } catch (error) {
        installs = before.installs; repos = before.repos; audits = before.audits;
        throw error;
      } finally {
        activeBatch = [];
      }
    };
  }
  return {
    binding: raw as unknown as Db["binding"], mutations, batches,
    provision: (repo) => { repos.push({ ...repo }); },
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
    const sharedOnly = {
      CONFIG_DB: db.binding,
      CORELINK_INTERNAL_AUTH_KEY: AUTH,
    } as InstallationProvisionEnv & { CORELINK_INTERNAL_AUTH_KEY: string };
    expect((await handleInstallationDeprovision(request(payload()), sharedOnly)).status).toBe(503);
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
    const batch = db.batches[0]!;
    expect(batch.filter((r) => /delete from runner_repo_allowlist/i.test(r.sql))).toHaveLength(1);
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
    const batch = cleared.batches[0]!;
    const mapDelete = batch.find((r) => /delete from tenant_gh_installation_map/i.test(r.sql));
    expect(["delete from tenant_gh_installation_map where installation_id = ?1 and tenant_id = ?2 and not exists (select 1 from runner_repo_allowlist where tenant_id = ?2)", "delete from tenant_gh_installation_map where installation_id = ?1 and tenant_id = ?2 and not exists (select 1 from runner_repo_allowlist where tenant_id = ?2 and repo_full_name not in (?3))", "delete from tenant_gh_installation_map where installation_id = ?1 and tenant_id = ?2 and not exists (select 1 from runner_repo_allowlist where tenant_id = ?2 and repo_full_name not in (select cast(value as text) from json_each(?3)))"]).toContain(norm(mapDelete!.sql));
    expect(param(mapDelete!, "installation_id")).toBe("inst-1");
    expect(param(mapDelete!, "tenant_id")).toBe("tenant-1");
    if (norm(mapDelete!.sql).includes("repo_full_name not in")) {
      expect(batch.indexOf(mapDelete!)).toBeLessThan(batch.findIndex((r) => /delete from runner_repo_allowlist/i.test(r.sql)));
    }
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
    const batch = db.batches[0]!;
    const deletes = batch.filter((r) => /delete from runner_repo_allowlist/i.test(r.sql));
    expect(deletes).toHaveLength(2);
    for (const d of deletes) {
      expect(["delete from runner_repo_allowlist where tenant_id = ?1 and repo_full_name = ?2", "delete from runner_repo_allowlist where tenant_id = ?1 and repo_full_name = ?2 and exists (select 1 from tenant_gh_installation_map where installation_id = ?3 and tenant_id = ?1)"]).toContain(norm(d.sql));
      expect(param(d, "tenant_id")).toBe("tenant-1");
      expect(repos).toContain(param(d, "repo_full_name"));
      if (norm(d.sql).includes("tenant_gh_installation_map")) expect(d.vals[2]).toBe("inst-1");
    }
    const audit = db.state().audits[0]!;
    expect(audit.event_type).toBe(EVENT);
    expect(audit.request_id).toBe("req-1725");
    expect(audit.tenant_id).toBe("tenant-1");
    expect(audit.region).toBe("sam");
    const envelope = JSON.parse(audit.payload_json) as Record<string, unknown>;
    expect(Object.keys(envelope).sort()).toEqual(["specversion", "id", "source", "type", "subject", "time", "data"].sort());
    expect(envelope.specversion).toBe("1.0");
    expect(envelope.type).toBe(EVENT);
    expect(envelope.source).toBe("corelink-signup-worker");
    expect(typeof envelope.id).toBe("string");
    expect(String(envelope.id).length).toBeGreaterThan(0);
    expect(String(envelope.id)).not.toContain("tenant-1");
    expect(String(envelope.id)).not.toContain(AUTH);
    expect(typeof envelope.subject).toBe("string");
    expect(String(envelope.subject)).toContain("inst-1");
    expect(String(envelope.subject)).not.toContain("tenant-1");
    expect(new Date(String(envelope.time)).toISOString()).toBe(envelope.time);
    const data = envelope.data as Record<string, unknown>;
    expect(Object.keys(data).sort()).toEqual(["installation_id", "remove_installation", "repositories_sha256", "repository_count"].sort());
    expect(data).toEqual({
      installation_id: "inst-1", repositories_sha256: await sha256(JSON.stringify(["alpha/web", "zeta/api"])),
      repository_count: 2, remove_installation: false,
    });
    expect(audit.payload_json).not.toMatch(/acme|alpha|zeta|tenant-1|Bearer|authorization/i);
    expect(audit.payload_json).not.toContain("req-1725");
    expect(audit.payload_json).not.toContain(AUTH);
  });

  it("keeps every D1 statement under the 100 bound-parameter limit", async () => {
    const repositories = Array.from({ length: 120 }, (_, index) => `acme/repo-${index}`);
    const db = fake({
      installs: [{ installation_id: "inst-1", tenant_id: "tenant-1" }],
      repos: repositories.map((repo_full_name) => ({ tenant_id: "tenant-1", repo_full_name })),
      regions: seed.regions,
    });
    const response = await handleInstallationDeprovision(
      request(payload({ repositories, remove_installation: true })),
      env(db),
    );
    expect(response.status).toBe(200);
    expect(db.batches[0]!.every((statement) => statement.vals.length <= 100)).toBe(true);
    expect(db.state().repos).toHaveLength(0);
    expect(db.state().installs).toHaveLength(0);
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
    const mapDb = fake({ installs: [{ installation_id: "inst-1", tenant_id: "tenant-1" }], repos: [{ tenant_id: "tenant-1", repo_full_name: "acme/web" }], regions: seed.regions }, { ignoreMapDelete: true });
    const mapResponse = await handleInstallationDeprovision(request(payload({ remove_installation: true })), env(mapDb));
    expect(mapResponse.status).toBe(500);
    expect(await mapResponse.json()).toEqual({ error: "deprovision_verify_failed" });
    expect(mapDb.state().installs).toEqual([{ installation_id: "inst-1", tenant_id: "tenant-1" }]);
  });

  it("replays canonical digest identity once, rejects divergent payload, and requires audit for absent maps", async () => {
    const db = fake({ ...seed, repos: [
      { tenant_id: "tenant-1", repo_full_name: "zeta/api" },
      { tenant_id: "tenant-1", repo_full_name: "alpha/web" },
    ] });
    const firstBody = payload({ repositories: ["zeta/api", "alpha/web"], remove_installation: true });
    expect((await handleInstallationDeprovision(request(firstBody), env(db))).status).toBe(200);
    const firstAudit = db.state().audits[0]!;
    const firstData = (JSON.parse(firstAudit.payload_json) as { data: unknown }).data;
    const mutationsAfterFirst = db.mutations.length;
    const replay = await handleInstallationDeprovision(request({ ...firstBody, repositories: [...firstBody.repositories].reverse() }), env(db));
    expect(replay.status).toBe(200);
    const replayBody = await replay.json() as { replayed: unknown };
    expect(replayBody.replayed).toBe(true);
    const replayAudit = db.state().audits[0]!;
    expect((JSON.parse(replayAudit.payload_json) as { data: unknown }).data).toEqual(firstData);
    expect(db.batches).toHaveLength(1);
    expect(db.mutations).toHaveLength(mutationsAfterFirst);
    const beforeConflict = db.state();
    const batchesBeforeConflict = db.batches.map((batch) => [...batch]);
    const conflict = await handleInstallationDeprovision(request(payload({ repositories: ["other/repo"], remove_installation: true })), env(db));
    expect(conflict.status).toBe(409);
    expect(db.state()).toEqual(beforeConflict);
    expect(db.batches).toEqual(batchesBeforeConflict);
    expect(db.mutations).toHaveLength(mutationsAfterFirst);
    const absent = fake({ regions: seed.regions });
    expect((await handleInstallationDeprovision(request(payload()), env(absent))).status).toBe(404);
    expect(absent.mutations).toHaveLength(0);
  });

  it("rejects a matching replay when live repository state contradicts its audit", async () => {
    const db = fake({ ...seed, repos: [{ tenant_id: "tenant-1", repo_full_name: "acme/web" }] }), body = payload({ remove_installation: true });
    expect((await handleInstallationDeprovision(request(body), env(db))).status).toBe(200);
    db.provision({ tenant_id: "tenant-1", repo_full_name: "acme/new" }); const batches = db.batches.length;
    expect((await handleInstallationDeprovision(request(body), env(db))).status).toBe(500);
    expect(db.batches).toHaveLength(batches);
    expect(db.state().repos).toContainEqual({ tenant_id: "tenant-1", repo_full_name: "acme/new" });
  });

  it("keeps both repositories and the map when a new repo races final installation removal", async () => {
    const db = fake({ installs: [{ installation_id: "inst-1", tenant_id: "tenant-1" }], repos: [{ tenant_id: "tenant-1", repo_full_name: "acme/web" }], regions: seed.regions },
      { provisionRepoBeforeBatch: { tenant_id: "tenant-1", repo_full_name: "acme/new" } }), body = payload({ remove_installation: true });
    expect((await handleInstallationDeprovision(request(body), env(db))).status).toBe(500);
    expect(db.state()).toEqual({ installs: [{ installation_id: "inst-1", tenant_id: "tenant-1" }], repos: [
      { tenant_id: "tenant-1", repo_full_name: "acme/web" }, { tenant_id: "tenant-1", repo_full_name: "acme/new" },
    ], audits: [] });
    const retry = await handleInstallationDeprovision(request(body), env(db));
    expect([409, 500]).toContain(retry.status); expect(retry.status).not.toBe(200); expect(db.state().audits).toHaveLength(0);
  });
});

// This unit suite cannot perform the live canary; keep it as a separate runbook/done gate.
