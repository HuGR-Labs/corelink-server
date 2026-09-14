import { readFileSync } from "node:fs";
import { timingSafeEqual } from "node:crypto";
import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";
import { handleRunnerCloseGeneration } from "../src/lib/runner_credential_generation_routes.js";
import workerHandler from "../src/index.js";

const { DatabaseSync } = createRequire(import.meta.url)("node:sqlite") as typeof import("node:sqlite");
Object.defineProperty(globalThis.crypto.subtle, "timingSafeEqual", { value: timingSafeEqual });
const TENANT = "11111111-1111-4111-8111-111111111111";
class SqliteD1 {
  readonly sqlite = new DatabaseSync(":memory:");
  constructor() { this.sqlite.exec("CREATE TABLE runner_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, job_id TEXT NOT NULL, repo TEXT NOT NULL, state TEXT NOT NULL, deadline_ms INTEGER NOT NULL, pat_id TEXT, token_id TEXT); CREATE TABLE devenv_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, state TEXT NOT NULL, deadline_ms INTEGER NOT NULL, pat_id TEXT, token_id TEXT); CREATE TABLE pat (pat_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, pat_hash TEXT NOT NULL, token_id TEXT NOT NULL, runner_job_ac_key TEXT, revoked_at_ms INTEGER);"); this.sqlite.exec(readFileSync(new URL("../../migrations/d1/0129_credential_lifecycle_generation.sql", import.meta.url), "utf8")); this.sqlite.exec(readFileSync(new URL("../../migrations/d1/0130_credential_generation_event_receipts.sql", import.meta.url), "utf8")); }
  prepare(sql: string) { const db = this.sqlite; return { bind: (...args: unknown[]) => { const n = [...sql.matchAll(/\?(\d+)/g)]; const normalized = sql.replace(/\?(\d+)/g, "?"); const values = n.length ? n.map(m => args[Number(m[1]) - 1]) : args; return { run: async () => { const r = db.prepare(normalized).run(...values); return { success: true, meta: { changes: Number(r.changes) } }; }, first: async <T>() => db.prepare(normalized).get(...values) as T | undefined ?? null, all: async <T>() => ({ results: db.prepare(normalized).all(...values) as T[] }) }; } }; }
  async batch(statements: Array<{ run: () => Promise<{ success: boolean; meta: { changes: number } }> }>) { this.sqlite.exec("BEGIN"); try { const out = []; for (const s of statements) out.push(await s.run()); this.sqlite.exec("COMMIT"); return out; } catch (e) { this.sqlite.exec("ROLLBACK"); throw e; } }
}
const key = "r".repeat(40);
function env(db: SqliteD1, keyValue: string | undefined = key, legacy = "") { return { CONFIG_DB: db as never, METADATA_KV: { delete: async () => {} }, CORELINK_RUNNER_MINT_AUTH_KEY: keyValue, CORELINK_INTERNAL_AUTH_KEY: legacy } as never; }
function request(body: unknown, auth = key) { return new Request("https://worker/internal/v1/runner/credentials/close-generation", { method: "POST", headers: { "content-type": "application/json", "x-corelink-internal-auth": auth }, body: JSON.stringify(body) }); }
const input = { event_id: "event-1", tenant_id: TENANT, lifecycle_generation: "0" };

describe("runner close-generation endpoint", () => {
  it("authenticates before parsing or touching D1", async () => {
    const db = new SqliteD1();
    const response = await handleRunnerCloseGeneration(request({ bad: true }, "wrong"), env(db), "req-1");
    expect(response.status).toBe(401);
    expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM credential_generation_event_receipts").get()?.n).toBe(0);
  });

  it("refuses shared-key fallback when the dedicated runner key is absent or wrong", async () => {
    const absent = new SqliteD1();
    const noDedicated = await handleRunnerCloseGeneration(request(input), env(absent, "", key), "r");
    expect(noDedicated.status).toBe(503);
    expect(absent.sqlite.prepare("SELECT COUNT(*) AS n FROM credential_generation_event_receipts").get()?.n).toBe(0);
    const wrongDedicated = new SqliteD1();
    const legacyRequest = request(input, key);
    const denied = await handleRunnerCloseGeneration(legacyRequest, env(wrongDedicated, "d".repeat(40), key), "r");
    expect(denied.status).toBe(401);
    expect(wrongDedicated.sqlite.prepare("SELECT COUNT(*) AS n FROM credential_generation_event_receipts").get()?.n).toBe(0);
  });

  it("rejects malformed and oversized events", async () => {
    const db = new SqliteD1();
    expect((await handleRunnerCloseGeneration(request({ ...input, extra: 1 }), env(db), "r")).status).toBe(400);
    expect((await handleRunnerCloseGeneration(request({ ...input, tenant_id: "00000000-0000-0000-0000-000000000000" }), env(db), "r")).status).toBe(400);
    const oversized = new Request("https://worker/internal/v1/runner/credentials/close-generation", { method: "POST", headers: { "x-corelink-internal-auth": key }, body: "x".repeat(17000) });
    expect((await handleRunnerCloseGeneration(oversized, env(db), "r")).status).toBe(400);
    const atLimit = await handleRunnerCloseGeneration(request({ ...input, event_id: "a".repeat(256) }), env(db), "r");
    expect(atLimit.status).toBe(200);
    const astralOver = await handleRunnerCloseGeneration(request({ ...input, event_id: "a".repeat(254) + "😀" }), env(db), "r");
    expect(astralOver.status).toBe(400);
  });

  it("returns the exact accepted tuple and supports conflict/replay", async () => {
    const db = new SqliteD1();
    const response = await handleRunnerCloseGeneration(request(input), env(db), "r");
    expect(response.status).toBe(200);
    expect(response.headers.get("x-corelink-legacy-coverage")).toBe("unknown");
    expect(await response.json()).toEqual({ ...input, complete: true });
    const conflict = await handleRunnerCloseGeneration(request({ ...input, lifecycle_generation: "1" }), env(db), "r");
    expect(conflict.status).toBe(409);
    expect(await conflict.json()).toEqual({ error: "suspension_event_identity_conflict" });
  });

  it("returns 202 while durable revocation evidence remains pending", async () => {
    const db = new SqliteD1();
    db.sqlite.prepare("INSERT INTO pat (pat_id,tenant_id,pat_hash,token_id,runner_job_ac_key,lifecycle_generation) VALUES (?,?,?,?,?,?)").run("22222222-2222-4222-8222-222222222222", TENANT, "h", "token", "ac", "0");
    const failing = env(db) as { METADATA_KV: { delete: () => Promise<void> } };
    failing.METADATA_KV = { delete: async () => { throw new Error("down"); } };
    const response = await handleRunnerCloseGeneration(request(input), failing as never, "r");
    expect(response.status).toBe(202);
    expect(await response.json()).toEqual({ ...input, complete: false });
  });

  it("mounts the close-generation route behind its dedicated runner authority", async () => {
    const db = new SqliteD1();
    const response = await workerHandler.fetch!(request(input), {
      ...env(db),
      ENVIRONMENT: "test",
      CORELINK_SERVER: { idFromName: () => ({}), get: () => ({ fetch: async () => new Response(null, { status: 503 }) }) },
    } as never, { waitUntil: () => {} } as never);
    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toEqual({ ...input, complete: true });
  });
});
