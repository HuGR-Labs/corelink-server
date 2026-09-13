import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";
import { closeCredentialGeneration, drainCredentialGenerationRevocations, validateLifecycleGeneration } from "../src/lib/credential_generation.js";

const { DatabaseSync } = createRequire(import.meta.url)("node:sqlite") as typeof import("node:sqlite");

const TENANT = "11111111-1111-4111-8111-111111111111";
const OTHER = "22222222-2222-4222-8222-222222222222";
const PAT = "33333333-3333-4333-8333-333333333333";

class SqliteD1 {
  readonly sqlite = new DatabaseSync(":memory:");
  constructor() {
    this.sqlite.exec(`CREATE TABLE runner_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, job_id TEXT NOT NULL, repo TEXT NOT NULL, state TEXT NOT NULL, deadline_ms INTEGER NOT NULL, pat_id TEXT, token_id TEXT);
      CREATE TABLE devenv_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, state TEXT NOT NULL, deadline_ms INTEGER NOT NULL, pat_id TEXT, token_id TEXT);
      CREATE TABLE pat (pat_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, pat_hash TEXT NOT NULL, token_id TEXT NOT NULL, runner_job_ac_key TEXT, revoked_at_ms INTEGER);`);
    this.sqlite.exec(readFileSync(new URL("../../migrations/d1/0129_credential_lifecycle_generation.sql", import.meta.url), "utf8"));
  }
  prepare(sql: string) {
    const database = this.sqlite;
    return {
      bind: (...args: unknown[]) => {
        const numbered = [...sql.matchAll(/\?(\d+)/g)];
        const normalized = sql.replace(/\?(\d+)/g, "?");
        const values = numbered.length === 0 ? args : numbered.map((match) => args[Number(match[1]) - 1]);
        return {
          run: async () => { const result = database.prepare(normalized).run(...values); return { success: true, meta: { changes: Number(result.changes) } }; },
          first: async <T>() => database.prepare(normalized).get(...values) as T | undefined ?? null,
          all: async <T>() => ({ results: database.prepare(normalized).all(...values) as T[] }),
        };
      },
    };
  }
  async batch(statements: Array<{ run: () => Promise<{ success: boolean; meta: { changes: number } }> }>) {
    this.sqlite.exec("BEGIN");
    try {
      const results = [];
      for (const statement of statements) results.push(await statement.run());
      this.sqlite.exec("COMMIT");
      return results;
    } catch (error) { this.sqlite.exec("ROLLBACK"); throw error; }
  }
}

function insertPat(db: SqliteD1, patId: string, tokenId: string, generation: string | null, runner = true, tenantId = TENANT) {
  db.sqlite.prepare("INSERT INTO pat (pat_id, tenant_id, pat_hash, token_id, runner_job_ac_key, lifecycle_generation) VALUES (?, ?, ?, ?, ?, ?)").run(
    patId, tenantId, `hash-${patId}`, tokenId, runner ? "ac-key" : null, generation,
  );
}

