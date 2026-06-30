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
  buildCspHeaderValue,
  generateNonce,
  STATIC_SECURITY_HEADERS,
} from "@/lib/csp";
import { isPublicPath, isSelfGatedPath } from "@/lib/route-matcher";

// WI-S16-007 deliverable 4: CSP rollout flag.
//   CSP_ENFORCEMENT=enforce      → enforce mode (production default).
//   CSP_ENFORCEMENT=report-only  → report-only (staging baseline window).
//   unset                        → enforce in production, report-only elsewhere.
function isReportOnly(): boolean {
  const rawMode = (process.env["CSP_ENFORCEMENT"] ?? "").toLowerCase();
  if (rawMode === "enforce") return false;
  if (rawMode === "report-only") return true;
  return process.env["NODE_ENV"] !== "production";
}

function applySecurityHeaders(res: NextResponse, nonce: string): void {
  const csp = buildCspHeader({ nonce, reportOnly: isReportOnly() });
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
  // CRITICAL: Next auto-injects the nonce onto its own inline bootstrap scripts
  // by reading the per-request nonce from the `content-security-policy` REQUEST
  // header (not `x-nonce`, and not the report-only variant). Without this the
  // served CSP carries our per-request nonce but Next's inline scripts carry
  // none → they are blocked under enforce-mode CSP. We always set the enforce
  // header name here purely to feed Next's nonce extractor; the actual SERVED
  // response header still honors report-only mode via applySecurityHeaders().
  requestHeaders.set(
    "content-security-policy",
    buildCspHeaderValue(nonce),
  );

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
              handler: (
                auth: { protect: () => Promise<unknown> },
                request: NextRequest,
              ) => Promise<Response> | Response,
              options?: { signInUrl?: string },
            ) => (req: NextRequest) => Promise<Response>;
          }
        | null;
      if (mod?.clerkMiddleware) {
        // Self-gated routes (e.g. /upgrade) need the Clerk request context
        // (server-side `auth()` must resolve) but own their signed-out
        // redirect themselves — everything else is enforced here.
        const enforce = !isSelfGatedPath(pathname);
        const handler = mod.clerkMiddleware(
          async (auth, _request) => {
            if (enforce) {
              // Real enforcement: unauthenticated requests to protected paths
              // are redirected to sign-in by Clerk (return URL preserved).
              await auth.protect();
            }
            const res = NextResponse.next({ request: { headers: requestHeaders } });
            applySecurityHeaders(res, nonce);
            return res;
          },
          // signInUrl MUST point at this app's real (locale-less) sign-in route.
          // Without it, `auth.protect()` on a signed-out request cannot build a
          // redirect and THROWS — which the catch below previously swallowed,
          // letting the request fall through and render a protected page whose
          // server-side `auth()` then 500'd (the /en/welcome outage). With it,
          // `protect()` returns a clean 307 to /sign-in and the page never runs.
          { signInUrl: "/sign-in" },
        );
        const result = await handler(req);
        // Preserve Clerk's response verbatim (set-cookie, handshake headers,
        // and the sign-in redirect from `auth.protect()`) while guaranteeing
        // our security headers ride along on EVERY response — including
        // Clerk's redirects. Re-wrapping makes the headers mutable.
        const augmented = new NextResponse(result.body, result);
        applySecurityHeaders(augmented, nonce);
        return augmented;
      }
    } catch (err) {
      // Observability: a Clerk outage / version skew presents here as "everyone
      // bounced to sign-in" — log it so it's not mistaken for a login-conversion
      // drop. (Surfaced to the Worker logs + Sentry.)
      console.error("clerkMiddleware enforcement error", err);
      // Fail CLOSED: if Clerk enforcement errors on an ENFORCED path, never
      // fall through and serve the protected page anonymously (that both leaks
      // the page and 500s when its `auth()` finds no middleware context).
      // Redirect to sign-in instead. Self-gated paths own their gating, so they
      // fall through to render as before.
      if (!isSelfGatedPath(pathname)) {
        const redirectRes = NextResponse.redirect(new URL("/sign-in", req.url));
        applySecurityHeaders(redirectRes, nonce);
        return redirectRes;
      }
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
