/**
 * Route matcher used by middleware (WI-S16-001).
 *
 * Returns `true` when the path should be PROTECTED (Clerk auth required).
 * Public exemptions: /sign-in, /sign-up, /api/csp-report, /api/health,
 * /_next/*, /locales/*, and static asset prefixes.
 */

export const PUBLIC_PATH_PREFIXES: readonly string[] = [
  "/sign-in",
  "/sign-up",
  "/api/csp-report",
  "/api/health",
  "/_next/",
  "/locales/",
  "/favicon",
  "/robots.txt",
  "/sitemap.xml",
];

export function isPublicPath(pathname: string): boolean {
  for (const prefix of PUBLIC_PATH_PREFIXES) {
    if (pathname === prefix || pathname.startsWith(prefix)) return true;
  }
  return false;
}

export function isProtectedPath(pathname: string): boolean {
  return !isPublicPath(pathname);
}
