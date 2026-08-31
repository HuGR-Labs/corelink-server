/**
 * Unit tests for the DevEnv authorisation guard (`worker/src/lib/devenv_guard.ts`).
 *
 * This is the test BACKLOG item B-075 asked for by name: the item's own
 * `verify-means` block admits its regex-over-TypeScript check was the weakest
 * in its batch and says the item should be closed "pelo teste e não pelo grep"
 * — a unit test of the guard with D1 absent and D1 throwing. B-075's inverted
 * `verify` now RUNS this file.
 *
 * The defect under regression: `checkDevenvQuota` denied ONLY on
 * `install_status === "suspended"`, and BOTH of the other exits fell through to
 * `return { allowed: true }` —
 *
 *   1. no `runners_entitlement` row      (`if (row)` had no `else`)
 *   2. the D1 read threw                 (`catch { /* Fail-open … *\/ }`)
 *
 * DevEnv is the most expensive per-request compute surface in the product, so
 * either hole hands billable compute to a tenant that never bought the SKU —
 * and the D1 one hands it to EVERY tenant for the duration of an outage.
 *
 * The positive control (a valid, non-suspended row → allowed) is deliberate:
 * without it a guard that simply denied everything would also pass, and the
 * suite would prove nothing about the guard's ability to observe success.
 */

import { describe, it, expect, vi } from "vitest";
import type { D1Database } from "@cloudflare/workers-types";
import { checkDevenvQuota, enforceDevenvQuota } from "../src/lib/devenv_guard.js";
import type { Env } from "../src/index.js";

const TENANT = "00000000-0000-0000-0000-0000000000d0";

type EntitlementRow = {
  max_concurrency: number;
  max_vcpu_h: number;
  install_status: string;
};

/**
 * Minimal CONFIG_DB stub shaped like the guard's single call chain:
 *   prepare(sql).bind(tenantId).first<Row>()
 *
 * `first` is either a resolver (returning the row or null) or a thrower; a
 * `throwOn` of "prepare" | "bind" | "first" injects the failure at that stage,
 * so the fail-closed catch is proved for the whole chain, not just the await.
 */
function makeConfigDb(opts: {
  row?: EntitlementRow | null;
  throwOn?: "prepare" | "bind" | "first";
  firstRejects?: boolean;
}): { db: D1Database; calls: { prepare: number; first: number } } {
  const calls = { prepare: 0, first: 0 };
  const boom = new Error("D1_ERROR: network error while connecting to D1");

  const db = {
    prepare: vi.fn((sql: string) => {
      calls.prepare += 1;
      if (opts.throwOn === "prepare") throw boom;
      expect(sql).toContain("runners_entitlement");
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

describe("checkDevenvQuota — fail-closed contract (B-075)", () => {
  describe("pre-existing denials (must not regress)", () => {
    it("denies a missing tenant id", async () => {
      const { db } = makeConfigDb({ row: null });
      await expect(checkDevenvQuota(makeEnv(db), "")).resolves.toMatchObject({
        allowed: false,
      });
    });

    it("denies the _anonymous sentinel", async () => {
      const { db, calls } = makeConfigDb({ row: null });
      const r = await checkDevenvQuota(makeEnv(db), "_anonymous");
      expect(r.allowed).toBe(false);
      // Short-circuits before touching D1 at all.
      expect(calls.prepare).toBe(0);
    });

    it("denies a tenant whose entitlement is suspended", async () => {
      const { db } = makeConfigDb({
        row: { max_concurrency: 4, max_vcpu_h: 100, install_status: "suspended" },
      });
      const r = await checkDevenvQuota(makeEnv(db), TENANT);
      expect(r.allowed).toBe(false);
      expect(r.reason).toMatch(/suspend/i);
    });
  });

  // ── The two fail-OPEN holes B-075 reported ────────────────────────────────
  describe("fail-open hole #1 — no entitlement row", () => {
    it("DENIES a tenant with no runners_entitlement row", async () => {
      const { db, calls } = makeConfigDb({ row: null });
      const r = await checkDevenvQuota(makeEnv(db), TENANT);
      // Pre-fix this returned { allowed: true }: `if (row)` had no `else`.
      expect(r.allowed).toBe(false);
      expect(r.reason).toBeTruthy();
      // It really did consult D1 — the denial is a decision, not a short-circuit.
      expect(calls.first).toBe(1);
    });

    it("DENIES when first() resolves undefined rather than null", async () => {
      const { db } = makeConfigDb({ row: undefined as unknown as null });
      await expect(checkDevenvQuota(makeEnv(db), TENANT)).resolves.toMatchObject({
        allowed: false,
      });
    });
  });

  describe("fail-open hole #2 — the D1 read throws", () => {
    it.each(["prepare", "bind", "first"] as const)(
      "DENIES when D1 throws at %s()",
      async (stage) => {
        const { db } = makeConfigDb({ throwOn: stage });
        const r = await checkDevenvQuota(makeEnv(db), TENANT);
        // Pre-fix the catch body was an empty `/* Fail-open */` comment and
        // control fell through to `return { allowed: true }`.
        expect(r.allowed).toBe(false);
        expect(r.reason).toBeTruthy();
      },
    );

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

  // ── Positive control ──────────────────────────────────────────────────────
  // Without this, a guard that denied unconditionally would pass every test
  // above. This proves the suite can still observe SUCCESS.
  describe("positive control — a real entitlement is still honoured", () => {
    it("ALLOWS a tenant with a valid, non-suspended row", async () => {
      const { db } = makeConfigDb({
        row: { max_concurrency: 8, max_vcpu_h: 200, install_status: "active" },
      });
      const r = await checkDevenvQuota(makeEnv(db), TENANT);
      expect(r.allowed).toBe(true);
      expect(r.reason).toBeUndefined();
    });

    it("ALLOWS via the enforceDevenvQuota alias too", async () => {
      const { db } = makeConfigDb({
        row: { max_concurrency: 1, max_vcpu_h: 10, install_status: "installed" },
      });
      await expect(enforceDevenvQuota(makeEnv(db), TENANT)).resolves.toEqual({
        allowed: true,
      });
    });
  });
});
