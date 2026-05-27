/**
 * Defense-in-depth security headers applied to EVERY response from the
 * analytics worker (pre-HN-launch hardening).
 *
 * The analytics worker exposes:
 *   - POST /v1/event   (CORS-checked, JSON only)
 *   - GET  /healthz
 *   - GET  /v1/digest/preview  (dev only)
 *
 * No HTML is ever served. CSP is strict-by-default (`default-src 'none'`)
 * because no browser document path exists; this also ensures error pages
 * cannot accidentally be rendered as HTML by misconfigured intermediates.
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
    "Referrer-Policy": "strict-origin-when-cross-origin",
    "Permissions-Policy":
        "interest-cohort=(), camera=(), microphone=(), geolocation=()",
};

/**
 * Merge `SECURITY_HEADERS` onto a `Response`. Existing per-route values
 * (e.g. CORS `Access-Control-Allow-Origin`) are preserved; security
 * headers only fill gaps. Returns a new `Response` so the original is
 * untouched (important when the body is a stream).
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
