import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { describe, expect, it, vi } from "vitest";
import { drainRunnerOperations, prepareRunnerOperation, RUNNER_PREPARE_MS } from "../src/lib/runner_credential_obligation";

const operation = { operationId: "11111111-1111-4111-8111-111111111111", tenantId: "tenant-a", jobId: "job-a", repo: "acme/repo", lifecycleGeneration: "1" };
const key = "runner-credential/11111111-1111-4111-8111-111111111111";
const marker = (overrides: Record<string, unknown> = {}) => ({ schema_version: 1, ...operation, deadline_ms: 91_000, due: 91_000, attempts: 0, ...overrides });
const operationAt = (n: number) => ({ ...operation, operationId: `11111111-1111-4111-8111-${String(n).padStart(12, "0")}` });

const { DatabaseSync } = createRequire(import.meta.url)("node:sqlite") as typeof import("node:sqlite");
class RealD1 {
  readonly sqlite = new DatabaseSync(":memory:");
  constructor() {
    this.sqlite.exec("CREATE TABLE runner_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT, job_id TEXT, repo TEXT, state TEXT, deadline_ms INTEGER, pat_id TEXT, token_id TEXT, lifecycle_generation TEXT); CREATE TABLE tenant_credential_revocation_floor (tenant_id TEXT PRIMARY KEY, revoked_through TEXT); CREATE TABLE pat (pat_id TEXT PRIMARY KEY, tenant_id TEXT, token_id TEXT);");
  }
  prepare(sql: string) {
    const db = this.sqlite;
    return { bind: (...args: unknown[]) => { const normalized = sql.replace(/\?(\d+)/g, "?"); const values = [...sql.matchAll(/\?(\d+)/g)].map((m) => args[Number(m[1]) - 1]); const statement = db.prepare(normalized); return { run: async () => { const result = statement.run(...values); return { meta: { changes: Number(result.changes) } }; }, first: async <T>() => statement.get(...values) as T | undefined ?? null }; } };
  }
}

function storage() {
  const map = new Map<string, unknown>(); let alarm: number | null = null;
  const io = {
    put: vi.fn(async (entryKey: string, value: unknown) => { map.set(entryKey, value); }),
    get: vi.fn(async (entryKey: string) => map.get(entryKey)),
    delete: vi.fn(async (entryKey: string) => { map.delete(entryKey); }),
    list: vi.fn(async ({ prefix, limit }: { prefix: string; limit: number }) => new Map([...map].filter(([entryKey]) => entryKey.startsWith(prefix)).slice(0, limit))),
    getAlarm: vi.fn(async () => alarm), setAlarm: vi.fn(async (value: number) => { alarm = value; }),
    transaction: vi.fn(async (fn: (txn: typeof io) => Promise<unknown>) => {
      const before = new Map(map); const alarmBefore = alarm;
      try { return await fn(io); } catch (error) { map.clear(); before.forEach((value, entryKey) => map.set(entryKey, value)); alarm = alarmBefore; throw error; }
    }),
  };
  return { map, alarm: () => alarm, storage: io as unknown as DurableObjectStorage, io };
}

function dbMock() {
  const run = vi.fn(async () => ({ success: true, results: [], meta: { changes: 1 } }));
  const first = vi.fn(async () => null);
  const prepare = vi.fn((sql: string) => ({ bind: (...args: unknown[]) => ({ run: () => run(sql, args), first: () => first(sql, args) }), sql }));
  const batch = vi.fn(async (_statements: unknown[]) => [1, 2, 3].map(() => ({ success: true, results: [], meta: { changes: 1 } })));
  return { run, first, prepare, batch, db: { prepare, batch } as unknown as D1Database };
}

