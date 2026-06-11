/**
 * Unit tests for the DSR 24h verification sweep cron (WI-S11-008 incr 5).
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

function svc(status: number): { fetch: typeof fetch } {
  return {
    fetch: (async () =>
      new Response(status >= 400 ? "err" : "ok", { status })) as unknown as typeof fetch,
  };
}

/** A D1Lite that returns `rows` from the single grouped query. */
function db(rows: Array<{ dsr_id: string; tenant_id: string; queued_at: string }>): D1Lite {
  return {
    prepare: () => ({
      bind: () => ({
        all: async () => ({ results: rows }),
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
  it("ok on 2xx", async () => {
    const env: DsrVerifyCronEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "k",
      CORELINK_API_SVC: svc(200),
      CONFIG_DB: db([]),
    };
    expect(
      await postVerify(env, { dsr_id: "d1", tenant_id: "t1", queued_at_ms: 1 }),
    ).toEqual({ ok: true, status: 200 });
  });
  it("not-ok (no throw) without the internal-auth key", async () => {
    const env: DsrVerifyCronEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_API_SVC: svc(200),
      CONFIG_DB: db([]),
    };
    expect(await postVerify(env, { dsr_id: "d1", tenant_id: "t1", queued_at_ms: 1 })).toEqual({
      ok: false,
      status: 0,
    });
  });
});

describe("runDsrVerifySweep", () => {
  it("skipped (inert) when the internal-auth key is unbound", async () => {
    const env: DsrVerifyCronEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_API_SVC: svc(200),
      CONFIG_DB: db([{ dsr_id: "d1", tenant_id: "t1", queued_at: "2023-11-14T22:13:20Z" }]),
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
      CONFIG_DB: db([
        { dsr_id: "d1", tenant_id: "t1", queued_at: "2023-11-14T22:13:20Z" },
        { dsr_id: "d2", tenant_id: "t2", queued_at: "2023-11-15T00:00:00Z" },
      ]),
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
      CONFIG_DB: db([{ dsr_id: "d1", tenant_id: "t1", queued_at: "2023-11-14T22:13:20Z" }]),
    };
    expect(await runDsrVerifySweep(env, Date.now())).toEqual({
      swept: 0,
      failed: 1,
      skipped: false,
    });
  });
});
