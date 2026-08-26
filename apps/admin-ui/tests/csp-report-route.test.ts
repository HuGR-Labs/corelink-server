/**
 * POST /api/csp-report — route handler regression cover.
 *
 * Two related defects live here, and the test file pins them down with one
 * case each so a future refactor cannot silently re-introduce either:
 *
 *   (1) `clientIp()` used to read `x-forwarded-for` first. On Cloudflare
 *       the edge APPENDS to that header — it does not overwrite — so a
 *       malicious caller can rotate the first hop on every request and
 *       obtain a fresh rate-limit bucket per call. `cf-connecting-ip` is
 *       set by the edge and is not client-spoofable, so it must be the
 *       primary key for rate limiting.
 *
 *   (2) The handler accepts a Reporting-API batch (an array of N reports)
 *       and used to issue one awaited `fetch` per item with no cap. A
 *       single 1000-item batch therefore triggered 1000 serial outbound
 *       calls from the Worker. The route now caps the fan-out at 32 and
 *       must surface the dropped count in a single `console.warn` so
 *       ops can see that reports were truncated — a silent cap would
 *       look like "all reports forwarded".
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NextRequest } from "next/server";

import { _resetRateLimiter } from "@/lib/rate-limit";
import { POST } from "@/app/api/csp-report/route";

function buildCspReq(headers: Record<string, string>, body: unknown): NextRequest {
  // NextRequest is just a Web Request with Next-flavored sugar; constructing
  // it from a standard Request is the documented way to drive the handler
  // from tests (see tests/checkout-session-route.test.ts).
  // No `RequestInit` annotation: the DOM lib types `signal` as
  // `AbortSignal | null`, while NextRequest's own RequestInit demands
  // `AbortSignal | undefined`, so annotating widens the literal into a
  // type tsc then rejects. Letting the literal infer keeps it assignable.
  return new NextRequest(new URL("https://humangr.com/corelink/api/csp-report"), {
    method: "POST",
    headers,
    body: typeof body === "string" ? body : JSON.stringify(body),
  });
}

describe("POST /api/csp-report", () => {
  // Captured per-test so individual cases can introspect the count of
  // outbound `fetch` calls without re-stubbing in their bodies.
  let fetchSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    _resetRateLimiter();
    // Pin the upstream so the handler doesn't try to reach corelink-api,
    // and stub `globalThis.fetch` so the (up to 100) outbound forwards in
    // the bucket-drain case resolve locally instead of actually hitting
    // the network.
    process.env.CSP_REPORT_ENDPOINT = "https://test.invalid/csp-violations";
    fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(new Response("{}", { status: 200 }));
  });

  afterEach(() => {
    _resetRateLimiter();
    delete process.env.CSP_REPORT_ENDPOINT;
    vi.restoreAllMocks();
  });

  it("rate-limits by cf-connecting-ip, not by client-spoofed x-forwarded-for", async () => {
    const cfIp = "198.51.100.10";

    // Drain the bucket at MAX_PER_WINDOW (100) for the legitimate edge IP,
    // rotating the spoofable XFF on every call. With the unfixed XFF-first
    // `clientIp()`, each of these 100 calls lands in a DIFFERENT bucket and
    // none of them is close to the cap — so the 101st call below would be
    // allowed, not 429ed.
    for (let i = 0; i < 100; i++) {
      const res = await POST(
        buildCspReq(
          {
            "cf-connecting-ip": cfIp,
            "x-forwarded-for": `203.0.113.${i + 1}`,
          },
          { "csp-report": { "document-uri": "https://example.com/" } },
        ),
      );
      expect(res.status).toBe(204);
    }

    // 101st call: same trusted edge IP, yet another spoofed XFF. With the
    // fix, the rate limiter sees the same `cfIp` key and the bucket is
    // full, so this must be 429. With the bug, `clientIp()` returns the
    // rotated XFF, opens a fresh bucket, and returns 204.
    const res = await POST(
      buildCspReq(
        {
          "cf-connecting-ip": cfIp,
          "x-forwarded-for": "203.0.113.999",
        },
        { "csp-report": { "document-uri": "https://example.com/" } },
      ),
    );

    expect(res.status).toBe(429);
    expect(res.headers.get("retry-after")).toBeTruthy();
  });

  it("caps a Reporting-API batch at 32 forwards and warns on the dropped count", async () => {
    // The fetch spy is installed in `beforeEach`; here we just observe its
    // call count. The handler also emits a `console.warn` for the cap —
    // stub that too so the test output stays clean AND so we can assert
    // on the exact message.
    const warnSpy = vi
      .spyOn(console, "warn")
      .mockImplementation(() => undefined);

    const reports = Array.from({ length: 100 }, (_, i) => ({
      "csp-report": { "document-uri": `https://example.com/p${i}` },
    }));

    const res = await POST(
      buildCspReq(
        {
          "cf-connecting-ip": "198.51.100.20",
        },
        reports,
      ),
    );

    // Response shape is unchanged: still 204 regardless of the cap.
    expect(res.status).toBe(204);

    // Hard cap: at most 32 outbound forwards, regardless of input length.
    expect(fetchSpy.mock.calls.length).toBeLessThanOrEqual(32);
    // And, given a 100-item batch, exactly 32 — the cap kicks in.
    expect(fetchSpy.mock.calls.length).toBe(32);

    // The dropped count (100 - 32 = 68) must be surfaced via a warn so ops
    // can see that reports were truncated. A silent cap would be a
    // "all reports forwarded" lie.
    const warnMessages = warnSpy.mock.calls.map((c) => String(c[0] ?? ""));
    const cappedWarn = warnMessages.find((m) => m.includes("csp-report fan-out capped"));
    expect(cappedWarn).toBeDefined();
    expect(cappedWarn).toContain("68");

    // Exactly one warn about the cap — not one per dropped item.
    const capWarns = warnMessages.filter((m) => m.includes("csp-report fan-out capped"));
    expect(capWarns).toHaveLength(1);
  });
});

