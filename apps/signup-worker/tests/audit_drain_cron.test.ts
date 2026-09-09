/**
 * Unit tests for the S-09 audit-chain drain sweep cron (B-064).
 *
 * The container call is mocked via `env.CORELINK_API_SVC`. The boundary accepts
 * only the handler's complete response contract: HTTP success alone is never
 * proof that an irreversible audit drain completed.
 */
import { describe, it, expect, vi } from "vitest";
import {
  runAuditDrainSweep,
  DRAIN_WALL_BUDGET_MS,
  MAX_DRAIN_CALLS,
  type AuditDrainCronEnv,
} from "../src/webhooks/audit_drain_cron.js";

const KEY = "e".repeat(32);

type DrainResponse = {
  ok: boolean;
  partitions_drained: number;
  rows_sealed: number;
  partitions_drifted: number;
  partitions_failed: number;
  partitions_leased: number;
  heads_resigned: number;
  incomplete: boolean;
};

/** A full wire contract; individual tests override exactly one dimension. */
function drainResponse(overrides: Partial<DrainResponse> = {}): DrainResponse {
  return {
    ok: true,
    partitions_drained: 0,
    rows_sealed: 0,
    partitions_drifted: 0,
    partitions_failed: 0,
    partitions_leased: 0,
    heads_resigned: 0,
    incomplete: false,
    ...overrides,
  };
}

/** Stub service binding replaying `bodies` in order; the last one repeats. */
function svcSeq(
  bodies: Array<{ status: number; body: unknown }>,
): { fetch: typeof fetch; calls: () => number } {
  let n = 0;
  return {
    calls: () => n,
    fetch: (async () => {
      const b = bodies.at(Math.min(n, bodies.length - 1));
      n++;
      if (!b) throw new Error("svcSeq: no bodies configured");
      return new Response(
        typeof b.body === "string" ? b.body : JSON.stringify(b.body),
        { status: b.status, headers: { "content-type": "application/json" } },
      );
    }) as unknown as typeof fetch,
  };
}

function env(over: Partial<AuditDrainCronEnv> = {}): AuditDrainCronEnv {
  return {
    CORELINK_API_BASE: "https://corelink-api.example",
    CORELINK_ERASE_AUTH_KEY: KEY,
    ...over,
  };
}

