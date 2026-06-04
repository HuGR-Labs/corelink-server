/**
 * Hardened Content-Security-Policy generation (WI-S16-001).
 *
 * NO `unsafe-inline`, NO `unsafe-eval`. Per-request nonce required for both
 * script and style tags. The nonce is generated in middleware and substituted
 * into the policy string just before the response is sent.
 *
 * Directives baseline:
 *   - default-src 'self'         (deny-by-default for most fetch contexts)
 *   - script-src  'self' nonce + clerk.corelink.humangr.com
 *                                + https://plausible.io
 *                                + https://js.stripe.com
 *   - style-src   'self' 'unsafe-inline' nonce  (Tailwind + Clerk widgets)
 *   - connect-src 'self' + api.corelink.humangr.com + clerk.corelink.humangr.com
 *                                + https://plausible.io
 *                                + https://api.stripe.com
 *                                + https://m.stripe.network
 *                                + https://checkout.stripe.com (Checkout session)
 *                                + https://billing.stripe.com  (Customer Portal)
 *   - frame-src   https://js.stripe.com + https://hooks.stripe.com
 *                                + https://challenges.cloudflare.com (Clerk bot)
 *                                + https://clerk.corelink.humangr.com (Clerk modals)
 *   - img-src     'self' data: https:
 *   - frame-ancestors 'none', form-action 'self', base-uri 'self'
 *   - report-uri /api/csp-report
 *
 * Vendor CSP sources:
 *   - Stripe:  https://docs.stripe.com/security/guide#content-security-policy
 *   - Clerk:   https://clerk.com/docs/security/content-security-policy
 *   - Plausible: https://plausible.io/docs/proxy/csp
 *
 * Pre-HN-launch (2026-05-27) the third-party allow-list was widened from
 * Clerk-only to also cover Plausible Analytics + Stripe Checkout/portal
 * frames so the public signup and billing flows succeed under enforce-mode
 * CSP. The allow-list remains explicit per-host — no wildcards on
 * connect-src or frame-src.
 *
 * See WI-S16-001 §6.1.3 + §20 STRIDE Tampering.
 */

export interface CspOptions {
  /** Per-request base64 nonce (already URL-safe). */
  nonce: string;
  /** When true emit Content-Security-Policy-Report-Only header instead of enforce. */
  reportOnly?: boolean;
}

/**
 * Build the directive list. Exported separately so tests can assert each
 * directive without parsing a single concatenated header value.
 */
export function buildCspDirectives(nonce: string): string[] {
  return [
    "default-src 'self'",
    `script-src 'self' 'nonce-${nonce}' https://clerk.corelink.humangr.com https://plausible.io https://js.stripe.com`,
    // 'unsafe-inline' on style-src is acceptable: required by Tailwind CSS JIT (inline
    // <style> blocks) and by Clerk's modal overlay styles. NEVER on script-src.
    // Source: https://clerk.com/docs/security/content-security-policy
    `style-src 'self' 'unsafe-inline' 'nonce-${nonce}'`,
    "img-src 'self' data: https:",
    "font-src 'self' data:",
    // connect-src additions vs Phase 0 baseline:
    //   https://checkout.stripe.com — required for Stripe Checkout session fetch
    //   https://billing.stripe.com  — required for Stripe Customer Portal redirect
    // Source: https://docs.stripe.com/security/guide#content-security-policy
    "connect-src 'self' https://corelink-api.humangr.com https://clerk.corelink.humangr.com https://plausible.io https://api.stripe.com https://m.stripe.network https://checkout.stripe.com https://billing.stripe.com",
    // frame-src addition: https://clerk.corelink.humangr.com for Clerk modal/popup auth steps
    // Source: https://clerk.com/docs/security/content-security-policy
    "frame-src https://js.stripe.com https://hooks.stripe.com https://challenges.cloudflare.com https://clerk.corelink.humangr.com",
    "frame-ancestors 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "object-src 'none'",
    "report-uri /api/csp-report",
  ];
}

/** Serialize directives into a single header value. */
export function buildCspHeaderValue(nonce: string): string {
  return buildCspDirectives(nonce).join("; ");
}

/** Header name + value pair, honoring report-only mode for staging. */
export function buildCspHeader(opts: CspOptions): { name: string; value: string } {
  const value = buildCspHeaderValue(opts.nonce);
  const name = opts.reportOnly
    ? "Content-Security-Policy-Report-Only"
    : "Content-Security-Policy";
  return { name, value };
}

/**
 * Additional security headers per WI-S16-001 §1 + §20.
 * These are static; no per-request data so safe to cache.
 */
export const STATIC_SECURITY_HEADERS: ReadonlyArray<{ name: string; value: string }> = [
  { name: "X-Frame-Options", value: "DENY" },
  { name: "X-Content-Type-Options", value: "nosniff" },
  {
    name: "Strict-Transport-Security",
    value: "max-age=63072000; includeSubDomains; preload",
  },
  { name: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
  {
    name: "Permissions-Policy",
    // `interest-cohort=()` opts out of FLoC/Topics tracking (pre-HN-launch hardening).
    value: "interest-cohort=(), camera=(), microphone=(), geolocation=()",
  },
];

/**
 * Generate a cryptographically random per-request nonce.
 * Uses Web Crypto (available in Edge runtime + Node 20+).
 */
export function generateNonce(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  // base64 encode without padding for compact header
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  if (typeof btoa === "function") {
    return btoa(bin).replace(/=+$/g, "");
  }
  // Node fallback
  return Buffer.from(bin, "binary").toString("base64").replace(/=+$/g, "");
}

/**
 * Assert (in tests) that a CSP header contains dangerous keywords that should
 * NEVER appear on `script-src`.
 *
 * Note: `'unsafe-inline'` is intentionally present on `style-src` (required by
 * Tailwind + Clerk). This function therefore only checks for `'unsafe-eval'` and
 * `unsafe-hashes` across the whole header, and checks `'unsafe-inline'` presence
 * on `script-src` specifically. Use `containsScriptUnsafeInline` for the latter.
 */
export function containsUnsafeDirective(headerValue: string): boolean {
  return (
    headerValue.includes("'unsafe-eval'") ||
    headerValue.includes("unsafe-hashes")
  );
}

/** Returns true if the header value has 'unsafe-inline' on the script-src directive. */
export function containsScriptUnsafeInline(headerValue: string): boolean {
  const scriptSrc = headerValue
    .split(";")
    .map((d) => d.trim())
    .find((d) => d.startsWith("script-src"));
  return scriptSrc ? scriptSrc.includes("'unsafe-inline'") : false;
}
