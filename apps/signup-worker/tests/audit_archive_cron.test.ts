/**
 * Unit tests for the S-09 offsite audit-archive sweep cron.
 *
 * The container call is mocked via `env.CORELINK_API_SVC`. What matters here is
 * that the sweep reports the truth: a partial failure must not read as success,
 * and a missing internal-auth key must skip rather than 401-storm.
 */
import { describe, it, expect } from "vitest";
import {
  runAuditArchiveSweep,
  type AuditArchiveCronEnv,
} from "../src/webhooks/audit_archive_cron.js";

const KEY = "e".repeat(32);

/** Stub service binding returning `status` + `body`. */
function svc(status: number, body: unknown): { fetch: typeof fetch } {
  return {
    fetch: (async () =>
      new Response(typeof body === "string" ? body : JSON.stringify(body), {
        status,
        headers: { "content-type": "application/json" },
      })) as unknown as typeof fetch,
  };
}

function env(over: Partial<AuditArchiveCronEnv> = {}): AuditArchiveCronEnv {
  return {
    CORELINK_API_BASE: "https://corelink-api.example",
    CORELINK_ERASE_AUTH_KEY: KEY,
    ...over,
  };
}

describe("runAuditArchiveSweep", () => {
  it("reports the counts a successful sweep returned", async () => {
    const r = await runAuditArchiveSweep(
      env({
        CORELINK_API_SVC: svc(200, {
          rows_archived: 1200,
          chunks_created: 3,
          partitions_failed: 0,
          incomplete: true,
        }),
      }),
      0,
    );
    expect(r).toMatchObject({
      ok: true,
      status: 200,
      rowsArchived: 1200,
      chunksCreated: 3,
      partitionsFailed: 0,
      incomplete: true,
      skipped: false,
    });
  });

  it("surfaces the quarantine counts, and defaults them to 0 when absent", async () => {
    // A quarantined row is permanently outside the offsite evidence copy. If
    // the driver dropped these counts, the only place they would appear is a
    // container log line nobody reads — which is the silent-backlog defect the
    // quarantine path exists to remove.
    const r = await runAuditArchiveSweep(
      env({
        CORELINK_API_SVC: svc(200, {
          rows_archived: 120,
          chunks_created: 1,
          partitions_failed: 0,
          rows_quarantined: 131,
          partitions_quarantined: 1,
          incomplete: false,
        }),
      }),
      0,
    );
    expect(r).toMatchObject({
      ok: true,
      rowsArchived: 120,
      rowsQuarantined: 131,
      partitionsQuarantined: 1,
    });

    // An older container that predates migration 0100 omits the fields; that
    // must read as "none quarantined", never as NaN.
    const old = await runAuditArchiveSweep(
      env({ CORELINK_API_SVC: svc(200, { rows_archived: 5 }) }),
      0,
    );
    expect(old.rowsQuarantined).toBe(0);
    expect(old.partitionsQuarantined).toBe(0);
  });

  it("does NOT read a partial failure as success", async () => {
    // The handler answers 500 with a body when any partition failed. Parsing
    // the body on a non-2xx is deliberate: `partitions_failed` is the whole
    // diagnostic, and dropping it would leave the operator with a bare 500.
    const r = await runAuditArchiveSweep(
      env({
        CORELINK_API_SVC: svc(500, {
          rows_archived: 40,
          chunks_created: 1,
          partitions_failed: 2,
          incomplete: false,
        }),
      }),
      0,
    );
    expect(r.ok).toBe(false);
    expect(r.status).toBe(500);
    expect(r.partitionsFailed).toBe(2);
    // Rows that DID archive are still reported — a failure elsewhere must not
    // erase the work that landed.
    expect(r.rowsArchived).toBe(40);
  });

  it("skips (does not call) when no internal-auth key is bound", async () => {
    let called = false;
    const r = await runAuditArchiveSweep(
      {
        CORELINK_API_BASE: "https://corelink-api.example",
        CORELINK_API_SVC: {
          fetch: (async () => {
            called = true;
            return new Response("{}", { status: 200 });
          }) as unknown as typeof fetch,
        },
      },
      0,
    );
    expect(r.skipped).toBe(true);
    expect(r.ok).toBe(false);
    expect(called).toBe(false);
  });

  it("survives a non-JSON body without throwing", async () => {
    const r = await runAuditArchiveSweep(
      env({ CORELINK_API_SVC: svc(200, "not json") }),
      0,
    );
    expect(r.ok).toBe(true);
    expect(r.rowsArchived).toBe(0);
  });

  it("survives a transport throw and reports failure", async () => {
    const r = await runAuditArchiveSweep(
      env({
        CORELINK_API_SVC: {
          fetch: (async () => {
            throw new Error("connection reset");
          }) as unknown as typeof fetch,
        },
      }),
      0,
    );
    expect(r).toMatchObject({ ok: false, status: 0, skipped: false });
  });

  it("posts to the archive endpoint with the internal-auth header", async () => {
    let seenUrl = "";
    let seenAuth: string | null = null;
    await runAuditArchiveSweep(
      env({
        CORELINK_API_SVC: {
          fetch: (async (req: Request) => {
            seenUrl = req.url;
            seenAuth = req.headers.get("x-corelink-internal-auth");
            return Response.json({ rows_archived: 0 });
          }) as unknown as typeof fetch,
        },
      }),
      0,
    );
    expect(seenUrl).toBe("https://corelink-api.example/_internal/audit/archive");
    expect(seenAuth).toBe(KEY);
  });
});
