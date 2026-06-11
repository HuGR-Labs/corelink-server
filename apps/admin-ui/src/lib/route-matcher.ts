/**
 * Route matcher used by middleware (WI-S16-001).
 *
 * Returns `true` when the path should be PROTECTED (Clerk auth required).
 * Public exemptions: the `/` landing page, /sign-in, /sign-up,
 * /api/csp-report, /api/health, /api/newsletter/subscribe, /_next/*,
 * /locales/*, static asset prefixes, and the locale-prefixed public
 * marketing/compliance pages (pricing, legal, privacy, security, 403).
 */

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
];

// Locales per src/i18n/request.ts LOCALES. Kept as a literal so this
// module stays import-free (middleware bundles it for the edge runtime).
const LOCALE_SEGMENT_RE = /^\/(?:en|pt|es|de)(?=\/|$)/;

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
  const delocalized = pathname.replace(LOCALE_SEGMENT_RE, "") || "/";
  for (const prefix of prefixes) {
    if (delocalized === prefix || delocalized.startsWith(`${prefix}/`)) {
      return true;
    }
  }
  return false;
}

export function isPublicPath(pathname: string): boolean {
  // The root landing page is the public top-of-funnel (signup CTA target).
  if (pathname === "/") return true;
  for (const prefix of PUBLIC_PATH_PREFIXES) {
    if (pathname === prefix || pathname.startsWith(prefix)) return true;
  }
  return matchesPagePrefix(pathname, PUBLIC_LOCALE_PAGE_PREFIXES);
}

/** See {@link SELF_GATED_PAGE_PREFIXES}. */
export function isSelfGatedPath(pathname: string): boolean {
  return matchesPagePrefix(pathname, SELF_GATED_PAGE_PREFIXES);
}

export function isProtectedPath(pathname: string): boolean {
  return !isPublicPath(pathname);
}
