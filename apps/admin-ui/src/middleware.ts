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
import {
  isPublicPath,
  isSelfGatedPath,
  signInPathFor,
  signInRedirectPath,
} from "@/lib/route-matcher";

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

// `next dev` (React Fast Refresh + webpack HMR) and Clerk's dev SDK evaluate
// code via `eval`/`new Function`, which the production-strict CSP forbids. Under
// the enforced dev CSP that produced an unbounded storm of CSP-violation reports
// to `/api/csp-report` (rate-limited to 429s) that saturated the connection pool
// and made timing-sensitive E2E specs flaky. Allow `'unsafe-eval'` ONLY outside
// production so dev/E2E are stable; production output is unchanged and stays
// eval-free (verified by tests/csp.test.ts, which call the builders with the
// strict default).
function devAllowUnsafeEval(): boolean {
  return process.env["NODE_ENV"] !== "production";
}

function applySecurityHeaders(res: NextResponse, nonce: string): void {
  const csp = buildCspHeader({
    nonce,
    reportOnly: isReportOnly(),
    allowUnsafeEval: devAllowUnsafeEval(),
  });
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
    buildCspHeaderValue(nonce, devAllowUnsafeEval()),
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

  // E2E test mode (never set in production) authenticates via the
  // __corelink_e2e_session cookie at the DATA LAYER (lib/auth.ts), not via
  // Clerk. We must skip the Clerk middleware entirely here: invoking
  // clerkMiddleware without a CLERK_SECRET_KEY (absent in local/CI test envs)
  // THROWS "Missing secretKey", which the catch below turns into a
  // fail-closed /sign-in redirect — bouncing every authed page to a blank
  // screen. Skipping the block lets the route render and its own guard read
  // the mock session. Gating an enforce flag *inside* the handler (as before)
  // was too deep: the throw happens at clerkMiddleware construction, before
  // protect() is ever reached.
  // Double-gate to match the data-layer (`auth.ts`) + mock-route guards: the
  // auth-skip only applies in a genuine test build, NEVER in production — so a
  // stray `NEXT_PUBLIC_E2E_TEST_MODE=1` in a prod deploy cannot disable auth here.
  const isE2E =
    process.env["NEXT_PUBLIC_E2E_TEST_MODE"] === "1" &&
    process.env["NODE_ENV"] !== "production";

  // Protected path — defer to Clerk if configured; otherwise fall through
  // and let the route render (dev/test ergonomics).
  const publishableKey = process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"];
  const secretKey = process.env["CLERK_SECRET_KEY"];

  // Fail CLOSED on missing Clerk server config in PRODUCTION. Without this guard
  // an absent key skips or breaks the Clerk block below and can fall through to
  // the terminal NextResponse.next() — rendering a protected page with NO
  // middleware auth gate. Non-production still falls through for dev/test
  // ergonomics, and self-gated paths own their signed-out redirect.
  if (
    (!publishableKey || !secretKey) &&
    !isE2E &&
    process.env["NODE_ENV"] === "production" &&
    !isSelfGatedPath(pathname)
  ) {
    const redirectRes = NextResponse.redirect(new URL(signInPathFor(pathname), req.url));
    applySecurityHeaders(redirectRes, nonce);
    return redirectRes;
  }

  if (publishableKey && secretKey && !isE2E) {
    try {
      // Dynamic import keeps the bundle slim for public paths.
      const mod = (await import("@clerk/nextjs/server").catch(() => null)) as
        | {
            clerkMiddleware?: (
              handler: (
                auth: {
                  protect: (options?: {
                    unauthenticatedUrl?: string;
                    unauthorizedUrl?: string;
                  }) => Promise<unknown>;
                },
                request: NextRequest,
              ) => Promise<Response> | Response,
              options?: { signInUrl?: string },
            ) => (req: NextRequest) => Promise<Response>;
          }
        | null;
      if (mod?.clerkMiddleware) {
        // Self-gated routes (e.g. /upgrade) need the Clerk request context
        // (server-side `auth()` must resolve) but own their signed-out
        // redirect themselves — everything else is enforced here. (E2E test
        // mode is handled earlier by skipping this whole block.)
        const enforce = !isSelfGatedPath(pathname);
        const handler = mod.clerkMiddleware(
          async (auth, _request) => {
            if (enforce) {
              // Real enforcement: unauthenticated requests to protected paths
              // are redirected to sign-in by Clerk (return URL preserved).
              //
              // NOTE (fix/admin-ui-welcome-signed-out-redirect): a bare
              // `auth.protect()` is DOCUMENTED to redirect to signInUrl from
              // middleware (see @clerk/nextjs protect.d.ts), but in
              // @clerk/nextjs 7.x it instead throws a Next `notFound()` for
              // signed-out requests — which surfaced in prod as `/welcome`
              // and `/en/welcome` returning **404 instead of a 307 to
              // /sign-in** (the route exists; protect 404s before it renders).
              // Passing an explicit `unauthenticatedUrl` forces the intended
              // redirect and preserves the return URL so the user lands back on
              // the originally-requested page after signing in.
              await auth.protect({
                unauthenticatedUrl: new URL(
                  // Surface-correct sign-in (`/corelink/sign-in` on the path
                  // surface, `/sign-in` on the root subdomain). The return URL
                  // is the raw pathname+search — already surface-correct — so
                  // the user lands back on the requested page after signing in.
                  signInRedirectPath(
                    req.nextUrl.pathname,
                    req.nextUrl.pathname + req.nextUrl.search,
                  ),
                  req.url,
                ).toString(),
              });
            }
            const res = NextResponse.next({ request: { headers: requestHeaders } });
            applySecurityHeaders(res, nonce);
            return res;
          },
          // signInUrl MUST point at this app's real (locale-less) sign-in route,
          // surface-correct (`/corelink/sign-in` on the path surface) — a bare
          // `/sign-in` on `humangr.com` resolves to the apex marketing site, not
          // this app. Without it, `auth.protect()` on a signed-out request cannot
          // build a redirect and THROWS — which the catch below previously
          // swallowed, letting the request fall through and render a protected
          // page whose server-side `auth()` then 500'd (the /en/welcome outage).
          // With it, `protect()` returns a clean 307 to the app's sign-in and the
          // page never runs.
          { signInUrl: signInPathFor(pathname) },
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
        const redirectRes = NextResponse.redirect(new URL(signInPathFor(pathname), req.url));
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