describe("runAuditDrainSweep", () => {
  it("sends only the dedicated ≥32-char erase key and reads the real fields", async () => {
    let header: string | null = null;
    const r = await runAuditDrainSweep(
      env({
        CORELINK_INTERNAL_AUTH_KEY: "shared-key-that-must-not-be-used",
        CORELINK_API_SVC: {
          fetch: (async (request: Request) => {
            header = request.headers.get("x-corelink-internal-auth");
            return new Response(
              JSON.stringify(
                drainResponse({ rows_sealed: 200, partitions_drained: 3 }),
              ),
              { status: 200, headers: { "content-type": "application/json" } },
            );
          }) as unknown as typeof fetch,
        },
      }),
      0,
    );

    expect(header).toBe(KEY);
    expect(r).toMatchObject({
      ok: true,
      status: 200,
      sealed: 200,
      partitions: 3,
      calls: 1,
      incomplete: false,
      skipped: false,
    });
  });

  it("shared-only configuration skips fail-closed and never sends a request", async () => {
    const svc = svcSeq([{ status: 200, body: drainResponse({ rows_sealed: 1 }) }]);
    const r = await runAuditDrainSweep(
      env({
        CORELINK_ERASE_AUTH_KEY: undefined,
        CORELINK_INTERNAL_AUTH_KEY: "s".repeat(32),
        CORELINK_API_SVC: svc,
      }),
      0,
    );
    expect(r).toMatchObject({ ok: false, skipped: true, calls: 0 });
    expect(svc.calls()).toBe(0);
  });

  it("short dedicated key also skips even when a shared key exists", async () => {
    const svc = svcSeq([{ status: 200, body: drainResponse({ rows_sealed: 1 }) }]);
    const r = await runAuditDrainSweep(
      env({
        CORELINK_ERASE_AUTH_KEY: "too-short",
        CORELINK_INTERNAL_AUTH_KEY: "s".repeat(32),
        CORELINK_API_SVC: svc,
      }),
      0,
    );
    expect(r).toMatchObject({ ok: false, skipped: true, calls: 0 });
    expect(svc.calls()).toBe(0);
  });

  it("re-calls while `incomplete` is true and sums real handler counters", async () => {
    const svc = svcSeq([
      { status: 200, body: drainResponse({ rows_sealed: 200, partitions_drained: 2, incomplete: true }) },
      { status: 200, body: drainResponse({ rows_sealed: 200, partitions_drained: 2, incomplete: true }) },
      { status: 200, body: drainResponse({ rows_sealed: 40, partitions_drained: 1 }) },
    ]);
    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r).toMatchObject({ calls: 3, sealed: 440, partitions: 5, incomplete: false, ok: true });
  });

  it("replays the smallest above-budget B-125 burst as 512 + 1 without loss", async () => {
    // 513 is the smallest population that proves the production 512-row call
    // budget is a bound rather than a truncation point.  This stays entirely
    // behind the service-binding stub: production audit_outbox is append-only
    // and has no honest cleanup contract for synthetic rows.
    const svc = svcSeq([
      {
        status: 200,
        body: drainResponse({
          rows_sealed: 512,
          partitions_drained: 1,
          incomplete: true,
        }),
      },
      {
        status: 200,
        body: drainResponse({
          rows_sealed: 1,
          partitions_drained: 1,
          incomplete: false,
        }),
      },
    ]);

    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r).toMatchObject({
      calls: 2,
      sealed: 513,
      partitions: 2,
      incomplete: false,
      ok: true,
    });
    expect(svc.calls()).toBe(2);
  });

  it("stops at MAX_DRAIN_CALLS and leaves an honestly incomplete success visible", async () => {
    const svc = svcSeq([
      { status: 200, body: drainResponse({ rows_sealed: 200, partitions_drained: 1, incomplete: true }) },
    ]);
    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r).toMatchObject({
      calls: MAX_DRAIN_CALLS,
      sealed: 200 * MAX_DRAIN_CALLS,
      incomplete: true,
      ok: true,
    });
  });

  it("stops at the wall-clock deadline without starting another call", async () => {
    const now = vi.spyOn(Date, "now")
      .mockReturnValueOnce(1_000)
      .mockReturnValueOnce(1_000 + DRAIN_WALL_BUDGET_MS);
    try {
      const svc = svcSeq([
        { status: 200, body: drainResponse({ rows_sealed: 1, incomplete: true }) },
      ]);
      const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
      expect(r).toMatchObject({ calls: 1, sealed: 1, incomplete: true, ok: true });
      expect(svc.calls()).toBe(1);
    } finally {
      now.mockRestore();
    }
  });

  it("stops the loop on a non-2xx instead of storming the endpoint", async () => {
    const svc = svcSeq([
      { status: 200, body: drainResponse({ rows_sealed: 200, partitions_drained: 1, incomplete: true }) },
      { status: 503, body: "unavailable" },
    ]);
    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r).toMatchObject({ calls: 2, ok: false, status: 503, sealed: 200, incomplete: true });
  });

  it("treats a 200 ok:false with failed partitions as a terminal non-complete result", async () => {
    const svc = svcSeq([
      {
        status: 200,
        body: drainResponse({
          ok: false,
          rows_sealed: 10,
          partitions_drained: 1,
          partitions_failed: 2,
          incomplete: false,
        }),
      },
    ]);
    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r).toMatchObject({
      calls: 1,
      ok: false,
      incomplete: true,
      sealed: 10,
      partitions: 1,
      partitionsFailed: 2,
    });
    expect(svc.calls()).toBe(1);
  });

  it("treats a 200 ok:false as terminal even when partitions_failed is zero", async () => {
    // Mutation tooth: deleting the `!body.ok` arm while retaining the counter
    // check turns this exact 200 into a false green. The wire contract makes
    // `ok` independently load-bearing, so disagreement is never normalized.
    const svc = svcSeq([
      {
        status: 200,
        body: drainResponse({
          ok: false,
          rows_sealed: 10,
          partitions_drained: 1,
          partitions_failed: 0,
          incomplete: false,
        }),
      },
    ]);
    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r).toMatchObject({
      calls: 1,
      ok: false,
      incomplete: true,
      sealed: 10,
      partitions: 1,
      partitionsFailed: 0,
    });
    expect(svc.calls()).toBe(1);
  });

  it.each([
    ["non-JSON body", () => "not json at all"],
    ["missing counter", () => {
      const body = drainResponse();
      delete (body as Partial<DrainResponse>).rows_sealed;
      return body;
    }],
    ["missing ok", () => {
      const body = drainResponse({ rows_sealed: 1 });
      delete (body as Partial<DrainResponse>).ok;
      return body;
    }],
    ["non-boolean ok", () => ({
      ...drainResponse({ rows_sealed: 1 }),
      // Mutation tooth: without `typeof body.ok !== "boolean"`, this truthy
      // value bypasses `!body.ok` and turns a malformed 200 into false green.
      ok: "true",
    })],
    ["missing incomplete", () => {
      const body = drainResponse({ rows_sealed: 1 });
      delete (body as Partial<DrainResponse>).incomplete;
      return body;
    }],
    ["non-boolean incomplete", () => ({
      ...drainResponse({ rows_sealed: 1 }),
      // Mutation tooth: without `typeof body.incomplete !== "boolean"`, this
      // truthy value makes repeated calls look like a valid incomplete sweep.
      incomplete: "true",
    })],
    ["negative counter", () => drainResponse({ partitions_failed: -1 })],
    [
      "non-finite counter",
      () => '{"ok":true,"partitions_drained":0,"rows_sealed":1e400,"partitions_drifted":0,"partitions_failed":0,"partitions_leased":0,"heads_resigned":0,"incomplete":false}',
    ],
  ])("treats a 200 with %s as a terminal non-complete failure", async (_name, body) => {
    const svc = svcSeq([{ status: 200, body: body() }]);
    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r).toMatchObject({ calls: 1, ok: false, incomplete: true, sealed: 0, partitions: 0 });
    expect(svc.calls()).toBe(1);
  });

  it("rejects incomplete-with-no-progress instead of blindly retrying", async () => {
    const svc = svcSeq([{ status: 200, body: drainResponse({ incomplete: true }) }]);
    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r).toMatchObject({ calls: 1, ok: false, incomplete: true });
    expect(svc.calls()).toBe(1);
  });

  it("treats lease skips and drift as backpressure, not retry progress", async () => {
    const svc = svcSeq([
      {
        status: 200,
        body: drainResponse({
          partitions_leased: 1,
          partitions_drifted: 1,
          incomplete: true,
        }),
      },
    ]);
    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r).toMatchObject({
      calls: 1,
      ok: false,
      incomplete: true,
      sealed: 0,
      partitions: 0,
    });
    expect(svc.calls()).toBe(1);
  });

  it("preserves completed work before a transport failure and marks it non-complete", async () => {
    let n = 0;
    const fetchImpl = (async () => {
      n++;
      if (n === 1) {
        return new Response(
          JSON.stringify(drainResponse({ rows_sealed: 200, partitions_drained: 1, incomplete: true })),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      throw new Error("socket hang up");
    }) as unknown as typeof fetch;
    const r = await runAuditDrainSweep(
      env({ CORELINK_API_SVC: { fetch: fetchImpl } }),
      0,
    );
    expect(r).toMatchObject({ ok: false, status: 0, sealed: 200, calls: 2, incomplete: true });
  });
});
