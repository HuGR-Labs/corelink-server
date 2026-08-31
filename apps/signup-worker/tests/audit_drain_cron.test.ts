/**
 * Unit tests for the S-09 audit-chain drain sweep cron (B-064).
 *
 * The container call is mocked via `env.CORELINK_API_SVC`. Two things are under
 * test and both were broken in prod: the sweep must read the keys the Rust
 * handler actually emits (`rows_sealed` / `partitions_drained`, NOT
 * `sealed` / `partitions`), and it must re-call while the handler reports
 * `incomplete: true` instead of leaving a per-call row budget to act as a
 * per-hour platform ceiling.
 */
import { describe, it, expect } from "vitest";
import {
  runAuditDrainSweep,
  MAX_DRAIN_CALLS,
  type AuditDrainCronEnv,
} from "../src/webhooks/audit_drain_cron.js";

const KEY = "e".repeat(32);

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
  it("reads the handler's real keys — `rows_sealed`/`partitions_drained`", async () => {
    const r = await runAuditDrainSweep(
      env({
        CORELINK_API_SVC: svcSeq([
          {
            status: 200,
            body: {
              ok: true,
              rows_sealed: 200,
              partitions_drained: 3,
              partitions_failed: 0,
              incomplete: false,
            },
          },
        ]),
      }),
      0,
    );
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

  it("reports ZERO for the OLD keys — the exact prod defect (`j.sealed`)", async () => {
    // Regression teeth: a body carrying only the pre-B-064 key names must not
    // be readable as work done. If someone reintroduces `j.sealed`, this test
    // is the one that goes red.
    const r = await runAuditDrainSweep(
      env({
        CORELINK_API_SVC: svcSeq([
          { status: 200, body: { ok: true, sealed: 200, partitions: 3 } },
        ]),
      }),
      0,
    );
    expect(r.sealed).toBe(0);
    expect(r.partitions).toBe(0);
  });

  it("re-calls while `incomplete` is true and SUMS the rows", async () => {
    const svc = svcSeq([
      { status: 200, body: { rows_sealed: 200, partitions_drained: 2, incomplete: true } },
      { status: 200, body: { rows_sealed: 200, partitions_drained: 2, incomplete: true } },
      { status: 200, body: { rows_sealed: 40, partitions_drained: 1, incomplete: false } },
    ]);
    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r.calls).toBe(3);
    expect(r.sealed).toBe(440);
    expect(r.partitions).toBe(5);
    expect(r.incomplete).toBe(false);
    expect(r.ok).toBe(true);
  });

  it("stops at MAX_DRAIN_CALLS and reports `incomplete: true`", async () => {
    // A backlog that never says "done" must be bounded, and the leftover must
    // be VISIBLE — an exhausted budget that reads as success is the defect
    // class this whole item is about.
    const svc = svcSeq([
      { status: 200, body: { rows_sealed: 200, partitions_drained: 1, incomplete: true } },
    ]);
    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r.calls).toBe(MAX_DRAIN_CALLS);
    expect(r.sealed).toBe(200 * MAX_DRAIN_CALLS);
    expect(r.incomplete).toBe(true);
  });

  it("stops the loop on a non-2xx instead of storming the endpoint", async () => {
    const svc = svcSeq([
      { status: 200, body: { rows_sealed: 200, partitions_drained: 1, incomplete: true } },
      { status: 503, body: "unavailable" },
    ]);
    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r.calls).toBe(2);
    expect(r.ok).toBe(false);
    expect(r.status).toBe(503);
    // The rows the first call DID seal are still reported.
    expect(r.sealed).toBe(200);
  });

  it("carries `partitions_failed` through instead of dropping it", async () => {
    const r = await runAuditDrainSweep(
      env({
        CORELINK_API_SVC: svcSeq([
          {
            status: 200,
            body: {
              rows_sealed: 10,
              partitions_drained: 1,
              partitions_failed: 2,
              incomplete: false,
            },
          },
        ]),
      }),
      0,
    );
    expect(r.partitionsFailed).toBe(2);
  });

  it("stops after one call on a non-JSON 200 rather than looping blind", async () => {
    const svc = svcSeq([{ status: 200, body: "not json at all" }]);
    const r = await runAuditDrainSweep(env({ CORELINK_API_SVC: svc }), 0);
    expect(r.calls).toBe(1);
    expect(r.ok).toBe(true);
    expect(r.incomplete).toBe(false);
  });

  it("skips (and never calls) when no internal-auth key is bound", async () => {
    const svc = svcSeq([{ status: 200, body: { rows_sealed: 1 } }]);
    const r = await runAuditDrainSweep(
      {
        CORELINK_API_BASE: "https://corelink-api.example",
        CORELINK_API_SVC: svc,
      },
      0,
    );
    expect(r.skipped).toBe(true);
    expect(r.calls).toBe(0);
    expect(svc.calls()).toBe(0);
  });

  it("reports ok:false and the rows already sealed when the transport throws", async () => {
    let n = 0;
    const fetchImpl = (async () => {
      n++;
      if (n === 1) {
        return new Response(
          JSON.stringify({ rows_sealed: 200, partitions_drained: 1, incomplete: true }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      throw new Error("socket hang up");
    }) as unknown as typeof fetch;
    const r = await runAuditDrainSweep(
      env({ CORELINK_API_SVC: { fetch: fetchImpl } }),
      0,
    );
    expect(r.ok).toBe(false);
    expect(r.status).toBe(0);
    expect(r.sealed).toBe(200);
    expect(r.calls).toBe(2);
  });
});
