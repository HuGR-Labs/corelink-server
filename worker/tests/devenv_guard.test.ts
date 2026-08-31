/**
 * Unit tests for the DevEnv entitlement guard (`worker/src/lib/devenv_guard.ts`).
 *
 * This is the test BACKLOG item B-075 asked for by name: its `verify-means`
 * block admits the original regex-over-TypeScript check was the weakest in its
 * batch and says the item should be closed "pelo teste e não pelo grep".
 * B-075's inverted `verify` runs this file.
 *
 * ## Two defects, and why the second one shapes this file
 *
 * 1. The guard denied only on `install_status === "suspended"`; the no-row path
 *    (`if (row)` with no `else`) and the `catch` both fell through to
 *    `allowed: true`.
 *
 * 2. **`install_status` is not a column and never was.** No migration creates
 *    it; it is synthesised into the API response by `customer_runners.rs:285`.
 *    Selecting it made D1 throw `no such column` on EVERY call — so the empty
 *    `catch` was not an edge case, it was the ONLY path, and the guard
 *    authorised everything unconditionally. Making that `catch` deny while
 *    leaving the phantom column in the SELECT would have flipped always-allow
 *    into always-deny: a total outage of the DevEnv surface, including the 8
 *    tenants holding real entitlement rows in prod.
 *
 * The first version of this suite could not see defect 2, and the reason is the
 * lesson: its mock FABRICATED the row shape the author expected, so
 * `SELECT totally_nonexistent_column` still passed 13/13. A mock that invents
 * the schema does not test the schema. So this file does two things differently:
 *
 *   - `schemaColumns()` parses the ACTUAL DDL out of the migrations and pins the
 *     guard's column list against it. This is the assertion that would have
 *     caught the outage.
 *   - `makeConfigDb` behaves like D1: it throws `no such column: <name>` for any
 *     column in the SELECT that the migrations do not create.
 *
 * The positive control is deliberate: without a case that reaches
 * `allowed: true`, a guard that denied everything — which is exactly what the
 * rejected first fix did — would pass every other test here.
 */

import { describe, it, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import type { D1Database } from "@cloudflare/workers-types";
import {
  checkDevenvQuota,
  enforceDevenvQuota,
  DEVENV_ENTITLEMENT_COLUMNS,
} from "../src/lib/devenv_guard.js";
import type { Env } from "../src/index.js";

const TENANT = "00000000-0000-0000-0000-0000000000d0";
const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/**
 * The REAL columns of `runners_entitlement`, parsed from the migrations that
 * create them — never hand-listed here, or this test would just restate the
 * author's belief the way the previous mock did.
 *
 * 0070 CREATEs the table; 0072 ALTERs in `max_vcpu_h`. Verified against prod D1
 * on 2026-08-31: PRAGMA table_info reports exactly
 * tenant_id, max_concurrency, plan, created_at_ms, max_vcpu_h.
 */
function schemaColumns(): Set<string> {
  const create = readFileSync(
    path.join(REPO_ROOT, "migrations/d1/0070_runners_entitlement.sql"),
    "utf8",
  );
  const body = /CREATE TABLE IF NOT EXISTS runners_entitlement\s*\(([\s\S]*?)\);/i.exec(create);
  if (!body) {
    throw new Error("could not parse the runners_entitlement CREATE TABLE out of migration 0070");
  }
  const cols = new Set<string>();
  for (const raw of body[1]!.split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("--")) continue;
    const m = /^([a-z_][a-z0-9_]*)\s+(TEXT|INTEGER|REAL|BLOB|NUMERIC)/i.exec(line);
    if (m) cols.add(m[1]!);
  }

  const alter = readFileSync(
    path.join(REPO_ROOT, "migrations/d1/0072_runners_entitlement_max_vcpu_h.sql"),
    "utf8",
  );
  for (const m of alter.matchAll(
    /^\s*ALTER TABLE runners_entitlement ADD COLUMN\s+([a-z_][a-z0-9_]*)/gim,
  )) {
    cols.add(m[1]!);
  }

  // Anti-vacuity: a parser that silently returned {} would make every
  // assertion below pass. Pin the two names the migrations demonstrably create.
  if (!cols.has("tenant_id") || !cols.has("max_concurrency")) {
    throw new Error(`migration parse produced an implausible column set: ${[...cols].join(",")}`);
  }
  return cols;
}

const SCHEMA = schemaColumns();

/** Columns named in a `SELECT a, b FROM …` clause. */
function selectedColumns(sql: string): string[] {
  const m = /SELECT\s+([\s\S]*?)\s+FROM/i.exec(sql);
  if (!m) throw new Error(`no SELECT…FROM clause in: ${sql}`);
  return m[1]!.split(",").map((c) => c.trim()).filter(Boolean);
}

