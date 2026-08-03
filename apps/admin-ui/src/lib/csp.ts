/**
 * Hardened Content-Security-Policy generation (WI-S16-001).
 *
 * NO `unsafe-inline`, NO `unsafe-eval`. Per-request nonce required for both
 * script and style tags. The nonce is generated in middleware and substituted
 * into the policy string just before the response is sent.
 *
 * Directives baseline:
 *   - default-src 'self'         (deny-by-default for most fetch contexts)
 *   - script-src  'self' nonce + clerk.corelink-app.humangr.com
 *                                + https://challenges.cloudflare.com (Clerk Smart
 *                                  CAPTCHA / Turnstile — injected by the widget)
 *                                + https://plausible.io
 *                                + https://js.stripe.com
 *   - style-src   'self' 'unsafe-inline'  (Tailwind + Clerk inline widget styles;
 *                                NO nonce — a nonce disables 'unsafe-inline' per CSP3)
 *   - connect-src 'self' + corelink-api.humangr.com + corelink-analytics.humangr.com
 *                                + clerk.corelink-app.humangr.com
 *                                + https://plausible.io
 *                                + https://api.stripe.com
 *                                + https://m.stripe.network
 *                                + https://checkout.stripe.com (Checkout session)
 *                                + https://billing.stripe.com  (Customer Portal)
 *   - frame-src   https://js.stripe.com + https://hooks.stripe.com
 *                                + https://challenges.cloudflare.com (Clerk bot)
 *                                + https://clerk.corelink-app.humangr.com (Clerk modals)
 *   - img-src     'self' data: https:
 *   - frame-ancestors 'none', form-action 'self', base-uri 'self'
 *   - report-uri /corelink/api/csp-report  (see {@link CSP_REPORT_PATH})
 *   - report-to  csp-endpoint             (+ the `Reporting-Endpoints` header)
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

import { withAppBasePath } from "./route-matcher";

/**
 * Surface-correct URL of the CSP violation sink.
 *
 * **Why this is not the literal `/api/csp-report`.** A `report-uri` /
 * `Reporting-Endpoints` value is a URL reference the browser resolves against
 * the *document's* URL, so a root-absolute path lands on the ORIGIN root — but
 * the handler (`src/app/api/csp-report/route.ts`) is mounted under the app's
 * Next `basePath` (`/corelink`), and Next never rewrites a path that we
 * hand-embed in a header value. Proven live 2026-08-03 on
 * `https://humangr.com/corelink/sign-in`, whose enforce-mode header carried
 * `report-uri /api/csp-report`:
 *
 *     POST https://humangr.com/api/csp-report          -> 405  (apex marketing
 *                                                              Pages app; the
 *                                                              report is lost)
 *     POST https://humangr.com/corelink/api/csp-report -> 204  (the real sink)
 *
 * Under `next dev` the same basePath-less URL 404s. Either way EVERY violation
 * report this app believed it collected was discarded — the pipe had never
 * carried a single report since the `/corelink` path-surface migration.
 *
 * Derived from {@link withAppBasePath} rather than re-typing `/corelink`, so
 * this cannot drift from `next.config.ts`'s `basePath` the way a second literal
 * would. Same failure mode as the Clerk-widget basePath blindness (#894), here
 * in our own code.
 */
export const CSP_REPORT_PATH: string = withAppBasePath("/api/csp-report");

/**
 * Reporting API v1 group name shared by the `report-to` CSP directive and the
 * `Reporting-Endpoints` response header. The two MUST agree or Chromium drops
 * the reports silently.
 */
export const CSP_REPORT_GROUP = "csp-endpoint";

/**
 * `Reporting-Endpoints` header (Reporting API v1) naming {@link CSP_REPORT_PATH}
 * under {@link CSP_REPORT_GROUP}.
 *
 * Emitted ALONGSIDE the deprecated `report-uri`, never instead of it — the two
 * cover disjoint browser populations and dropping either loses reports:
 *   - Chromium implements `report-to` and IGNORES `report-uri` whenever a
 *     resolvable `report-to` group is present.
 *   - Firefox and Safari implement `report-uri` ONLY; `report-to` is inert
 *     there, so removing `report-uri` would blind us to those engines.
 * This is the standard non-lossy migration for the `report-uri` deprecation.
 *
 * Note the two channels deliver DIFFERENT bodies — `report-uri` POSTs a single
 * `application/csp-report` object, the Reporting API POSTs an
 * `application/reports+json` ARRAY. The route handler accepts both.
 *
 * Distinct header name from Cloudflare's legacy `Report-To: {"group":"cf-nel"…}`
 * (which the edge already injects), so the two do not collide.
 */
