import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";
import { activateRunnerPat, adoptRunnerOperation } from "../src/lib/runner_credential_obligation";

const { DatabaseSync } = createRequire(import.meta.url)("node:sqlite") as typeof import("node:sqlite");
const OP = { operationId: "11111111-1111-4111-8111-111111111111", tenantId: "22222222-2222-4222-8222-222222222222", jobId: "job-a", repo: "acme/repo", lifecycleGeneration: "7" };
const PAT = { pat_id: "22222222-2222-4222-8222-222222222222", token_id: "token-a", hash: "hmac$argon2$digest", expires_ms: Date.now() + 86_400_000 };
const RAW_SECRET = "raw-runner-secret-must-not-persist";

class RealD1 {
  readonly sqlite = new DatabaseSync(":memory:");
  failSecondBatchStatement = false;
  constructor() {
    this.sqlite.exec(`CREATE TABLE pat (
      pat_id TEXT PRIMARY KEY NOT NULL, tenant_id TEXT NOT NULL, pat_hash TEXT NOT NULL UNIQUE,
      scope TEXT NOT NULL DEFAULT 'read-write', expires_ms INTEGER NOT NULL, shown_once_token TEXT NOT NULL UNIQUE,
      shown_once_consumed INTEGER NOT NULL DEFAULT 0, created_ms INTEGER NOT NULL,
      token_id TEXT, runner_job_ac_key TEXT, revoked_at_ms INTEGER
    );
    CREATE TABLE devenv_credential_obligation (
      operation_id TEXT PRIMARY KEY NOT NULL, tenant_id TEXT NOT NULL, state TEXT NOT NULL,
      deadline_ms INTEGER NOT NULL, pat_id TEXT, token_id TEXT
    );`);
    for (const file of ["0127_devenv_credential_obligation.sql", "0128_runner_credential_obligation.sql", "0129_credential_lifecycle_generation.sql"]) {
      this.sqlite.exec(readFileSync(new URL(`../../migrations/d1/${file}`, import.meta.url), "utf8"));
    }
  }
  prepare(sql: string) {
    const db = this.sqlite;
    return { bind: (...args: unknown[]) => {
      const values = [...sql.matchAll(/\?(\d+)/g)].map((match) => args[Number(match[1]) - 1]);
      const statement = db.prepare(sql.replace(/\?\d+/g, "?"));
      return {
        run: async () => ({ meta: { changes: Number(statement.run(...values).changes) }, success: true }),
        first: async <T>() => statement.get(...values) as T | undefined ?? null,
      };
    }};
  }
  async batch(statements: Array<{ run(): Promise<{ meta: { changes: number }; success: boolean }> }>) {
    this.sqlite.exec("BEGIN");
    try {
      const results = [];
      for (const [index, statement] of statements.entries()) {
        if (this.failSecondBatchStatement && index === 1) throw new Error("injected activation update failure");
        results.push(await statement.run());
      }
      this.sqlite.exec("COMMIT");
      return results;
    } catch (error) {
      this.sqlite.exec("ROLLBACK");
      throw error;
    }
  }
}

function prepared(db: RealD1, operation = OP, deadline = Date.now() + 60_000) {
  db.sqlite.prepare(`INSERT INTO runner_credential_obligation
    (operation_id, tenant_id, job_id, repo, state, deadline_ms, lifecycle_generation)
    VALUES (?, ?, ?, ?, 'prepared', ?, ?)`)
    .run(operation.operationId, operation.tenantId, operation.jobId, operation.repo, deadline, operation.lifecycleGeneration);
}
function row(db: RealD1) { return db.sqlite.prepare("SELECT * FROM runner_credential_obligation").get() as Record<string, unknown> | undefined; }
function patCount(db: RealD1) { return Number((db.sqlite.prepare("SELECT COUNT(*) AS count FROM pat").get() as { count: number }).count); }