describe("runner credential obligation issuer handoff", () => {
  it.each(["1", "9223372036854775807"])("rejects a prepared generation at revocation floor %s before any PAT persistence", async (floor) => {
    const store = storage(); const db = new RealD1();
    db.sqlite.prepare("INSERT INTO tenant_credential_revocation_floor VALUES (?, ?)").run(operation.tenantId, floor);
    const result = await prepareRunnerOperation(store.storage, db as never, { ...operation, lifecycleGeneration: floor }, 1000);
    expect(result).toBe(false);
    expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM runner_credential_obligation").get()?.n).toBe(0);
    expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM pat").get()?.n).toBe(0);
  });

  it("arms a durable marker and alarm before preparing the D1 obligation", async () => {
    const store = storage(); const db = dbMock();
    await expect(prepareRunnerOperation(store.storage, db.db, operation, 1000)).resolves.toBe(true);
    expect(store.map.get(key)).toEqual(marker({ deadline_ms: 91_000, due: 91_000 }));
    expect(store.alarm()).toBe(1000 + RUNNER_PREPARE_MS);
  });

  it("rolls storage back and never touches D1 when the marker write fails", async () => {
    const store = storage(); const db = dbMock(); store.io.put.mockRejectedValueOnce(new Error("storage unavailable"));
    await expect(prepareRunnerOperation(store.storage, db.db, operation, 1000)).rejects.toThrow("storage unavailable");
    expect(store.map).toEqual(new Map()); expect(store.alarm()).toBeNull(); expect(db.run).not.toHaveBeenCalled();
  });

  it("rejects a new operation at the 500-marker cap before D1", async () => {
    const store = storage(); const db = dbMock();
    for (let i = 0; i < 500; i++) { const op = operationAt(i + 1); store.map.set(`runner-credential/${op.operationId}`, marker({ ...op })); }
    await expect(prepareRunnerOperation(store.storage, db.db, operation, 1000)).rejects.toThrow("runner credential capacity exhausted");
    expect(db.run).not.toHaveBeenCalled();
  });

  it("reuses an exact marker at capacity with its original immutable deadline", async () => {
    const store = storage(); const db = dbMock();
    store.map.set(key, marker({ deadline_ms: 50_000, due: 1234, attempts: 2 }));
    for (let i = 0; i < 499; i++) { const op = operationAt(i + 1); store.map.set(`runner-credential/${op.operationId}`, marker({ ...op })); }
    db.run.mockResolvedValueOnce({ success: true, results: [], meta: { changes: 0 } });
    db.first.mockResolvedValueOnce({ tenant_id: operation.tenantId, job_id: operation.jobId, repo: operation.repo, state: "prepared", deadline_ms: 50_000 });
    await expect(prepareRunnerOperation(store.storage, db.db, operation, 1000)).resolves.toBe(true);
    expect(store.map.get(key)).toEqual(marker({ deadline_ms: 50_000, due: 1234, attempts: 2 }));
    expect(db.run.mock.calls[0]?.[1]).toContain(50_000);
    expect(store.alarm()).toBe(1234);
  });

  it("rejects expired durable preparation before D1 and never re-arms it", async () => {
    const store = storage(); const db = dbMock(); store.map.set(key, marker({ deadline_ms: 1000, due: 9999 }));
    await expect(prepareRunnerOperation(store.storage, db.db, operation, 1000)).resolves.toBe(false);
    expect(db.run).not.toHaveBeenCalled(); expect(store.alarm()).toBeNull();
  });

  it("requires a full-tuple, same-deadline D1 readback when the insert is uncertain", async () => {
    const store = storage(); const db = dbMock(); db.run.mockResolvedValue({ success: true, results: [], meta: { changes: 0 } });
    db.first.mockResolvedValueOnce({ tenant_id: operation.tenantId, job_id: operation.jobId, repo: operation.repo, state: "prepared", deadline_ms: 90_999 });
    await expect(prepareRunnerOperation(store.storage, db.db, operation, 1000)).resolves.toBe(false);
    db.first.mockResolvedValueOnce({ tenant_id: operation.tenantId, job_id: "other-job", repo: operation.repo, state: "prepared", deadline_ms: 91_000 });
    await expect(prepareRunnerOperation(store.storage, db.db, operation, 1000)).resolves.toBe(false);
    db.first.mockResolvedValueOnce({ tenant_id: operation.tenantId, job_id: operation.jobId, repo: operation.repo, state: "prepared", deadline_ms: 91_000 });
    await expect(prepareRunnerOperation(store.storage, db.db, operation, 1000)).resolves.toBe(true);
  });

  it("retains a pending issued credential when metadata KV is unavailable", async () => {
    const store = storage(); const db = dbMock(); store.map.set(key, marker({ due: 1 }));
    db.first.mockResolvedValue({ ...operation, state: "revoking", deadline_ms: 91_000, token_id: "token-a", pat_id: "pat-a" });
    await expect(drainRunnerOperations(store.storage, db.db, undefined, 1000)).resolves.toBe(true);
    expect(store.map.get(key)).toMatchObject({ deadline_ms: 91_000, attempts: 1 });
  });

  it("retains a corrupt marker without effects and still drains a later healthy marker", async () => {
    const store = storage(); const db = dbMock(); const healthy = operationAt(9); const healthyKey = `runner-credential/${healthy.operationId}`;
    store.map.set("runner-credential/corrupt", { schema_version: 1, ...operation, due: 1, attempts: 0 });
    store.map.set(healthyKey, marker({ ...healthy, due: 1 }));
    db.first.mockResolvedValue({ ...healthy, state: "revoked", deadline_ms: 91_000, token_id: null, pat_id: null });
    await expect(drainRunnerOperations(store.storage, db.db, undefined, 1000)).resolves.toBe(true);
    expect(store.map.has("runner-credential/corrupt")).toBe(true); expect(store.map.has(healthyKey)).toBe(false); expect(db.batch).toHaveBeenCalledTimes(1);
  });

  it("drains a legacy marker as generation zero without inventing a current generation", async () => {
    const store = storage(); const db = dbMock();
    const legacy = { schema_version: 1, operationId: operation.operationId, tenantId: operation.tenantId, jobId: operation.jobId, repo: operation.repo, deadline_ms: 91_000, due: 1, attempts: 0 };
    store.map.set(key, legacy);
    db.first.mockResolvedValue({ ...operation, lifecycle_generation: "0", state: "revoked", deadline_ms: 91_000, token_id: null, pat_id: null });
    await expect(drainRunnerOperations(store.storage, db.db, undefined, 1000)).resolves.toBe(false);
    expect(store.map.has(key)).toBe(false);
  });

  it("rejects rebinding an existing marker to another generation", async () => {
    const store = storage(); const db = dbMock(); store.map.set(key, marker());
    await expect(prepareRunnerOperation(store.storage, db.db, { ...operation, lifecycleGeneration: "2" }, 1000)).rejects.toThrow("malformed runner credential marker");
    expect(db.run).not.toHaveBeenCalled();
  });

  it("backs off four blocked due markers without starving a fifth due marker", async () => {
    const store = storage(); const db = dbMock();
    for (let i = 1; i <= 5; i++) { const op = operationAt(i); store.map.set(`runner-credential/${op.operationId}`, marker({ ...op, due: 1 })); }
    db.first.mockResolvedValue(null);
    await expect(drainRunnerOperations(store.storage, db.db, undefined, 1000)).resolves.toBe(true);
    for (let i = 1; i <= 4; i++) { const op = operationAt(i); expect(store.map.get(`runner-credential/${op.operationId}`)).toMatchObject({ attempts: 1, due: 3000, deadline_ms: 91_000 }); }
    const fifth = operationAt(5); expect(store.map.get(`runner-credential/${fifth.operationId}`)).toMatchObject({ attempts: 0, due: 1, deadline_ms: 91_000 });
  });
});
