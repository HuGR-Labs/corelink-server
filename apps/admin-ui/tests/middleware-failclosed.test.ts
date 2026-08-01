/**
 * Fail-CLOSED regression for the root Next.js edge middleware
 * (apps/admin-ui/src/middleware.ts, fix/ts-failclosed-middleware-and-oauth-public-gate).
 *
 * Defect: when NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY is ABSENT the middleware skipped
 * the Clerk enforcement block and fell through to NextResponse.next(), rendering a
 * PROTECTED page with NO middleware auth gate (a fail-OPEN in production).
 *
 * Contract asserted here:
 *   (a) production + no publishable key + protected path → 307 redirect to /sign-in
 *       (fail-CLOSED, mirroring the catch-branch), NEVER a pass-through.
 *   (b) non-production + no publishable key → falls through (dev/test ergonomics
 *       preserved — a missing key must not break local boot).
 *   (c) production + no publishable key + SELF-GATED path (/upgrade) → falls
 *       through (self-gated paths own their own gating).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NextRequest } from "next/server";

import middleware from "@/middleware";

const KEY_ENV = "NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY";

beforeEach(() => {
  // No publishable key + no E2E bypass is the defect precondition. `vi.stubEnv`
  // keeps the (read-only-typed) NODE_ENV mutations scoped + auto-restorable.
  vi.stubEnv(KEY_ENV, undefined);
  vi.stubEnv("NEXT_PUBLIC_E2E_TEST_MODE", undefined);
});

afterEach(() => {
  vi.unstubAllEnvs();
});

function reqFor(path: string): NextRequest {
  return new NextRequest(new URL(`https://humangr.com/corelink${path}`));
}

// A request whose pathname does NOT carry the `/corelink` mount. This is the
// shape the RETIRED subdomain surface (`corelink-app` / `corelink-admin`,
// custom_domain bindings removed — both NXDOMAIN, re-verified 2026-08-01)
// used to produce, and it is ALSO the shape Next hands the middleware on the
// live path surface, since basePath is stripped before middleware runs.
// Either way the redirect must re-attach `/corelink`.
function rootReqFor(path: string): NextRequest {
  return new NextRequest(new URL(`https://humangr.com${path}`));
}

describe("middleware fail-CLOSED on absent publishable key", () => {
  it("(a) production + no key + protected path → 307 redirect to /sign-in", async () => {
    vi.stubEnv("NODE_ENV", "production");
    const res = await middleware(reqFor("/admin/tenants"));
    expect(res.status).toBe(307);
    const location = res.headers.get("location") ?? "";
    // basePath-ABSOLUTE: a bare `/sign-in` resolves to the apex marketing site,
    // not this app — the redirect MUST carry the `/corelink` basePath.
    expect(location).toContain("/corelink/sign-in");
    // Security headers still ride along on the fail-closed redirect.
    expect(res.headers.get("x-nonce")).toBeTruthy();
  });

  it("(a2) path surface → redirect target carries the /corelink mount prefix", async () => {
    vi.stubEnv("NODE_ENV", "production");
    const res = await middleware(reqFor("/admin/tenants"));
    expect(res.status).toBe(307);
    expect(res.headers.get("location") ?? "").toContain("/corelink/sign-in");
  });

  it("(a3) prefix-less pathname → STILL /corelink/sign-in, never the bare apex path", async () => {
    vi.stubEnv("NODE_ENV", "production");
    const res = await middleware(rootReqFor("/admin/tenants"));
    expect(res.status).toBe(307);
    const location = new URL(
      res.headers.get("location") ?? "",
      "https://humangr.com",
    );
    // This case used to assert a bare `/sign-in`, which was the whole bug: a
    // bare `/sign-in` on humangr.com serves the hugr-site MARKETING landing
    // (HTTP 200, title "HuGR — CoreLink · content-addressed cache & execution
    // fabric", zero Clerk — measured live 2026-08-01), so every logged-out
    // user hitting a protected route was stranded on marketing. The old
    // suite was green from both sides because a companion case asserted the
    // prefixed output as correct for a prefixed input production never sends.
    expect(location.pathname).toBe("/corelink/sign-in");
    expect(location.pathname).not.toBe("/sign-in");
  });

  it("(b) non-production + no key → falls through (dev/test ergonomics)", async () => {
    vi.stubEnv("NODE_ENV", "development");
    const res = await middleware(reqFor("/admin/tenants"));
    // A pass-through response is NOT a redirect (no /sign-in location).
    expect(res.status).not.toBe(307);
    expect(res.headers.get("location")).toBeNull();
  });

  it("(c) production + no key + self-gated path (/upgrade) → falls through", async () => {
    vi.stubEnv("NODE_ENV", "production");
    const res = await middleware(reqFor("/upgrade"));
    // Self-gated routes own their own signed-out handling → not force-redirected.
    expect(res.status).not.toBe(307);
    expect(res.headers.get("location")).toBeNull();
  });
});
