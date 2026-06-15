/**
 * Unit tests for the DSR 24h verification sweep cron (WI-S11-008 incr 5 + G4).
 * Mocks D1 via env.CONFIG_DB and the container call via env.CORELINK_API_SVC.
 */
import { describe, it, expect } from "vitest";
import {
  runDsrVerifySweep,
  postVerify,
  isoFromMs,
  msFromIso,
  type DsrVerifyCronEnv,
  type D1Lite,
} from "../src/webhooks/dsr_verify_cron.js";

/** Container verify response: HTTP `status` + a `decision` body (G4). */
function svc(status: number, decision = "verified_complete"): { fetch: typeof fetch } {
  return {
    fetch: (async () =>
      status >= 400
        ? new Response("err", { status })
        : Response.json({ ok: true, decision })) as unknown as typeof fetch,
  };
}

/**
 * Query-aware D1Lite mock. Routes by SQL substring: `dsr_requested` SELECT →
 * `requestedRows`, `dsr_erasure_log` SELECT → `logRows`. `UPDATE dsr_requested`
 * records the flipped dsr_id into `flips`.
 */
function db(opts: {
  requestedRows?: Array<Record<string, unknown>>;
  logRows?: Array<Record<string, unknown>>;
  flips?: string[];
  requestedThrows?: boolean;
}): D1Lite {
  return {
    prepare: (sql: string) => ({
      bind: (...vals: unknown[]) => ({
        all: async () => {
          if (sql.includes("dsr_requested")) {
            if (opts.requestedThrows) throw new Error("no such table: dsr_requested");
            return { results: opts.requestedRows ?? [] };
          }
          return { results: opts.logRows ?? [] };
        },
        run: async () => {
          if (sql.startsWith("UPDATE dsr_requested")) {
            opts.flips?.push(String(vals[0]));
          }
          return {};
        },
      }),
    }),
  };
}

describe("iso/ms helpers", () => {
  it("round-trips second precision", () => {
    const ms = 1_700_000_000_000;
    expect(isoFromMs(ms)).toBe("2023-11-14T22:13:20Z");
    expect(msFromIso(isoFromMs(ms))).toBe(ms);
  });
  it("msFromIso returns 0 on garbage", () => {
    expect(msFromIso("not-a-date")).toBe(0);
  });
});

describe("postVerify", () => {
  it("ok on 2xx and surfaces the decision", async () => {
    const env: DsrVerifyCronEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "k",
      CORELINK_API_SVC: svc(200, "verified_complete"),
      CONFIG_DB: db({}),
    };
    expect(
      await postVerify(env, { dsr_id: "d1", tenant_id: "t1", queued_at_ms: 1 }),
    ).toEqual({ ok: true, status: 200, decision: "verified_complete" });
  });
  it("not-ok (no throw) without the internal-auth key", async () => {
    const env: DsrVerifyCronEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_API_SVC: svc(200),
      CONFIG_DB: db({}),
    };
    expect(await postVerify(env, { dsr_id: "d1", tenant_id: "t1", queued_at_ms: 1 })).toEqual({
      ok: false,
      status: 0,
      decision: "",
    });
  });
});

describe("runDsrVerifySweep", () => {
  it("skipped (inert) when the internal-auth key is unbound", async () => {
    const env: DsrVerifyCronEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_API_SVC: svc(200),
      CONFIG_DB: db({ logRows: [{ dsr_id: "d1", tenant_id: "t1", queued_at: "2023-11-14T22:13:20Z" }] }),
    };
    expect(await runDsrVerifySweep(env, Date.now())).toEqual({
      swept: 0,
      failed: 0,
      skipped: true,
    });
  });

  it("sweeps each candidate DSR and counts ok vs failed", async () => {
    const env: DsrVerifyCronEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "k",
      CORELINK_API_SVC: svc(200),
      CONFIG_DB: db({
        logRows: [
          { dsr_id: "d1", tenant_id: "t1", queued_at: "2023-11-14T22:13:20Z" },
          { dsr_id: "d2", tenant_id: "t2", queued_at: "2023-11-15T00:00:00Z" },
        ],
      }),
    };
    expect(await runDsrVerifySweep(env, Date.now())).toEqual({
      swept: 2,
      failed: 0,
      skipped: false,
    });
  });

  it("counts a container failure as failed (never throws)", async () => {
    const env: DsrVerifyCronEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "k",
      CORELINK_API_SVC: svc(500),
      CONFIG_DB: db({ logRows: [{ dsr_id: "d1", tenant_id: "t1", queued_at: "2023-11-14T22:13:20Z" }] }),
    };
    expect(await runDsrVerifySweep(env, Date.now())).toEqual({
      swept: 0,
      failed: 1,
      skipped: false,
    });
  });

  // ── G4: dsr_requested anchor ──────────────────────────────────────────────

  it("G4: sweeps a requested DSR that has NO dsr_erasure_log row (pre-tombstone failure)", async () => {
    const flips: string[] = [];
    const now = Date.now();
    const env: DsrVerifyCronEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "k",
      CORELINK_API_SVC: svc(200, "verified_complete"),
      CONFIG_DB: db({
        // Only a requested anchor — no tombstone in dsr_erasure_log.
        requestedRows: [{ dsr_id: "dX", tenant_id: "tX", requested_at: now - 2 * 86_400_000 }],
        logRows: [],
        flips,
      }),
    };
    const r = await runDsrVerifySweep(env, now);
    expect(r).toEqual({ swept: 1, failed: 0, skipped: false });
    // verified_complete → the anchor is flipped so it stops being re-enumerated.
    expect(flips).toEqual(["dX"]);
  });

  it("G4: does NOT flip on a non-complete decision (verified_partial stays enumerable)", async () => {
    const flips: string[] = [];
    const now = Date.now();
    const env: DsrVerifyCronEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "k",
      CORELINK_API_SVC: svc(200, "verified_partial"),
      CONFIG_DB: db({
        requestedRows: [{ dsr_id: "dY", tenant_id: "tY", requested_at: now - 2 * 86_400_000 }],
        flips,
      }),
    };
    const r = await runDsrVerifySweep(env, now);
    expect(r.swept).toBe(1);
    expect(flips).toEqual([]); // partial → NOT flipped → re-enumerated next sweep
  });

  it("G4: dedupes by dsr_id across both sources (requested anchor wins)", async () => {
    const now = Date.now();
    const env: DsrVerifyCronEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "k",
      CORELINK_API_SVC: svc(200),
      CONFIG_DB: db({
        requestedRows: [{ dsr_id: "dDup", tenant_id: "t", requested_at: now - 2 * 86_400_000 }],
        logRows: [{ dsr_id: "dDup", tenant_id: "t", queued_at: "2023-11-14T22:13:20Z" }],
      }),
    };
    // Same dsr_id in both sources → swept once, not twice.
    expect((await runDsrVerifySweep(env, now)).swept).toBe(1);
  });

  it("G4: degrades to dsr_erasure_log when dsr_requested is absent (migration not applied)", async () => {
    const now = Date.now();
    const env: DsrVerifyCronEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "k",
      CORELINK_API_SVC: svc(200),
      CONFIG_DB: db({
        requestedThrows: true,
        logRows: [{ dsr_id: "dLog", tenant_id: "t", queued_at: "2023-11-14T22:13:20Z" }],
      }),
    };
    expect((await runDsrVerifySweep(env, now)).swept).toBe(1);
  });
});