export const REPORTING_ENDPOINTS_HEADER: { name: string; value: string } = {
  name: "Reporting-Endpoints",
  value: `${CSP_REPORT_GROUP}="${CSP_REPORT_PATH}"`,
};

export interface CspOptions {
  /** Per-request base64 nonce (already URL-safe). */
  nonce: string;
  /** When true emit Content-Security-Policy-Report-Only header instead of enforce. */
  reportOnly?: boolean;
  /**
   * When true, add `'unsafe-eval'` to `script-src`. This is required ONLY by
   * the local `next dev` server: React Fast Refresh / the webpack HMR runtime
   * and Clerk's dev build evaluate code via `eval`/`new Function`, which the
   * production-strict CSP forbids — producing a continuous stream of CSP
   * violation reports (each hammering `/api/csp-report`) that destabilises the
   * dev server and makes E2E timing flaky. The middleware sets this ONLY when
   * `NODE_ENV !== "production"`, so production output is unchanged and stays
   * `eval`-free. NEVER pass this in production. Default (unset) = strict.
   */
  allowUnsafeEval?: boolean;
}

/**
 * Build the directive list. Exported separately so tests can assert each
 * directive without parsing a single concatenated header value.
 *
 * `allowUnsafeEval` is a dev-only escape hatch (see {@link CspOptions}); it is
 * off by default so the strict production output is the value every caller gets
 * unless it explicitly opts in.
 */
export function buildCspDirectives(nonce: string, allowUnsafeEval = false): string[] {
  const evalSrc = allowUnsafeEval ? " 'unsafe-eval'" : "";
  return [
    "default-src 'self'",
    `script-src 'self' 'nonce-${nonce}'${evalSrc} https://clerk.corelink-app.humangr.com https://challenges.cloudflare.com https://plausible.io https://js.stripe.com`,
    // style-src uses 'unsafe-inline' (NO nonce) — required by Tailwind's inline
    // <style> + Clerk's runtime-injected widget styles. CRITICAL: a nonce MUST
    // NOT appear here. Per CSP3, when a nonce (or hash) is present the browser
    // IGNORES 'unsafe-inline' — which blocked every inline style Clerk injects,
    // rendering the sign-in/up widget completely UNSTYLED in prod (the Clerk
    // widget can't carry our per-request nonce). 'unsafe-inline' on style-src is
    // accepted per OWASP (styles are not a script-execution XSS vector); the
    // nonce stays on script-src where it actually hardens XSS. NEVER add a nonce
    // or 'unsafe-inline' to script-src. Source: https://clerk.com/docs/security/content-security-policy
    "style-src 'self' 'unsafe-inline'",
    // worker-src: Clerk's Smart CAPTCHA (Cloudflare Turnstile) and Clerk itself
    // spawn a Web Worker from a `blob:` URL. Without an explicit worker-src the
    // browser falls back to default-src ('self'), which forbids blob: → the
    // worker is refused under enforce-mode CSP and bot-protection breaks.
    // Source: https://clerk.com/docs/security/content-security-policy
    "worker-src 'self' blob:",
    "img-src 'self' data: https:",
    "font-src 'self' data:",
    // connect-src additions vs Phase 0 baseline:
    //   https://checkout.stripe.com — required for Stripe Checkout session fetch
    //   https://billing.stripe.com  — required for Stripe Customer Portal redirect
    // Source: https://docs.stripe.com/security/guide#content-security-policy
    // corelink-analytics.humangr.com — first-party PLG event sink
    //   (src/lib/analytics.ts default endpoint `/v1/event`). Without it the
    //   enforce-mode CSP refuses the signup/usage beacons (connect-src).
    // clerk-telemetry.com — Clerk SDK telemetry beacon (default on); without it
    //   enforce-mode CSP throws a violation on every widget mount (noise, not a
    //   functional break). Source: https://clerk.com/docs/security/content-security-policy
    "connect-src 'self' https://corelink-api.humangr.com https://corelink-analytics.humangr.com https://clerk.corelink-app.humangr.com https://clerk-telemetry.com https://plausible.io https://api.stripe.com https://m.stripe.network https://checkout.stripe.com https://billing.stripe.com",
    // frame-src addition: https://clerk.corelink-app.humangr.com for Clerk modal/popup auth steps
    // Source: https://clerk.com/docs/security/content-security-policy
    "frame-src https://js.stripe.com https://hooks.stripe.com https://challenges.cloudflare.com https://clerk.corelink-app.humangr.com",
    "frame-ancestors 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "object-src 'none'",
    // Both reporting channels, basePath-correct. See CSP_REPORT_PATH for the
    // live proof that the bare `/api/csp-report` literal that used to sit here
    // discarded 100% of reports, and REPORTING_ENDPOINTS_HEADER for why the
    // deprecated `report-uri` stays next to `report-to`.
    `report-uri ${CSP_REPORT_PATH}`,
    `report-to ${CSP_REPORT_GROUP}`,
  ];
}