type Row = {
  max_concurrency?: number | null | string;
  max_vcpu_h?: number | null;
};

/**
 * CONFIG_DB stub shaped like the guard's call chain — `prepare(sql).bind(id).first()`
 * — that behaves like D1 on the one axis the old mock faked away: a SELECT naming
 * a column the migrations do not create THROWS, exactly as prod does.
 */
function makeConfigDb(opts: {
  row?: Row | null;
  throwOn?: "prepare" | "bind" | "first";
  firstRejects?: boolean;
}): { db: D1Database; calls: { prepare: number; first: number } } {
  const calls = { prepare: 0, first: 0 };
  const boom = new Error("D1_ERROR: network error while connecting to D1");

  const db = {
    prepare: vi.fn((sql: string) => {
      calls.prepare += 1;
      if (opts.throwOn === "prepare") throw boom;
      for (const col of selectedColumns(sql)) {
        if (!SCHEMA.has(col)) {
          // This is the prod failure verbatim: `no such column: install_status`.
          throw new Error(`D1_ERROR: no such column: ${col}: SQLITE_ERROR`);
        }
      }
      return {
        bind: vi.fn((..._args: unknown[]) => {
          if (opts.throwOn === "bind") throw boom;
          return {
            first: vi.fn(async () => {
              calls.first += 1;
              if (opts.throwOn === "first") {
                if (opts.firstRejects) return Promise.reject(boom);
                throw boom;
              }
              return opts.row ?? null;
            }),
          };
        }),
      };
    }),
  } as unknown as D1Database;

  return { db, calls };
}

function makeEnv(db: D1Database | undefined): Env {
  return { CONFIG_DB: db } as unknown as Env;
}

describe("checkDevenvQuota — the query matches the real schema", () => {
  it("selects ONLY columns the migrations actually create", () => {
    for (const col of DEVENV_ENTITLEMENT_COLUMNS) {
      expect(
        SCHEMA.has(col),
        `devenv_guard selects "${col}", which no migration creates — every D1 read ` +
          `would throw "no such column: ${col}" and the guard would deny (or, with a ` +
          `fail-open catch, allow) 100% of requests. This is the assertion that the ` +
          `phantom install_status column defeated.`,
      ).toBe(true);
    }
  });

  it("does not select the phantom install_status column", () => {
    expect(SCHEMA.has("install_status")).toBe(false); // control: it really is absent
    expect([...DEVENV_ENTITLEMENT_COLUMNS]).not.toContain("install_status");
  });

  it("still reads the tenant's concurrency cap", () => {
    expect([...DEVENV_ENTITLEMENT_COLUMNS]).toContain("max_concurrency");
  });

  it("the D1 stub REJECTS an invented column (teeth test for the mock itself)", () => {
    const { db } = makeConfigDb({ row: { max_concurrency: 4 } });
    expect(() =>
      (db as unknown as { prepare: (s: string) => unknown }).prepare(
        "SELECT install_status FROM runners_entitlement WHERE tenant_id = ?1",
      ),
    ).toThrow(/no such column: install_status/);
  });
});

