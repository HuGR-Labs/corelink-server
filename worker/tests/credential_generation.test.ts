import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";
import { closeCredentialGeneration, drainCredentialGenerationRevocations } from "../src/lib/credential_generation.js";

const { DatabaseSync } = createRequire(import.meta.url)("node:sqlite") as typeof import("node:sqlite");
const TENANT = "11111111-1111-4111-8111-111111111111";
const OTHER_TENANT = "22222222-2222-4222-8222-222222222222";
const patId = (n: number) => `${n.toString(16).padStart(8, "0")}-6666-4666-8666-666666666666`;

type Statement = { run: () => Promise<{ success: boolean; meta: { changes: number } }> };
class SqliteD1 {
  readonly sqlite = new DatabaseSync(":memory:");
  constructor() {
    this.sqlite.exec(`CREATE TABLE runner_credential_obligation (
      operation_id TEXT PRIMARY KEY, tenant_id TEXT, job_id TEXT, repo TEXT,
      state TEXT, deadline_ms INTEGER, pat_id TEXT, token_id TEXT
    );
    CREATE TABLE devenv_credential_obligation (
      operation_id TEXT PRIMARY KEY, tenant_id TEXT, state TEXT, deadline_ms INTEGER,
      pat_id TEXT, token_id TEXT
    );
    CREATE TABLE pat (
      pat_id TEXT PRIMARY KEY, tenant_id TEXT, pat_hash TEXT, token_id TEXT,
      runner_job_ac_key TEXT, revoked_at_ms INTEGER
    );`);
    this.sqlite.exec(readFileSync(new URL("../../migrations/d1/0129_credential_lifecycle_generation.sql", import.meta.url), "utf8"));
  }
  prepare(sql: string) {
    const normalized = sql.replace(/\?(\d+)/g, "?");
    return {
      bind: (...args: unknown[]) => {
        const values = [...sql.matchAll(/\?(\d+)/g)].map(match => args[Number(match[1]) - 1]);
        const bound = values.length > 0 ? values : args;
        return {
          run: async () => { const result = this.sqlite.prepare(normalized).run(...bound); return { success: true, meta: { changes: Number(result.changes) } }; },
          first: async <T>() => (this.sqlite.prepare(normalized).get(...bound) as T | undefined) ?? null,
          all: async <T>() => ({ results: this.sqlite.prepare(normalized).all(...bound) as T[] }),
        };
      },
    };
  }
  async batch(statements: Statement[]) {
    this.sqlite.exec("BEGIN");
    try {
      const results = [];
      for (const statement of statements) results.push(await statement.run());
      this.sqlite.exec("COMMIT");
      return results;
    } catch (error) {
      this.sqlite.exec("ROLLBACK");
      throw error;
    }
  }
}

function insertPat(db: SqliteD1, id: string, token: string, generation: string | null, tenant = TENANT, runner = true) {
  db.sqlite.prepare("INSERT INTO pat (pat_id, tenant_id, pat_hash, token_id, runner_job_ac_key, lifecycle_generation) VALUES (?, ?, ?, ?, ?, ?)")
    .run(id, tenant, `hash-${id}`, token, runner ? "runner-key" : null, generation);
}

describe("credential lifecycle generation projection", () => {
  it("rejects a cross-tenant current-PAT identity conflict", async () => {
    const db = new SqliteD1();
    insertPat(db, patId(1), "original", "0");
    await closeCredentialGeneration(db as never, TENANT, "0");
    db.sqlite.prepare("UPDATE pat SET tenant_id = ? WHERE pat_id = ?").run(OTHER_TENANT, patId(1));
    await expect(closeCredentialGeneration(db as never, TENANT, "0")).rejects.toThrow("identity conflict");
  });

  it("does not let 256 malformed rows starve a valid row", async () => {
    const db = new SqliteD1();
    insertPat(db, patId(2), "valid-token", "0");
    await closeCredentialGeneration(db as never, TENANT, "0");
    for (let i = 0; i < 256; i++) db.sqlite.prepare("INSERT INTO credential_generation_revocation (pat_id, token_id, tenant_id, lifecycle_generation) VALUES (?, ?, ?, ?)").run(`!malformed-${i}`, `bad-${i}`, TENANT, "0");
    const deleted: string[] = [];
    const result = await drainCredentialGenerationRevocations(db as never, { delete: async key => deleted.push(key) }, TENANT, "0");
    expect(deleted).toContain("patrow:valid-token");
    expect(result.complete).toBe(false);
  });

  it("revalidates current PAT identity before deleting KV", async () => {
    const db = new SqliteD1();
    insertPat(db, patId(3), "stale-token", "0");
    await closeCredentialGeneration(db as never, TENANT, "0");
    db.sqlite.prepare("UPDATE pat SET token_id = ? WHERE pat_id = ?").run("current-token", patId(3));
    const deleted: string[] = [];
    const result = await drainCredentialGenerationRevocations(db as never, { delete: async key => deleted.push(key) }, TENANT, "0");
    expect(deleted).toEqual([]); expect(result.complete).toBe(false);
  });

  it("isolates customer PATs with non-null generation", async () => {
    const db = new SqliteD1();
    insertPat(db, patId(4), "customer-token", "0", TENANT, false);
    await closeCredentialGeneration(db as never, TENANT, "0");
    expect(db.sqlite.prepare("SELECT COUNT(*) AS count FROM credential_generation_revocation").get().count).toBe(0);
    expect(db.sqlite.prepare("SELECT revoked_at_ms FROM pat WHERE pat_id = ?").get(patId(4)).revoked_at_ms).toBeNull();
  });

  it("retries after a crash between KV delete and acknowledgement", async () => {
    const db = new SqliteD1();
    insertPat(db, patId(5), "retry-token", "0");
    await closeCredentialGeneration(db as never, TENANT, "0");
    const deleted: string[] = []; let crash = true;
    const originalPrepare = db.prepare.bind(db);
    db.prepare = (sql: string) => {
      const statement = originalPrepare(sql);
      if (!sql.includes("UPDATE credential_generation_revocation SET state") || !crash) return statement;
      return { ...statement, bind: (...args: unknown[]) => { const bound = statement.bind(...args); return { ...bound, run: async () => { crash = false; throw new Error("simulated crash"); } }; } };
    };
    const first = await drainCredentialGenerationRevocations(db as never, { delete: async key => deleted.push(key) }, TENANT, "0");
    const second = await drainCredentialGenerationRevocations(db as never, { delete: async key => deleted.push(key) }, TENANT, "0");
    expect(first.complete).toBe(false); expect(second.complete).toBe(true); expect(deleted).toEqual(["patrow:retry-token", "patrow:retry-token"]);
  });
});