/** Serialize directives into a single header value. */
export function buildCspHeaderValue(nonce: string, allowUnsafeEval = false): string {
  return buildCspDirectives(nonce, allowUnsafeEval).join("; ");
}

/** Header name + value pair, honoring report-only mode for staging. */
export function buildCspHeader(opts: CspOptions): { name: string; value: string } {
  const value = buildCspHeaderValue(opts.nonce, opts.allowUnsafeEval ?? false);
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
  // Names the `report-to csp-endpoint` group the CSP references. Nonce-free, so
  // it belongs with the static set and rides on every response that carries a
  // CSP (middleware) as well as the static-asset paths middleware skips.
  REPORTING_ENDPOINTS_HEADER,
];

/**
 * The paths the root middleware's `config.matcher` (`src/middleware.ts`)
 * EXCLUDES — kept here so the `headers()` sources below can be its exact
 * complement, and so a unit test can prove the two stay complementary.
 *
 * This list MUST stay byte-identical to the negative-lookahead alternation in
 * that matcher. Next statically analyses `config.matcher` at build time, so the
 * matcher itself cannot import this constant; `tests/security-headers-single-
 * emitter.test.ts` re-reads the middleware source and fails if they diverge.
 */
export const MIDDLEWARE_EXCLUDED_ASSET_PATHS: readonly string[] = [
  "_next/static",
  "_next/image",
  "favicon.ico",
  "robots.txt",
  "sitemap.xml",
];

/**
 * `source` patterns for `next.config.ts`'s `headers()` — EXACTLY the paths
 * {@link MIDDLEWARE_EXCLUDED_ASSET_PATHS} names, i.e. the exact complement of
 * the middleware matcher. Next prefixes each with the `basePath`.
 *
 * WHY A COMPLEMENT AND NOT `"/:path*"` (fix/reporting-endpoints-emitted-twice):
 * `STATIC_SECURITY_HEADERS` has two emitters — this `headers()` route set and
 * the `res.headers.set()` loop in `src/middleware.ts`. `set()` replaces, so
 * middleware alone can never double a header; but under `@opennextjs/cloudflare`
 * the two emitters are merged as PLAIN OBJECT KEYS before the Worker builds its
 * `Headers`, and they disagree on CASE — `getNextConfigHeaders` copies our
 * `h.key` verbatim (`Reporting-Endpoints`) while the middleware `Response`
 * lowercases (`reporting-endpoints`). Two object keys survive the merge, and
 * `new Headers({...})` APPENDS each entry, so the same value lands twice on one
 * folded line. Observed live on `/corelink/sign-in` after #981:
 *   `reporting-endpoints: csp-endpoint="/corelink/api/csp-report", csp-endpoint="…"`
 * and identically on `X-Frame-Options`, `Referrer-Policy` and
 * `Permissions-Policy` (the other two static headers are masked because the
 * Cloudflare zone's `security_header` setting overwrites HSTS + nosniff at the
 * edge). RFC 8941 dictionaries are last-wins so nothing broke — but the day the
 * two sources disagree, one silently wins.
 *
 * WHY NEITHER EMITTER COULD SIMPLY BE DELETED:
 *   - Delete this one → `/_next/static/*`, `favicon.ico`, `robots.txt` and
 *     `sitemap.xml` lose every security header: the middleware matcher skips
 *     them by design (they must not pay for a middleware invocation).
 *   - Delete the middleware loop → every middleware SHORT-CIRCUIT loses them.
 *     OpenNext's `routingHandler` computes the `headers()` set BEFORE running
 *     middleware and then `return`s early when middleware produces a result,
 *     discarding it — which is why the live 307 from `/corelink/dashboard` to
 *     `/corelink/sign-in` carries exactly one copy of each header today.
 * So the two are partitioned by PATH instead: every response now has exactly
 * one emitter.
 */
export const SECURITY_HEADER_ROUTE_SOURCES: readonly string[] = [
  "/_next/static/:path*",
  "/_next/image",
  "/favicon.ico",
  "/robots.txt",
  "/sitemap.xml",
];

/**
 * The `headers()` return value for `next.config.ts`. Lives here (rather than
 * inline in the config) so the single-emitter partition is unit-testable
 * without importing the Sentry/next-intl-wrapped config.
 */
export function securityHeaderRoutes(): Array<{
  source: string;
  headers: Array<{ key: string; value: string }>;
}> {
  return SECURITY_HEADER_ROUTE_SOURCES.map((source) => ({
    source,
    headers: STATIC_SECURITY_HEADERS.map((h) => ({ key: h.name, value: h.value })),
  }));
}

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
