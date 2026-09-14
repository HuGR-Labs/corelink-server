import { readFileSync } from "node:fs";
import { timingSafeEqual } from "node:crypto";
import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";
import { handleRunnerCloseGeneration } from "../src/lib/runner_credential_generation_routes.js";

const { DatabaseSync } = createRequire(import.meta.url)("node:sqlite") as typeof import("node:sqlite");
Object.defineProperty(globalThis.crypto.subtle, "timingSafeEqual", { value: timingSafeEqual });
const TENANT = "11111111-1111-4111-8111-111111111111";
const KEY = "r".repeat(40);
class D1 {
  readonly sqlite = new DatabaseSync(":memory:");
  constructor() { this.sqlite.exec("CREATE TABLE runner_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT, job_id TEXT, repo TEXT, state TEXT, deadline_ms INTEGER, pat_id TEXT, token_id TEXT); CREATE TABLE devenv_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT, state TEXT, deadline_ms INTEGER, pat_id TEXT, token_id TEXT); CREATE TABLE customer_audit_events (tenant_id TEXT, event_type TEXT, target TEXT); CREATE TABLE pat (pat_id TEXT PRIMARY KEY, tenant_id TEXT, pat_hash TEXT, token_id TEXT, runner_job_ac_key TEXT, revoked_at_ms INTEGER, expires_ms INTEGER);"); this.sqlite.exec(readFileSync(new URL("../../migrations/d1/0129_credential_lifecycle_generation.sql", import.meta.url), "utf8")); this.sqlite.exec(readFileSync(new URL("../../migrations/d1/0130_credential_generation_event_receipts.sql", import.meta.url), "utf8")); }
  prepare(sql: string) { const db = this.sqlite; return { bind: (...args: unknown[]) => { const marks = [...sql.matchAll(/\?(\d+)/g)]; const values = marks.length ? marks.map(m => args[Number(m[1]) - 1]) : args; const q = sql.replace(/\?(\d+)/g, "?"); return { run: async () => { const r = db.prepare(q).run(...values); return { success: true, meta: { changes: Number(r.changes) } }; }, first: async <T>() => db.prepare(q).get(...values) as T | undefined ?? null, all: async <T>() => ({ results: db.prepare(q).all(...values) as T[] }) }; } }; }
  async batch(statements: Array<{ run: () => Promise<{ success: boolean; meta: { changes: number } }> }>) { this.sqlite.exec("BEGIN"); try { const output = []; for (const statement of statements) output.push(await statement.run()); this.sqlite.exec("COMMIT"); return output; } catch (error) { this.sqlite.exec("ROLLBACK"); throw error; } }
}
function env(db: D1, key: string | undefined = KEY, legacy = "") { return { CONFIG_DB: db as never, METADATA_KV: { delete: async () => {} }, CORELINK_RUNNER_MINT_AUTH_KEY: key, CORELINK_INTERNAL_AUTH_KEY: legacy } as never; }
function request(body: unknown, auth = KEY, method = "POST") { return new Request("https://worker/internal/v1/runner/credentials/close-generation", { method, headers: { "content-type": "application/json", "x-corelink-internal-auth": auth }, body: method === "POST" ? JSON.stringify(body) : undefined }); }
const input = { event_id: "event-1", tenant_id: TENANT, lifecycle_generation: "0" };

describe("runner close-generation handler", () => {
  it("authenticates before parsing or touching D1", async () => { const db = new D1(); const response = await handleRunnerCloseGeneration(request({ bad: true }, "wrong"), env(db), "req"); expect(response.status).toBe(401); expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM credential_generation_event_receipts").get()?.n).toBe(0); });
  it.each([[""], ["short"]])("requires a dedicated key (%s)", async key => { const db = new D1(); expect((await handleRunnerCloseGeneration(request(input), env(db, key), "r")).status).toBe(503); });
  it("accepts only POST and enforces the 16KiB limit on chunked bodies", async () => {
    const db = new D1();
    expect((await handleRunnerCloseGeneration(request(input, KEY, "GET"), env(db), "r")).status).toBe(405);
    const stream = new ReadableStream<Uint8Array>({ start(controller) { controller.enqueue(new Uint8Array(16 * 1024)); controller.enqueue(new Uint8Array(1)); controller.close(); } });
    const oversized = new Request("https://worker/x", { method: "POST", headers: { "x-corelink-internal-auth": KEY }, body: stream, duplex: "half" } as RequestInit);
    expect((await handleRunnerCloseGeneration(oversized, env(db), "r")).status).toBe(400);
  });
  it("returns canonical tuple, verified coverage, and conflict", async () => { const db = new D1(); db.sqlite.prepare("INSERT INTO pat VALUES (?, ?, ?, ?, ?, ?, ?, ?)").run("22222222-2222-4222-8222-222222222222", TENANT, "hash", "token", null, 1, 4102444800000, "0"); db.sqlite.prepare("INSERT INTO customer_audit_events VALUES (?, ?, ?)").run(TENANT, "pat.created", "22222222-2222-4222-8222-222222222222"); const response = await handleRunnerCloseGeneration(request(input), env(db), "r"); expect(response.status).toBe(200); expect(response.headers.get("x-corelink-legacy-coverage")).toBe("verified"); expect(await response.json()).toEqual({ ...input, complete: true }); expect((await handleRunnerCloseGeneration(request({ ...input, lifecycle_generation: "1" }), env(db), "r")).status).toBe(409); });
  it("returns pending and unknown coverage fail-closed", async () => { const pending = new D1(); pending.sqlite.prepare("INSERT INTO pat VALUES (?, ?, ?, ?, ?, ?, ?, ?)").run("22222222-2222-4222-8222-222222222222", TENANT, "hash", "token", "ac", null, 4102444800000, "0"); const failing = env(pending) as { METADATA_KV: { delete: () => Promise<void> } }; failing.METADATA_KV = { delete: async () => { throw new Error("down"); } }; expect((await handleRunnerCloseGeneration(request(input), failing as never, "r")).status).toBe(202); const unknown = new D1(); await handleRunnerCloseGeneration(request(input), env(unknown), "r"); unknown.sqlite.exec("DROP TABLE customer_audit_events"); const replay = await handleRunnerCloseGeneration(request(input), env(unknown), "r"); expect(replay.headers.get("x-corelink-legacy-coverage")).toBe("unknown"); });
  it("rejects malformed, short/foreign auth, and oversized events without secrets", async () => { const db = new D1(); expect((await handleRunnerCloseGeneration(request({ ...input, extra: 1 }), env(db), "r")).status).toBe(400); expect((await handleRunnerCloseGeneration(request(input, "d".repeat(40)), env(db), "r")).status).toBe(401); const body = await (await handleRunnerCloseGeneration(request(input), env(db), "r")).text(); expect(body).not.toMatch(/hash|token|upstream|credential_generation_unavailable/); });
});
