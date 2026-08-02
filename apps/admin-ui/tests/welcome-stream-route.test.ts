/**
 * GET /api/welcome/stream — route handler tests.
 *
 * Regression cover for the two defects that made the `/welcome` pane unable to
 * display anything (W7):
 *   1. no `[[d1_databases]]` binding in apps/admin-ui/wrangler.toml, and
 *   2. the handler read bindings off `globalThis.__env__` (the `next-on-pages`
 *      convention) while admin-ui runs on `@opennextjs/cloudflare`, whose
 *      bindings arrive via `getCloudflareContext()`.
 *
 * Defect 2 is what these tests pin: with the old `__env__` read, a bound
 * `ANALYTICS_DB` is invisible and the stream can only ever emit heartbeats, so
 * "emits one SSE data frame per row" fails. Defect 1 is a config fact and is
 * asserted separately in `wrangler-analytics-binding.test.ts`.
 *
 * NOTE: nothing emits these activation events yet — the emitter is a separate
 * work package. These tests prove the READER would display a row if one
 * existed.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Hoisted mocks — registered BEFORE the route module is imported.
const { mockGetCloudflareContext, mockAuth } = vi.hoisted(() => ({
  mockGetCloudflareContext: vi.fn(),
  mockAuth: vi.fn(),
}));

vi.mock("@opennextjs/cloudflare", () => ({
  getCloudflareContext: mockGetCloudflareContext,
}));

vi.mock("@clerk/nextjs/server", () => ({
  auth: mockAuth,
}));

// Import the route AFTER the mocks are registered.
const { GET } = await import("@/app/api/welcome/stream/route");

const POLL_INTERVAL_MS = 1500;
const TENANT_ID = "tenant_w7_test";

type Row = {
  id: string;
  event_name: string;
  tenant_id: string;
  created_at: string;
};

/** Minimal D1 double that records the prepared SQL + bound params. */
function makeDb(rows: Row[]) {
  const all = vi.fn().mockResolvedValue({ results: rows });
  const bind = vi.fn((..._params: unknown[]) => ({ all }));
  const prepare = vi.fn((_sql: string) => ({ bind }));
  return { db: { prepare }, prepare, bind, all };
}

/** Read one already-enqueued chunk as text. */
async function readChunk(
  reader: ReadableStreamDefaultReader<Uint8Array>,
): Promise<string> {
  const { value } = await reader.read();
  return new TextDecoder().decode(value);
}

describe("GET /api/welcome/stream", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockGetCloudflareContext.mockReset();
    mockAuth.mockReset();
    mockAuth.mockResolvedValue({ sessionClaims: { tenant_id: TENANT_ID } });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("401s when there is no Clerk session", async () => {
    mockAuth.mockResolvedValue({ sessionClaims: {} });
    mockGetCloudflareContext.mockResolvedValue({ env: {} });

    const res = await GET();

    expect(res.status).toBe(401);
    await expect(res.json()).resolves.toEqual({ error: "no_active_session" });
  });

  it("reads ANALYTICS_DB from the Cloudflare context and emits one SSE data frame per row", async () => {
    const row: Row = {
      id: "evt_1",
      event_name: "first_cas_write",
      tenant_id: TENANT_ID,
      created_at: "2026-08-01T12:00:00.000Z",
    };
    const { db, prepare, bind } = makeDb([row]);
    mockGetCloudflareContext.mockResolvedValue({ env: { ANALYTICS_DB: db } });

    const res = await GET();
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toBe(
      "text/event-stream; charset=utf-8",
    );

    // Bindings must come from the opennextjs context, not `globalThis.__env__`.
    // The async overload is required so the call also resolves under `next dev`.
    expect(mockGetCloudflareContext).toHaveBeenCalledWith({ async: true });

    const reader = res.body!.getReader();
    expect(await readChunk(reader)).toContain(": connected");

    await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS + 50);

    expect(await readChunk(reader)).toBe("event: first_cas_write\n");
    const dataFrame = await readChunk(reader);
    expect(dataFrame.startsWith("data: ")).toBe(true);
    expect(dataFrame.endsWith("\n\n")).toBe(true);
    expect(JSON.parse(dataFrame.slice("data: ".length))).toEqual({
      id: "evt_1",
      event_name: "first_cas_write",
      created_at: "2026-08-01T12:00:00.000Z",
    });

    // The tenant filter is bound, never interpolated.
    expect(prepare.mock.calls[0]?.[0]).toContain("FROM analytics_events");
    expect(bind).toHaveBeenCalledWith(TENANT_ID, new Date(0).toISOString());

    await reader.cancel();
  });

  it("degrades to a heartbeat-only stream (no 500) when the binding is absent", async () => {
    mockGetCloudflareContext.mockResolvedValue({ env: {} });

    const res = await GET();
    expect(res.status).toBe(200);

    const reader = res.body!.getReader();
    expect(await readChunk(reader)).toContain(": connected");

    await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS + 50);
    // No poll-error frame: a missing binding is a no-op, not an exception.
    await vi.advanceTimersByTimeAsync(15_000);
    expect(await readChunk(reader)).toContain(": keepalive");

    await reader.cancel();
  });

  it("degrades to a heartbeat-only stream when the Cloudflare context is unavailable", async () => {
    mockGetCloudflareContext.mockRejectedValue(
      new Error("getCloudflareContext has been called without ..."),
    );

    const res = await GET();
    expect(res.status).toBe(200);

    const reader = res.body!.getReader();
    expect(await readChunk(reader)).toContain(": connected");

    await reader.cancel();
  });
});
