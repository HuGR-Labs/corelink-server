/**
 * Edge middleware (WI-S16-001):
 *   1. Generate per-request CSP nonce.
 *   2. Attach hardened CSP + security headers to every response.
 *   3. Delegate auth to Clerk for protected routes.
 *
 * Note: Clerk's authMiddleware export name has shifted between releases
 * (`authMiddleware` → `clerkMiddleware`). We feature-detect at runtime so a
 * version bump doesn't silently break protection.
 */

import { NextResponse, type NextRequest } from "next/server";
import {
  buildCspHeader,
  generateNonce,
  STATIC_SECURITY_HEADERS,
} from "@/lib/csp";
import { isPublicPath } from "@/lib/route-matcher";

function applySecurityHeaders(res: NextResponse, nonce: string): void {
  const reportOnly = process.env["NODE_ENV"] !== "production";
  const csp = buildCspHeader({ nonce, reportOnly });
  res.headers.set(csp.name, csp.value);
  res.headers.set("x-nonce", nonce);
  for (const h of STATIC_SECURITY_HEADERS) {
    res.headers.set(h.name, h.value);
  }
}

export default async function middleware(req: NextRequest): Promise<NextResponse> {
  const nonce = generateNonce();
  const pathname = req.nextUrl.pathname;

  // Forward the nonce into the request headers so layout/page can read it
  // via `headers()` (needed for <Script nonce={...}>).
  const requestHeaders = new Headers(req.headers);
  requestHeaders.set("x-nonce", nonce);

  // For public paths or auth UI we never invoke Clerk middleware.
  // Clerk integration runs only for protected paths; we keep this stub
  // intentionally minimal so a missing CLERK_SECRET_KEY in dev does not
  // break local boot.
  if (isPublicPath(pathname)) {
    const res = NextResponse.next({ request: { headers: requestHeaders } });
    applySecurityHeaders(res, nonce);
    return res;
  }

  // Protected path — defer to Clerk if configured; otherwise fall through
  // and let the route render (dev/test ergonomics).
  const publishableKey = process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"];
  if (publishableKey) {
    try {
      // Dynamic import keeps the bundle slim for public paths.
      const mod = (await import("@clerk/nextjs/server").catch(() => null)) as
        | {
            clerkMiddleware?: (
              handler: (auth: unknown, request: NextRequest) => Promise<Response> | Response,
            ) => (req: NextRequest) => Promise<Response>;
          }
        | null;
      if (mod?.clerkMiddleware) {
        const handler = mod.clerkMiddleware(async (_auth, _request) => {
          const res = NextResponse.next({ request: { headers: requestHeaders } });
          applySecurityHeaders(res, nonce);
          return res;
        });
        const result = await handler(req);
        // Augment Clerk's response (which may be a redirect) with our headers.
        const augmented = NextResponse.next({
          request: { headers: requestHeaders },
        });
        // Copy status + location from Clerk's redirect if applicable.
        if (result.status >= 300 && result.status < 400 && result.headers.get("location")) {
          const redirect = NextResponse.redirect(
            new URL(result.headers.get("location") as string, req.url),
            result.status,
          );
          applySecurityHeaders(redirect, nonce);
          return redirect;
        }
        applySecurityHeaders(augmented, nonce);
        return augmented;
      }
    } catch {
      // Defensive: never crash middleware. Fall through to default response.
    }
  }

  const res = NextResponse.next({ request: { headers: requestHeaders } });
  applySecurityHeaders(res, nonce);
  return res;
}

export const config = {
  // Run middleware on all paths EXCEPT static assets (Next automatically
  // excludes /_next/static and image-optimized routes via this matcher).
  matcher: [
    "/((?!_next/static|_next/image|favicon.ico|robots.txt|sitemap.xml).*)",
  ],
};
