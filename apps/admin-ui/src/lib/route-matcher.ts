/**
 * Route matcher used by middleware (WI-S16-001).
 *
 * Returns `true` when the path should be PROTECTED (Clerk auth required).
 * Public exemptions: the `/` landing page, /sign-in, /sign-up,
 * /api/csp-report, /api/health, /api/newsletter/subscribe, /_next/*,
 * /locales/*, static asset prefixes, and the locale-prefixed public
 * marketing/compliance pages (pricing, legal, privacy, security, 403, and the
 * pre-auth consent-capture leaf /consent/new).
 *
 * basePath (`/corelink`) awareness — THE load-bearing invariant:
 *   OpenNext (@opennextjs/cloudflare) invokes the edge middleware with
 *   `req.nextUrl.pathname` STILL CARRYING the configured `basePath`
 *   (`/corelink/sign-up`, `/corelink/_next/...`), unlike `next dev` which
 *   strips it. Every matcher here therefore normalizes the path through
 *   {@link stripBasePath} first, so a public path is public in BOTH the
 *   basePath-prefixed (prod) and basePath-less (dev/test) shapes. Without
 *   this, every `/corelink/*` public path fell through to Clerk and was 307'd
 *   to a basePath-less `/sign-in` (which resolves to the apex marketing site,
 *   not this app). Regression-locked in tests/route-matcher.test.ts.
 */

/** The path prefix this app is mounted under on the path-based surface. */
export const APP_BASE_PATH = "/corelink";

export const PUBLIC_PATH_PREFIXES: readonly string[] = [
  "/sign-in",
  "/sign-up",
  "/api/csp-report",
  "/api/health",
  // Deliberately public top-of-funnel: the docs site POSTs here
  // cross-origin with no credentials (see the route's CORS notes).
  // Rate-limited per-IP in the route handler.
  "/api/newsletter/subscribe",
  "/_next/",
  "/locales/",
  "/favicon",
  "/robots.txt",
  "/sitemap.xml",
  "/llms.txt",
  "/pricing.txt",
];

/**
 * Public marketing/compliance pages that live under the `/[locale]/`
 * segment (and are also public at their locale-less shape). These must
 * never require auth: they are the pre-signup funnel (docs CTAs, footer
 * legal links) plus the canonical 403 landing RbacGuard links to.
 */
export const PUBLIC_LOCALE_PAGE_PREFIXES: readonly string[] = [
  "/pricing",
  "/legal",
  "/privacy",
  "/security",
  "/403",
  // The consent-CAPTURE page (`/[locale]/consent/new`) is a public, pre-auth
  // compliance surface: it renders the disclosed-purpose notice + grant form
  // with NO user-data read (see the page + ConsentCaptureFlow — no `auth()`),
  // POSTing only to the backend `/v1/consent/grant` (which enforces its own
  // auth). It is one of the three Lighthouse-audited public routes (S-16 DoD).
  // NOTE: intentionally the `/consent/new` LEAF, not the `/consent` tree —
  // `/consent/history` and `/consent/withdraw/:id` are user-specific and MUST
  // stay protected. Was previously (mis)treated as protected: the fail-closed
  // sign-in redirect masked it (it landed on a 200 sign-in), until the
  // `/corelink` basePath turned that hop into a 404 and broke the gate.
  "/consent/new",
];

// Locales per src/i18n/request.ts LOCALES. Kept as a literal so this
// module stays import-free (middleware bundles it for the edge runtime).
const LOCALE_SEGMENT_RE = /^\/(?:en|pt|es|de)(?=\/|$)/;

/**
 * Reduce a raw middleware pathname to its app-relative shape by removing the
 * `basePath` prefix. Tolerates the basePath being:
 *   - present  → `/corelink/sign-up`  (prod / OpenNext — the real shape)
 *   - absent   → `/sign-up`           (dev / `next dev` / unit tests)
 *   - doubled  → `/corelink/corelink/…` (defensive vs a future rewrite)
 * and returns "/" for the bare app root (`/corelink` or `/corelink/`).
 */
function stripBasePath(pathname: string): string {
  let p = pathname || "/";
  while (p === APP_BASE_PATH || p.startsWith(`${APP_BASE_PATH}/`)) {
    p = p.slice(APP_BASE_PATH.length) || "/";
  }
  return p;
}

/**
 * Paths that MUST run `clerkMiddleware` (so the server-side `auth()`
 * probe resolves) but own their signed-out handling themselves — the
 * middleware must NOT `auth.protect()` them. `/upgrade` performs its own
 * `/sign-in?redirect_url=…` round-trip preserving `?plan=` (see
 * `[locale]/upgrade/page.tsx` — it gates on exactly the predicate the
 * checkout POST authenticates with).
 */
export const SELF_GATED_PAGE_PREFIXES: readonly string[] = ["/upgrade"];

