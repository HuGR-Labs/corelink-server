import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it, vi } from "vitest";
import { drainDevenvOperations, prepareDevenvOperation, PREPARE_MS } from "../src/lib/devenv_cleanup.js";
const operation = "00000000-0000-4000-8000-000000000001";
const tenant = "00000000-0000-4000-8000-000000000002";
const prefix = "devenv-cleanup/";
function durable() {
  const saved = new Map<string, unknown>(); const events: string[] = []; let alarm: number | null = null;
  const adapter = () => {
    const io = {
      put: vi.fn(async (key: string, value: unknown) => { events.push("put"); saved.set(key, value); }),
      get: vi.fn(async (key: string) => saved.get(key)),
      delete: vi.fn(async (key: string) => saved.delete(key)),
      list: vi.fn(async ({ prefix: start, limit }: { prefix: string; limit: number }) => new Map([...saved].filter(([key]) => key.startsWith(start)).slice(0, limit))),
      getAlarm: vi.fn(async () => alarm),
      setAlarm: vi.fn(async (value: number) => { events.push("alarm"); alarm = value; }),
      transaction: vi.fn(async (fn: (txn: unknown) => Promise<void>) => {
        const before = new Map(saved); const beforeAlarm = alarm;
        try { await fn(io); } catch (error) { saved.clear(); for (const [key, value] of before) saved.set(key, value); alarm = beforeAlarm; throw error; }
      }),
    };
    return { io, storage: io as unknown as DurableObjectStorage };
  };
  return { saved, events, adapter, alarm: () => alarm };
}
function database(events: string[] = []) {
  let present = true;
  const all = vi.fn(async () => ({ success: true, results: [], meta: { changes: 0 } }));
  const first = vi.fn(async () => present ? { state: "revoking", deadline_ms: 1, token_id: "token-id" } : null);
  const run = vi.fn(async (sql: string) => { events.push("d1-run"); if (sql.includes("SET state = 'revoked'")) present = false; return { success: true, results: [], meta: { changes: 1 } }; });
  const batch = vi.fn(async (_statements: unknown[]) => [{ success: true, results: [], meta: { changes: 1 } }, { success: true, results: [], meta: { changes: 1 } }, { success: true, results: [], meta: { changes: 1 } }]);
  const prepare = vi.fn((sql: string) => ({ bind: (..._args: unknown[]) => ({ run: () => run(sql), first, all }), all }));
  return { all, first, run, batch, db: { prepare, batch } as unknown as D1Database };
}
describe("DevEnv durable alarm obligations", () => {
  it("arms storage before creating the activation intent", async () => {
    const store = durable(); const db = database(store.events);
    expect(await prepareDevenvOperation(store.adapter().storage, db.db, operation, tenant, 1000, "0")).toBe(true);
    expect(store.events).toEqual(["put", "alarm", "d1-run"]);
    expect(store.alarm()).toBe(1000 + PREPARE_MS + 2000); expect(store.saved.has(prefix + operation)).toBe(true);
  });
  it("missing schema creates neither alarm nor intent", async () => {
    const store = durable(); const db = database(); db.all.mockRejectedValue(new Error("no table"));
    await expect(prepareDevenvOperation(store.adapter().storage, db.db, operation, tenant, 1000, "0")).rejects.toThrow();
    expect(store.saved.size).toBe(0); expect(store.alarm()).toBeNull(); expect(db.run).not.toHaveBeenCalled();
  });
  it("failed alarm transaction cannot permit activation", async () => {
    const store = durable(); const db = database(); const { storage, io } = store.adapter(); io.setAlarm.mockRejectedValue(new Error("storage fault"));
    await expect(prepareDevenvOperation(storage, db.db, operation, tenant, 1000, "0")).rejects.toThrow();
    expect(store.saved.size).toBe(0); expect(db.run).not.toHaveBeenCalled();
  });
  it("retains cleanup across adapter restart and KV failure", async () => {
    const store = durable(); const db = database(); store.saved.set(prefix + operation, { due: 1000, attempts: 0, tenantId: tenant });
    const kv = { delete: vi.fn(async (_key: string) => {}) }; kv.delete.mockRejectedValueOnce(new Error("KV fault"));
    expect(await drainDevenvOperations(store.adapter().storage, db.db, kv, 2000)).toBe(true);
    expect(store.saved.get(prefix + operation)).toEqual({ due: 4000, attempts: 1, tenantId: tenant }); expect(db.run).not.toHaveBeenCalled();
    expect(await drainDevenvOperations(store.adapter().storage, db.db, kv, 4000)).toBe(false);
    expect(store.saved.size).toBe(0); expect(kv.delete).toHaveBeenCalledWith("patrow:token-id");
  });
  it("preserves future pending work without touching D1", async () => {
    const store = durable(); const db = database(); store.saved.set(prefix + operation, { due: 9000, attempts: 0, tenantId: tenant });
    expect(await drainDevenvOperations(store.adapter().storage, db.db, undefined, 1000)).toBe(true);
    expect(db.batch).not.toHaveBeenCalled();
  });
  it("drains at most four due markers, without future-key starvation", async () => {
    const store = durable(); const db = database(); db.first.mockResolvedValue({ state: "revoked", deadline_ms: 1, token_id: "token-id" });
    store.saved.set(prefix + "future", { due: 9000, attempts: 0, tenantId: tenant });
    for (let i = 0; i < 6; i++) store.saved.set(prefix + i, { due: 1, attempts: 0, tenantId: tenant });
    expect(await drainDevenvOperations(store.adapter().storage, db.db, undefined, 1000)).toBe(true);
    expect(db.batch).toHaveBeenCalledTimes(4); expect(store.saved.size).toBe(3); expect(store.saved.has(prefix + "future")).toBe(true);
  });
  it("refuses issuance at the durable cleanup capacity", async () => {
    const store = durable(); const db = database();
    for (let i = 0; i < 64; i++) store.saved.set(prefix + i, { due: 1, attempts: 0, tenantId: tenant });
    await expect(prepareDevenvOperation(store.adapter().storage, db.db, operation, tenant, 1000, "0")).rejects.toThrow("capacity");
    expect(db.run).not.toHaveBeenCalled(); expect(store.saved.size).toBe(64);
  });
  it("a hung schema query has a finite deadline", async () => {
    vi.useFakeTimers();
    try {
      const store = durable(); const db = database(); db.all.mockImplementation(() => new Promise(() => {}));
      const result = prepareDevenvOperation(store.adapter().storage, db.db, operation, tenant, 1000, "0");
      const rejected = expect(result).rejects.toThrow("timeout"); await vi.advanceTimersByTimeAsync(5001); await rejected;
      expect(store.saved.size).toBe(0); expect(db.run).not.toHaveBeenCalled();
    } finally { vi.useRealTimers(); }
  });
  it("rejects noncanonical or overflowing lifecycle generations before effects", async () => {
    for (const generation of ["", "01", "-1", "9223372036854775808"]) {
      const store = durable(); const db = database();
      await expect(prepareDevenvOperation(store.adapter().storage, db.db, operation, tenant, 1000, generation)).resolves.toBe(false);
      expect(db.all).not.toHaveBeenCalled(); expect(store.events).toEqual([]);
    }
  });
  it("keeps an existing marker immutable across exact replay and collision", async () => {
    const store = durable(); const db = database();
    await expect(prepareDevenvOperation(store.adapter().storage, db.db, operation, tenant, 1000, "3")).resolves.toBe(true);
    const puts = store.events.filter(event => event === "put").length;
    await expect(prepareDevenvOperation(store.adapter().storage, db.db, operation, tenant, 1000, "3")).resolves.toBe(true);
    expect(store.events.filter(event => event === "put").length).toBe(puts);
    await expect(prepareDevenvOperation(store.adapter().storage, db.db, operation, tenant, 1000, "4")).rejects.toThrow("conflict");
  });
  it("retains a legacy marker when the KV invalidation dependency is absent", async () => {
    const store = durable(); const db = database(); store.saved.set(prefix + operation, { due: 1, attempts: 0, tenantId: tenant });
    await expect(drainDevenvOperations(store.adapter().storage, db.db, undefined, 1000)).resolves.toBe(true);
    expect(store.saved.has(prefix + operation)).toBe(true); expect(db.run).not.toHaveBeenCalled();
  });
});

it("executes production activation/revocation SQL against SQLite migrations", () => {
  const output = execFileSync("python3", [fileURLToPath(new URL("./devenv_cleanup_sql.py", import.meta.url))], { encoding: "utf8", timeout: 5000, stdio: ["ignore", "pipe", "pipe"] });
  expect(output).toBe("");
});
