import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";
import { closeGenerationEvent, CredentialGenerationEventError } from "../src/lib/credential_generation_receipts.js";

const { DatabaseSync } = createRequire(import.meta.url)("node:sqlite") as typeof import("node:sqlite");
const TENANT = "11111111-1111-4111-8111-111111111111";

class SqliteD1 {
  readonly sqlite = new DatabaseSync(":memory:");
  constructor() {
    this.sqlite.exec("CREATE TABLE runner_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, job_id TEXT NOT NULL, repo TEXT NOT NULL, state TEXT NOT NULL, deadline_ms INTEGER NOT NULL, pat_id TEXT, token_id TEXT); CREATE TABLE devenv_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, state TEXT NOT NULL, deadline_ms INTEGER NOT NULL, pat_id TEXT, token_id TEXT); CREATE TABLE pat (pat_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, pat_hash TEXT NOT NULL, token_id TEXT NOT NULL, runner_job_ac_key TEXT, revoked_at_ms INTEGER);");
    this.sqlite.exec(readFileSync(new URL("../../migrations/d1/0129_credential_lifecycle_generation.sql", import.meta.url), "utf8"));
    this.sqlite.exec(readFileSync(new URL("../../migrations/d1/0130_credential_generation_event_receipts.sql", import.meta.url), "utf8"));
  }
  prepare(sql: string) { const db = this.sqlite; return { bind: (...args: unknown[]) => { const n = [...sql.matchAll(/\?(\d+)/g)]; const normalized = sql.replace(/\?(\d+)/g, "?"); const values = n.length ? n.map(m => args[Number(m[1]) - 1]) : args; return { run: async () => { const r = db.prepare(normalized).run(...values); return { success: true, meta: { changes: Number(r.changes) } }; }, first: async <T>() => db.prepare(normalized).get(...values) as T | undefined ?? null, all: async <T>() => ({ results: db.prepare(normalized).all(...values) as T[] }) }; } }; }
  async batch(statements: Array<{ run: () => Promise<{ success: boolean; meta: { changes: number } }> }>) { this.sqlite.exec("BEGIN"); try { const out = []; for (const s of statements) out.push(await s.run()); this.sqlite.exec("COMMIT"); return out; } catch (e) { this.sqlite.exec("ROLLBACK"); throw e; } }
}

function pat(db: SqliteD1, i: number, tenant = TENANT) {
  const id = `${i.toString(16).padStart(8, "0")}-1111-4111-8111-111111111111`;
  db.sqlite.prepare("INSERT INTO pat (pat_id,tenant_id,pat_hash,token_id,runner_job_ac_key,lifecycle_generation) VALUES (?,?,?,?,?,?)").run(id, tenant, `h${i}`, `token-${i}`, "ac", "0");
}

describe("credential generation event receipts", () => {
  it("rejects tuple collisions before changing the floor", async () => {
    const db = new SqliteD1();
    await closeGenerationEvent(db as never, { delete: async () => {} }, { event_id: "evt", tenant_id: TENANT, lifecycle_generation: "0" }, 2);
    await expect(closeGenerationEvent(db as never, { delete: async () => {} }, { event_id: "evt", tenant_id: TENANT, lifecycle_generation: "1" }, 2)).rejects.toMatchObject({ code: "conflict" });
    expect(db.sqlite.prepare("SELECT revoked_through FROM tenant_credential_revocation_floor WHERE tenant_id=?").get(TENANT)?.revoked_through).toBe("0");
  });

  it("persists the floor and requested receipt when KV fails", async () => {
    const db = new SqliteD1(); pat(db, 1);
    const result = await closeGenerationEvent(db as never, { delete: async () => { throw new Error("down"); } }, { event_id: "evt", tenant_id: TENANT, lifecycle_generation: "0" }, 2);
    expect(result).toEqual({ complete: false });
    expect(db.sqlite.prepare("SELECT revoked_through FROM tenant_credential_revocation_floor WHERE tenant_id=?").get(TENANT)?.revoked_through).toBe("0");
    expect(db.sqlite.prepare("SELECT state FROM credential_generation_event_receipts WHERE event_id='evt'").get()?.state).toBe("requested");
  });

  it("drains 513 credentials within the bounded event budget", async () => {
    const db = new SqliteD1(); for (let i = 0; i < 513; i++) pat(db, i);
    const result = await closeGenerationEvent(db as never, { delete: async () => {} }, { event_id: "evt", tenant_id: TENANT, lifecycle_generation: "0" }, 256);
    expect(result).toEqual({ complete: true });
    expect(db.sqlite.prepare("SELECT state FROM credential_generation_event_receipts WHERE event_id='evt'").get()?.state).toBe("complete");
    expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM credential_generation_revocation WHERE state='revoked'").get()?.n).toBe(513);
  });

  it("replays complete events only while the durable floor proves coverage", async () => {
    const db = new SqliteD1();
    const input = { event_id: "evt", tenant_id: TENANT, lifecycle_generation: "0" };
    await closeGenerationEvent(db as never, { delete: async () => {} }, input, 2);
    expect(await closeGenerationEvent(db as never, { delete: async () => {} }, input, 2)).toEqual({ complete: true });
    db.sqlite.prepare("DELETE FROM tenant_credential_revocation_floor WHERE tenant_id=?").run(TENANT);
    await expect(closeGenerationEvent(db as never, { delete: async () => {} }, input, 2)).rejects.toMatchObject({ code: "unavailable" });
  });
});
