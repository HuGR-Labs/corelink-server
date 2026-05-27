/**
 * Defense-in-depth security headers applied to EVERY response from the
 * signup worker (pre-HN-launch hardening).
 *
 * The signup worker only serves machine-to-machine webhook traffic
 * (Clerk `user.created`, future Stripe `checkout.session.completed`) plus
 * a `/health` JSON probe. No HTML is ever rendered. CSP is therefore
 * strict-by-default (`default-src 'none'`) so that any accidental HTML
 * response (e.g. error page injected by an intermediate proxy) cannot
 * execute scripts in a browser if a user were ever pointed here.
 *
 * Target: securityheaders.com B+ or better post-deploy.
 */
export const SECURITY_HEADERS: Readonly<Record<string, string>> = {
  "Content-Security-Policy":
    "default-src 'none'; frame-ancestors 'none'; base-uri 'none'",
  "Strict-Transport-Security":
    "max-age=63072000; includeSubDomains; preload",
  "X-Content-Type-Options": "nosniff",
  "X-Frame-Options": "DENY",
  "Referrer-Policy": "no-referrer",
  "Permissions-Policy":
    "interest-cohort=(), camera=(), microphone=(), geolocation=()",
};

/**
 * Merge `SECURITY_HEADERS` onto a `Response`. Existing per-route values
 * (e.g. `Content-Type`) are preserved; security headers only fill gaps.
 * Returns a new `Response` so the original is untouched.
 */
export function withSecurityHeaders(res: Response): Response {
  const merged = new Headers(res.headers);
  for (const [name, value] of Object.entries(SECURITY_HEADERS)) {
    if (!merged.has(name)) merged.set(name, value);
  }
  return new Response(res.body, {
    status: res.status,
    statusText: res.statusText,
    headers: merged,
  });
}
