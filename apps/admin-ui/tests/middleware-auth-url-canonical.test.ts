/**
 * Middleware-level lock on the public buyer funnel (2026-08-03).
 *
 * Measured against PRODUCTION before the fix:
 *
 *   GET https://humangr.com/corelink/en/sign-up
 *     -> 307  location: /corelink/sign-in?redirect_url=%2Fcorelink%2Fen%2Fsign-up
 *     -> 200  at /corelink/sign-in?redirect_url=…
 *
 * A brand-new prospect clicking "Sign up free" on the pricing page was handed
 * the sign-IN screen, and the return target it carried did not exist — so
 * completing sign-in dropped them right back on the same dead URL.
 *
 * The link builders now emit the locale-less `/sign-up` (see
 * `resolveTierCtaHref`), but every `/`<locale>`/sign-{in,up}` URL already in
 * the wild — a bookmark, an email, a stale `redirect_url` — would keep taking
 * that path. This asserts the middleware canonicalizes it onto the real route
 * INSTEAD of treating it as a protected page.
 *
 * Note the precondition: no Clerk publishable key + NODE_ENV=production is the
 * fail-CLOSED configuration (tests/middleware-failclosed.test.ts) — i.e. the
 * harshest branch, the one that unconditionally 307s a protected path to
 * sign-in. If canonicalization holds HERE it holds everywhere.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NextRequest } from "next/server";

import middleware from "@/middleware";
import { APP_BASE_PATH } from "@/lib/route-matcher";

const LOCALES = ["en", "pt", "es", "de"] as const;

beforeEach(() => {
  vi.stubEnv("NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY", undefined);
  vi.stubEnv("NEXT_PUBLIC_E2E_TEST_MODE", undefined);
  vi.stubEnv("NODE_ENV", "production");
});

afterEach(() => {
  vi.unstubAllEnvs();
});

/** The shape Next actually hands the middleware (basePath already stripped). */
function req(path: string): NextRequest {
  return new NextRequest(new URL(`https://humangr.com${path}`));
}

/** The shape with the mount prefix still attached — must behave identically. */
function mountedReq(path: string): NextRequest {
  return new NextRequest(new URL(`https://humangr.com${APP_BASE_PATH}${path}`));
}

function locationOf(res: Response): URL {
  return new URL(res.headers.get("location") ?? "", "https://humangr.com");
}

describe("locale-prefixed auth URLs are canonicalized, not bounced to sign-IN", () => {
  it.each(LOCALES)(
    "/%s/sign-up 308s to the real sign-up page (was: 307 to /sign-in)",
    async (locale) => {
      const res = await middleware(req(`/${locale}/sign-up`));
      expect(res.status).toBe(308);
      const target = locationOf(res);
      expect(target.pathname).toBe(`${APP_BASE_PATH}/sign-up`);
      // The defect signature: landing on sign-IN instead of sign-UP.
      expect(target.pathname).not.toContain("/sign-in");
      // …and never carrying a redirect_url back to the dead locale path.
      expect(target.search).not.toContain("redirect_url");
      // Security headers ride along on the redirect like every other response.
      expect(res.headers.get("x-nonce")).toBeTruthy();
    },
  );

  it.each(LOCALES)("/%s/sign-in 308s to the real sign-in page", async (locale) => {
    const res = await middleware(req(`/${locale}/sign-in`));
    expect(res.status).toBe(308);
    expect(locationOf(res).pathname).toBe(`${APP_BASE_PATH}/sign-in`);
  });

  it("behaves identically when the pathname still carries the mount prefix", async () => {
    const res = await middleware(mountedReq("/pt/sign-up"));
    expect(res.status).toBe(308);
    // Never `/corelink/corelink/sign-up`.
    expect(locationOf(res).pathname).toBe(`${APP_BASE_PATH}/sign-up`);
  });

  it("preserves the buyer's intent on the query string", async () => {
    const res = await middleware(
      req("/de/sign-up?redirect_url=%2Fcorelink%2Fen%2Fupgrade%3Fplan%3Dpro"),
    );
    const target = locationOf(res);
    expect(target.pathname).toBe(`${APP_BASE_PATH}/sign-up`);
    expect(target.searchParams.get("redirect_url")).toBe(
      "/corelink/en/upgrade?plan=pro",
    );
  });

  it("keeps Clerk's own step routes under the widget path", async () => {
    const res = await middleware(req("/es/sign-up/verify-email-address"));
    expect(res.status).toBe(308);
    expect(locationOf(res).pathname).toBe(
      `${APP_BASE_PATH}/sign-up/verify-email-address`,
    );
  });
});

describe("canonicalization does not fire where it must not", () => {
  it("leaves the already-canonical sign-up alone (no redirect loop)", async () => {
    const res = await middleware(req("/sign-up"));
    expect(res.status).not.toBe(308);
    expect(res.headers.get("location")).toBeNull();
  });

  it("leaves the locale-mounted public pricing page alone", async () => {
    const res = await middleware(req("/en/pricing"));
    expect(res.status).not.toBe(308);
  });

  it("still fail-CLOSES a genuinely protected locale page", async () => {
    const res = await middleware(req("/en/customer"));
    expect(res.status).toBe(307);
    expect(locationOf(res).pathname).toBe(`${APP_BASE_PATH}/sign-in`);
  });
});
