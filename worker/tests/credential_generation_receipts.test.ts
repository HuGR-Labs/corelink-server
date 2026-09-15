import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";
import { closeGenerationEvent } from "../src/lib/credential_generation_receipts.js";

const { DatabaseSync } = createRequire(import.meta.url)("node:sqlite") as typeof import("node:sqlite");
const TENANT = "11111111-1111-4111-8111-111111111111";

class SqliteD1 {
  readonly sqlite = new DatabaseSync(":memory:");
  failAfterCompletionUpdate = false;
  constructor() {
    this.sqlite.exec("CREATE TABLE runner_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, job_id TEXT NOT NULL, repo TEXT NOT NULL, state TEXT NOT NULL, deadline_ms INTEGER NOT NULL, pat_id TEXT, token_id TEXT); CREATE TABLE devenv_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, state TEXT NOT NULL, deadline_ms INTEGER NOT NULL, pat_id TEXT, token_id TEXT); CREATE TABLE pat (pat_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, pat_hash TEXT NOT NULL, token_id TEXT NOT NULL, runner_job_ac_key TEXT, revoked_at_ms INTEGER);");
    this.sqlite.exec(readFileSync(new URL("../../migrations/d1/0129_credential_lifecycle_generation.sql", import.meta.url), "utf8"));
    this.sqlite.exec(readFileSync(new URL("../../migrations/d1/0130_credential_generation_event_receipts.sql", import.meta.url), "utf8"));
  }
  prepare(sql: string) {
    const db = this.sqlite;
    return { bind: (...args: unknown[]) => {
      const markers = [...sql.matchAll(/\?(\d+)/g)];
      const normalized = sql.replace(/\?(\d+)/g, "?");
      const values = markers.length ? markers.map(m => args[Number(m[1]) - 1]) : args;
      return {
        run: async () => {
          const result = db.prepare(normalized).run(...values);
          if (this.failAfterCompletionUpdate && normalized.startsWith("UPDATE credential_generation_event_receipts")) {
            this.failAfterCompletionUpdate = false;
            throw new Error("connection lost after commit");
          }
          return { success: true, meta: { changes: Number(result.changes) } };
        },
        first: async <T>() => db.prepare(normalized).get(...values) as T | undefined ?? null,
        all: async <T>() => ({ results: db.prepare(normalized).all(...values) as T[] }),
      };
    } };
  }
  async batch(statements: Array<{ run: () => Promise<{ success: boolean; meta: { changes: number } }> }>) {
    this.sqlite.exec("BEGIN");
    try { const output = []; for (const statement of statements) output.push(await statement.run()); this.sqlite.exec("COMMIT"); return output; }
    catch (error) { this.sqlite.exec("ROLLBACK"); throw error; }
  }
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
  }, 20_000);

  it("replays complete events only while the durable floor proves coverage", async () => {
    const db = new SqliteD1();
    const input = { event_id: "evt", tenant_id: TENANT, lifecycle_generation: "0" };
    await closeGenerationEvent(db as never, { delete: async () => {} }, input, 2);
    expect(await closeGenerationEvent(db as never, { delete: async () => {} }, input, 2)).toEqual({ complete: true });
    db.sqlite.prepare("DELETE FROM tenant_credential_revocation_floor WHERE tenant_id=?").run(TENANT);
    await expect(closeGenerationEvent(db as never, { delete: async () => {} }, input, 2)).rejects.toMatchObject({ code: "unavailable" });
  });

  it("rejects a forged complete receipt with a different generation", async () => {
    const db = new SqliteD1();
    db.sqlite.prepare("INSERT INTO credential_generation_event_receipts VALUES (?, ?, ?, 'complete')").run("evt", TENANT, "3");
    db.sqlite.prepare("INSERT INTO tenant_credential_revocation_floor VALUES (?, ?)").run(TENANT, "3");
    await expect(closeGenerationEvent(db as never, { delete: async () => {} }, { event_id: "evt", tenant_id: TENANT, lifecycle_generation: "4" }, 2)).rejects.toMatchObject({ code: "conflict" });
    expect(db.sqlite.prepare("SELECT lifecycle_generation FROM credential_generation_event_receipts WHERE event_id='evt'").get()?.lifecycle_generation).toBe("3");
  });

  it("fails closed for a complete receipt without its durable floor", async () => {
    const db = new SqliteD1();
    db.sqlite.prepare("INSERT INTO credential_generation_event_receipts VALUES (?, ?, ?, 'complete')").run("evt", TENANT, "0");
    await expect(closeGenerationEvent(db as never, { delete: async () => {} }, { event_id: "evt", tenant_id: TENANT, lifecycle_generation: "0" }, 2)).rejects.toMatchObject({ code: "unavailable" });
  });

  it("does not mutate a receipt when its event id is replayed by another tenant", async () => {
    const db = new SqliteD1();
    const otherTenant = "22222222-2222-4222-8222-222222222222";
    await closeGenerationEvent(db as never, { delete: async () => {} }, { event_id: "evt", tenant_id: TENANT, lifecycle_generation: "0" }, 2);
    await expect(closeGenerationEvent(db as never, { delete: async () => {} }, { event_id: "evt", tenant_id: otherTenant, lifecycle_generation: "0" }, 2)).rejects.toMatchObject({ code: "conflict" });
    expect(db.sqlite.prepare("SELECT tenant_id FROM credential_generation_event_receipts WHERE event_id='evt'").get()?.tenant_id).toBe(TENANT);
  });

  it("reports replay KV failure as unavailable", async () => {
    const db = new SqliteD1();
    pat(db, 1);
    const input = { event_id: "evt", tenant_id: TENANT, lifecycle_generation: "0" };
    await closeGenerationEvent(db as never, { delete: async () => {} }, input, 2);
    pat(db, 2);
    db.sqlite.prepare("INSERT INTO credential_generation_revocation (pat_id, token_id, tenant_id, lifecycle_generation, state) VALUES (?, ?, ?, ?, 'pending')")
      .run("00000002-1111-4111-8111-111111111111", "token-2", TENANT, "0");
    await expect(closeGenerationEvent(db as never, { delete: async () => { throw new Error("down"); } }, input, 2)).rejects.toMatchObject({ code: "unavailable" });
  });

  it("retries successfully after completion UPDATE committed before a crash", async () => {
    const db = new SqliteD1();
    const input = { event_id: "evt", tenant_id: TENANT, lifecycle_generation: "0" };
    db.failAfterCompletionUpdate = true;
    await expect(closeGenerationEvent(db as never, { delete: async () => {} }, input, 2)).rejects.toMatchObject({ code: "unavailable" });
    expect(await closeGenerationEvent(db as never, { delete: async () => {} }, input, 2)).toEqual({ complete: true });
  });
});