describe("credential lifecycle generation D1 projection", () => {
  it("validates canonical nonnegative i64 generations", () => {
    expect(validateLifecycleGeneration("0")).toBe("0");
    expect(validateLifecycleGeneration("9223372036854775807")).toBe("9223372036854775807");
    for (const value of ["", "01", "-1", " 1", "9223372036854775808"]) expect(() => validateLifecycleGeneration(value)).toThrow();
    expect(() => validateLifecycleGeneration("12345678901234567890")).toThrow();
    expect(() => new SqliteD1().sqlite.prepare(
      "INSERT INTO tenant_credential_revocation_floor (tenant_id, revoked_through) VALUES (?, ?)",
    ).run(TENANT, "9223372036854775808")).toThrow();
  });

  it("classifies only known legacy credentials and isolates future generations", async () => {
    const db = new SqliteD1();
    insertPat(db, PAT, "tok-runner", "0");
    insertPat(db, "44444444-4444-4444-8444-444444444444", "tok-customer", null, false);
    db.sqlite.prepare("INSERT INTO runner_credential_obligation (operation_id, tenant_id, job_id, repo, state, deadline_ms, pat_id, token_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)").run("op", TENANT, "job", "repo", "adopted", 1, PAT, "tok-runner");
    await closeCredentialGeneration(db as never, TENANT, "0");
    expect(db.sqlite.prepare("SELECT revoked_through FROM tenant_credential_revocation_floor WHERE tenant_id = ?").get(TENANT)?.revoked_through).toBe("0");
    expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM credential_generation_revocation").get().n).toBe(1);
    expect(db.sqlite.prepare("SELECT revoked_at_ms FROM pat WHERE pat_id = ?").get(PAT)?.revoked_at_ms).not.toBeNull();
    expect(db.sqlite.prepare("SELECT lifecycle_generation, revoked_at_ms FROM pat WHERE token_id = 'tok-customer'").get()).toEqual({ lifecycle_generation: null, revoked_at_ms: null });
    insertPat(db, "55555555-5555-4555-8555-555555555555", "tok-new", "1");
    await closeCredentialGeneration(db as never, TENANT, "0");
    expect(db.sqlite.prepare("SELECT revoked_at_ms FROM pat WHERE token_id = 'tok-new'").get()?.revoked_at_ms).toBeNull();
    await closeCredentialGeneration(db as never, TENANT, "1");
    expect(db.sqlite.prepare("SELECT revoked_at_ms FROM pat WHERE token_id = 'tok-new'").get()?.revoked_at_ms).not.toBeNull();
  });

  it("keeps pending rows on KV failure and drains at most four with exact identity fencing", async () => {
    const db = new SqliteD1();
    for (let i = 0; i < 5; i++) insertPat(db, `${String(i).padStart(2, "0")}666666-6666-4666-8666-66666666666${i}`, `tok-${i}`, "0");
    await closeCredentialGeneration(db as never, TENANT, "0");
    const failed = await drainCredentialGenerationRevocations(db as never, { delete: async () => { throw new Error("KV down"); } }, TENANT, "0");
    expect(failed).toEqual({ complete: false, revoked: 0 });
    expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM credential_generation_revocation").get().n).toBe(5);
    const deleted: string[] = [];
    const first = await drainCredentialGenerationRevocations(db as never, { delete: async (key) => { deleted.push(key); } }, TENANT, "0");
    expect(first).toEqual({ complete: false, revoked: 4 });
    expect(deleted).toHaveLength(4);
    expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM credential_generation_revocation WHERE state = 'revoked'").get().n).toBe(4);
    await closeCredentialGeneration(db as never, TENANT, "0");
    expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM credential_generation_revocation").get().n).toBe(5);
    const second = await drainCredentialGenerationRevocations(db as never, { delete: async (key) => { deleted.push(key); } }, TENANT, "0");
    expect(second).toEqual({ complete: true, revoked: 1 });
    await expect(drainCredentialGenerationRevocations(db as never, undefined, TENANT, "0")).rejects.toThrow();
  });

  it("pages a larger inventory and does not let a malformed row starve valid rows", async () => {
    const db = new SqliteD1();
    for (let i = 0; i < 40; i++) {
      const hex = i.toString(16).padStart(2, "0");
      insertPat(db, `${hex}777777-7777-4777-8777-7777777777${hex}`, `bulk-${i}`, "0");
    }
    await closeCredentialGeneration(db as never, TENANT, "0");
    expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM credential_generation_revocation").get().n).toBe(40);
    db.sqlite.prepare("INSERT INTO credential_generation_revocation (pat_id, token_id, tenant_id, lifecycle_generation) VALUES (?, ?, ?, ?)").run(
      "00000000-0000-0000-0000-000000000000", "bad-token", TENANT, "0",
    );
    const deleted: string[] = [];
    const result = await drainCredentialGenerationRevocations(db as never, { delete: async (key) => { deleted.push(key); } }, TENANT, "0");
    expect(result).toEqual({ complete: false, revoked: 4 });
    expect(deleted).toHaveLength(4);
    expect(db.sqlite.prepare("SELECT state FROM credential_generation_revocation WHERE pat_id = '00000000-0000-0000-0000-000000000000'").get()?.state).toBe("pending");
  });

  it("rejects conflicting immutable queue identity", async () => {
    const db = new SqliteD1();
    insertPat(db, PAT, "tok-original", "0");
    await closeCredentialGeneration(db as never, TENANT, "0");
    db.sqlite.prepare("UPDATE pat SET token_id = ? WHERE pat_id = ?").run("tok-mutated", PAT);
    await expect(closeCredentialGeneration(db as never, TENANT, "0")).rejects.toThrow("identity conflict");
  });

  it("advances beyond the bounded page without replaying confirmed rows", async () => {
    const db = new SqliteD1();
    for (let i = 0; i < 513; i++) {
      const hex = i.toString(16).padStart(3, "0");
      insertPat(db, `${hex}88888-8888-4888-8888-888888888${hex}`, `page-${i}`, "0");
    }
    insertPat(db, "99999999-9999-4999-8999-999999999999", "future", "2");
    insertPat(db, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", "other-tenant", "0", true, OTHER);
    const deleted: string[] = [];
    let complete = false;
    for (let page = 0; page < 8 && !complete; page++) {
      await closeCredentialGeneration(db as never, TENANT, "0");
      const drained = await drainCredentialGenerationRevocations(db as never, { delete: async (key) => { deleted.push(key); } }, TENANT, "0");
      complete = drained.complete;
    }
    expect(complete).toBe(false);
    expect(deleted).toHaveLength(32);
    expect(new Set(deleted).size).toBe(32);
    expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM credential_generation_revocation WHERE tenant_id = ?").get(TENANT)?.n).toBe(513);
    while (!complete) {
      await closeCredentialGeneration(db as never, TENANT, "0");
      const drained = await drainCredentialGenerationRevocations(db as never, { delete: async (key) => { deleted.push(key); } }, TENANT, "0");
      complete = drained.complete;
    }
    expect(deleted).toHaveLength(513);
    expect(new Set(deleted).size).toBe(513);
    expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM credential_generation_revocation WHERE state = 'revoked'").get().n).toBe(513);
    expect(db.sqlite.prepare("SELECT COUNT(*) AS n FROM pat WHERE token_id IN ('future', 'other-tenant') AND revoked_at_ms IS NOT NULL").get().n).toBe(0);
  });

  it("does not lower a later floor or sweep another tenant", async () => {
    const db = new SqliteD1();
    insertPat(db, PAT, "tok-a", "1");
    await closeCredentialGeneration(db as never, TENANT, "3");
    await closeCredentialGeneration(db as never, TENANT, "1");
    expect(db.sqlite.prepare("SELECT revoked_through FROM tenant_credential_revocation_floor WHERE tenant_id = ?").get(TENANT)?.revoked_through).toBe("3");
    expect(() => closeCredentialGeneration(db as never, OTHER, "0")).not.toThrow();
  });
});