describe("runner credential activation and adoption against migrated D1", () => {
  it("activates only the prepared exact tuple and never persists plaintext", async () => {
    const db = new RealD1(); prepared(db);
    await expect(activateRunnerPat(db as never, OP, { ...PAT, rawSecret: RAW_SECRET } as never, "read-write", "ac-key")).resolves.toBe(true);
    expect(row(db)).toMatchObject({ state: "issued", tenant_id: OP.tenantId, job_id: OP.jobId, repo: OP.repo, lifecycle_generation: "7", pat_id: PAT.pat_id, token_id: PAT.token_id });
    expect(db.sqlite.prepare("SELECT pat_hash, shown_once_token FROM pat").get()).toEqual({ pat_hash: PAT.hash, shown_once_token: PAT.token_id });
    expect(JSON.stringify(db.sqlite.prepare("SELECT * FROM pat").get())).not.toContain(RAW_SECRET);
    expect(JSON.stringify(row(db))).not.toContain(RAW_SECRET);
  });

  it("rolls back PAT insertion when the obligation update fails", async () => {
    const db = new RealD1(); prepared(db); db.failSecondBatchStatement = true;
    await expect(activateRunnerPat(db as never, OP, { ...PAT, rawSecret: RAW_SECRET } as never, "read-write", "ac-key")).rejects.toThrow("injected activation update failure");
    expect(patCount(db)).toBe(0); expect(row(db)).toMatchObject({ state: "prepared", pat_id: null, token_id: null });
  });

  it.each(["7", "8"])("blocks activation at revocation floor %s with no PAT side effect", async (floor) => {
    const db = new RealD1(); prepared(db); db.sqlite.prepare("INSERT INTO tenant_credential_revocation_floor VALUES (?, ?)").run(OP.tenantId, floor);
    await expect(activateRunnerPat(db as never, OP, PAT, "read-write", "ac-key")).resolves.toBe(false);
    expect(patCount(db)).toBe(0); expect(row(db)?.state).toBe("prepared");
  });

  it("rejects a mismatched tenant, job, repo, or generation without inserting a PAT", async () => {
    const db = new RealD1(); prepared(db);
    for (const mismatch of [{ tenantId: "other" }, { jobId: "other" }, { repo: "other/repo" }, { lifecycleGeneration: "8" }]) {
      await expect(activateRunnerPat(db as never, { ...OP, ...mismatch }, PAT, "read-write", "ac-key")).resolves.toBe(false);
      expect(patCount(db)).toBe(0);
    }
  });

  it("adopts issued credentials idempotently, while expiry and stale generation deny", async () => {
    const db = new RealD1(); prepared(db); await activateRunnerPat(db as never, OP, PAT, "read-write", "ac-key");
    await expect(adoptRunnerOperation(db as never, OP.operationId, PAT.pat_id)).resolves.toBe(true);
    await expect(adoptRunnerOperation(db as never, OP.operationId, PAT.pat_id)).resolves.toBe(true);
    expect(row(db)?.state).toBe("adopted");

    const expired = new RealD1(); prepared(expired); await activateRunnerPat(expired as never, OP, PAT, "read-write", "ac-key");
    expired.sqlite.prepare("UPDATE runner_credential_obligation SET deadline_ms = 1").run();
    expect(await adoptRunnerOperation(expired as never, OP.operationId, PAT.pat_id)).toBe(false);
    const stale = new RealD1(); prepared(stale); await activateRunnerPat(stale as never, OP, PAT, "read-write", "ac-key");
    expect(await adoptRunnerOperation(stale as never, OP.operationId, "33333333-3333-4333-8333-333333333333")).toBe(false);
    stale.sqlite.prepare("INSERT INTO tenant_credential_revocation_floor VALUES (?, ?)").run(OP.tenantId, "7");
    expect(await adoptRunnerOperation(stale as never, OP.operationId, PAT.pat_id)).toBe(false);
  });
});