function matchesPagePrefix(pathname: string, prefixes: readonly string[]): boolean {
  const delocalized = stripBasePath(pathname).replace(LOCALE_SEGMENT_RE, "") || "/";
  for (const prefix of prefixes) {
    if (delocalized === prefix || delocalized.startsWith(`${prefix}/`)) {
      return true;
    }
  }
  return false;
}

export function isPublicPath(pathname: string): boolean {
  const normalized = stripBasePath(pathname);
  // The root landing page is the public top-of-funnel (signup CTA target).
  if (normalized === "/") return true;
  for (const prefix of PUBLIC_PATH_PREFIXES) {
    if (normalized === prefix || normalized.startsWith(prefix)) return true;
  }
  return matchesPagePrefix(normalized, PUBLIC_LOCALE_PAGE_PREFIXES);
}

/** See {@link SELF_GATED_PAGE_PREFIXES}. */
export function isSelfGatedPath(pathname: string): boolean {
  return matchesPagePrefix(pathname, SELF_GATED_PAGE_PREFIXES);
}

export function isProtectedPath(pathname: string): boolean {
  return !isPublicPath(pathname);
}

/**
 * The mount prefix a hand-built redirect must re-attach. Always
 * {@link APP_BASE_PATH} — the app is served on exactly ONE surface.
 *
 * This used to branch on whether `pathname` already carried `/corelink`, to
 * support the subdomain→path migration where the app answered on BOTH
 * `corelink-app.humangr.com/*` (no prefix) and `humangr.com/corelink/*`. That
 * branch was wrong in production for two independent reasons, and the two
 * cancelled out into a silent breakage:
 *
 *   1. **Next strips `basePath` BEFORE middleware runs.** A request to
 *      `humangr.com/corelink/dashboard` reaches the middleware as `/dashboard`,
 *      so the `startsWith("/corelink")` test never matched and the function
 *      always returned "" — the branch written for the subdomain.
 *   2. **The subdomain surface is retired.** Both `corelink-app` and
 *      `corelink-admin` had their `custom_domain` bindings removed
 *      (`apps/admin-ui/wrangler.toml`), so the no-prefix surface the ""
 *      branch existed for no longer answers at all.
 *
 * Live consequence before this fix: `/corelink/dashboard` 307'd to
 * `humangr.com/sign-in`, which is the APEX MARKETING SITE (a different app,
 * 42 KB landing page) rather than this app's sign-in at `/corelink/sign-in`.
 * Every logged-out user sent to a protected route landed on marketing — the
 * user-visible "signup is broken" report.
 *
 * The unit tests did not catch it: they fed `signInPathFor` a pathname WITH the
 * prefix (an input the middleware never receives in production), and the
 * companion case asserted the no-prefix output as correct.
 *
 * (Next only auto-applies `basePath` to framework-generated links, never to
 * URLs the middleware builds by hand — hence the manual re-attachment.)
 */
export function requestBasePath(_pathname: string): string {
  return APP_BASE_PATH;
}

/** Surface-correct, app-absolute `/sign-in` path for the request's pathname. */
export function signInPathFor(pathname: string): string {
  return `${requestBasePath(pathname)}/sign-in`;
}

/**
 * Build the middleware's sign-in redirect target for the surface the request
 * arrived on, optionally preserving the post-auth return URL. `returnTo` is
 * passed through verbatim (it is already surface-correct — the raw
 * `pathname + search`) so the user lands back on the requested page.
 */
export function signInRedirectPath(pathname: string, returnTo?: string): string {
  const base = signInPathFor(pathname);
  return returnTo ? `${base}?redirect_url=${encodeURIComponent(returnTo)}` : base;
}

/**
 * Prefix an INTERNAL absolute href with the app {@link APP_BASE_PATH}, for the
 * hand-built links Next does NOT auto-basePath (a raw `<a href>` or the kit
 * `<Button href>` anchor — see the module header: Next only auto-applies
 * `basePath` to framework-generated links like `next/link`/`router.push`, never
 * to hrefs built by hand). Without this a `<Button href="/upgrade">` /
 * `<a href="/api/install/github">` navigates to `humangr.com/upgrade` (the apex
 * marketing site), NOT `humangr.com/corelink/upgrade` — the class of bug that
 * broke the runner Install button.
 *
 * Left UNTOUCHED: external (`http(s):`, `mailto:`, protocol-relative `//`),
 * hash-only (`#…`), relative (no leading `/`), and already-prefixed
 * (`/corelink…`) hrefs. Idempotent.
 */
export function withAppBasePath(href: string): string {
  if (!href.startsWith("/") || href.startsWith("//")) {
    return href; // relative, protocol-relative, or non-path (mailto:/http:) — leave as-is.
  }
  if (href === APP_BASE_PATH || href.startsWith(`${APP_BASE_PATH}/`)) {
    return href; // already prefixed — idempotent, never double-prefix.
  }
  return `${APP_BASE_PATH}${href}`;
}
