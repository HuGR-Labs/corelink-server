/**
 * Hardened Content-Security-Policy generation (WI-S16-001).
 *
 * NO `unsafe-inline`, NO `unsafe-eval`. Per-request nonce required for both
 * script and style tags. The nonce is generated in middleware and substituted
 * into the policy string just before the response is sent.
 *
 * Directives baseline:
 *   - default-src 'self'         (deny-by-default for most fetch contexts)
 *   - script-src  'self' nonce + clerk.corelink.dev
 *   - style-src   'self' nonce
 *   - connect-src 'self' + api.corelink.dev + clerk.corelink.dev
 *   - img-src     'self' data: https:
 *   - frame-ancestors 'none', form-action 'self', base-uri 'self'
 *   - report-uri /api/csp-report
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
    `script-src 'self' 'nonce-${nonce}' https://clerk.corelink.dev`,
    `style-src 'self' 'nonce-${nonce}'`,
    "img-src 'self' data: https:",
    "font-src 'self' data:",
    "connect-src 'self' https://api.corelink.dev https://clerk.corelink.dev",
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
  { name: "Permissions-Policy", value: "camera=(), microphone=(), geolocation=()" },
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

/** Assert (in tests) that a CSP header contains no dangerous keywords. */
export function containsUnsafeDirective(headerValue: string): boolean {
  return (
    headerValue.includes("'unsafe-inline'") ||
    headerValue.includes("'unsafe-eval'") ||
    headerValue.includes("unsafe-hashes")
  );
}