describe("checkDevenvQuota — fail-closed contract (B-075)", () => {
  describe("pre-existing denials (must not regress)", () => {
    it("denies a missing tenant id", async () => {
      const { db } = makeConfigDb({ row: null });
      await expect(checkDevenvQuota(makeEnv(db), "")).resolves.toMatchObject({ allowed: false });
    });

    it("denies the _anonymous sentinel without touching D1", async () => {
      const { db, calls } = makeConfigDb({ row: null });
      const r = await checkDevenvQuota(makeEnv(db), "_anonymous");
      expect(r.allowed).toBe(false);
      expect(calls.prepare).toBe(0);
    });
  });

  describe("fail-open hole #1 — no entitlement row", () => {
    it("DENIES a tenant with no runners_entitlement row", async () => {
      const { db, calls } = makeConfigDb({ row: null });
      const r = await checkDevenvQuota(makeEnv(db), TENANT);
      expect(r.allowed).toBe(false);
      expect(r.reason).toBeTruthy();
      expect(calls.first).toBe(1); // it really consulted D1
    });

    it("DENIES when first() resolves undefined rather than null", async () => {
      const { db } = makeConfigDb({ row: undefined as unknown as null });
      await expect(checkDevenvQuota(makeEnv(db), TENANT)).resolves.toMatchObject({
        allowed: false,
      });
    });
  });

  describe("fail-open hole #2 — the D1 read throws", () => {
    it.each(["prepare", "bind", "first"] as const)("DENIES when D1 throws at %s()", async (stage) => {
      const { db } = makeConfigDb({ throwOn: stage });
      const r = await checkDevenvQuota(makeEnv(db), TENANT);
      expect(r.allowed).toBe(false);
      expect(r.reason).toBeTruthy();
    });

    it("DENIES when first() returns a REJECTED promise (async outage)", async () => {
      const { db } = makeConfigDb({ throwOn: "first", firstRejects: true });
      await expect(checkDevenvQuota(makeEnv(db), TENANT)).resolves.toMatchObject({
        allowed: false,
      });
    });

    it("never rejects — an outage is a denial, not an unhandled throw", async () => {
      const { db } = makeConfigDb({ throwOn: "first" });
      await expect(checkDevenvQuota(makeEnv(db), TENANT)).resolves.toBeDefined();
    });
  });

  describe("CONFIG_DB entirely unbound", () => {
    it("DENIES when env.CONFIG_DB is absent", async () => {
      const r = await checkDevenvQuota(makeEnv(undefined), TENANT);
      expect(r.allowed).toBe(false);
      expect(r.reason).toBeTruthy();
    });
  });

  describe("the cap on the row (migration 0070's CHECK, restated)", () => {
    it.each([0, -1, null, undefined])(
      "DENIES a row whose max_concurrency is %s",
      async (cap) => {
        const { db } = makeConfigDb({ row: { max_concurrency: cap as number | null } });
        const r = await checkDevenvQuota(makeEnv(db), TENANT);
        expect(r.allowed).toBe(false);
      },
    );

    // The `typeof … !== "number"` half of that condition was the ONE surviving
    // mutant of 9: delete it and every other case here still passed, because
    // `"8" > 0` is true in JS. It is not hypothetical in this repo — D1 over the
    // REST binding is known to return numeric columns as REAL/strings, so a
    // non-number cap is a shape the transport can actually deliver, and without
    // the typeof check it would AUTHORISE.
    it("DENIES a row whose max_concurrency is the STRING \"8\", not a number", async () => {
      const { db } = makeConfigDb({ row: { max_concurrency: "8" } });
      const r = await checkDevenvQuota(makeEnv(db), TENANT);
      expect(r.allowed).toBe(false);
      expect(r.reason).toBeTruthy();
    });
  });

  describe("max_vcpu_h is SELECTed but decides nothing", () => {
    // Pins the docstring's claim so it cannot rot back into the lie it replaced:
    // the guard used to say max_vcpu_h "is read so that a non-positive value can
    // be refused", which was false. Adding that refusal would red THIS test and
    // force the doc (and migration 0072's wall-off semantics) to be revisited
    // deliberately rather than silently.
    it("ALLOWS a positive cap even with a NEGATIVE max_vcpu_h — the column is inert", async () => {
      const { db } = makeConfigDb({ row: { max_concurrency: 8, max_vcpu_h: -5 } });
      await expect(checkDevenvQuota(makeEnv(db), TENANT)).resolves.toEqual({ allowed: true });
    });
  });
});

// ── Positive controls ───────────────────────────────────────────────────────
// The rejected first fix denied 100% of requests and would have passed every
// test above. These are what separate "fails closed" from "is broken".
describe("checkDevenvQuota — a real entitlement is still honoured", () => {
  it("ALLOWS a tenant with a positive concurrency cap", async () => {
    const { db } = makeConfigDb({ row: { max_concurrency: 8, max_vcpu_h: 200 } });
    const r = await checkDevenvQuota(makeEnv(db), TENANT);
    expect(r.allowed).toBe(true);
    expect(r.reason).toBeUndefined();
  });

  it("ALLOWS when max_vcpu_h is absent — 0072 says absent walls off and proceeds", async () => {
    const { db } = makeConfigDb({ row: { max_concurrency: 1, max_vcpu_h: null } });
    await expect(checkDevenvQuota(makeEnv(db), TENANT)).resolves.toEqual({ allowed: true });
  });

  it("ALLOWS via the enforceDevenvQuota alias too", async () => {
    const { db } = makeConfigDb({ row: { max_concurrency: 2, max_vcpu_h: 10 } });
    await expect(enforceDevenvQuota(makeEnv(db), TENANT)).resolves.toEqual({ allowed: true });
  });

  it("the guard's OWN query survives the schema-faithful stub end to end", async () => {
    // The regression that matters: with `install_status` in the SELECT this
    // throws `no such column` and the tenant is denied despite a valid row.
    const { db } = makeConfigDb({ row: { max_concurrency: 4, max_vcpu_h: 100 } });
    await expect(checkDevenvQuota(makeEnv(db), TENANT)).resolves.toEqual({ allowed: true });
  });
});
