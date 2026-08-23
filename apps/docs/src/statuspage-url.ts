/**
 * statuspage-url — DEBT-016 closure (R-prep wave-24), repointed post
 * false-"operational" incident (2026-08-22, see StatusPill.tsx doc-comment).
 *
 * Single source of truth for the customer-facing statuspage URL, exposed to
 * Docusaurus build-time (`docusaurus.config.ts customFields.statuspageUrl`)
 * and to client-side React components via `useDocusaurusContext()`.
 *
 * Operator-bound provisioning paths (see `specs/_runbooks/STATUSPAGE-INIT.md`):
 *
 *   - Option A (preferred, zero rebuild): Operator CNAMEs the canonical
 *     `status.corelink.humangr.com` to the real Atlassian Statuspage tenant. All
 *     literal MDX URLs resolve correctly without docs rebuild.
 *
 *   - Option B (env-var override, rebuild required): Operator sets
 *     `STATUSPAGE_URL=https://status.example.com` before `pnpm build`. New
 *     components / future MDX consumers can use the helpers below to read
 *     the operator-provided value; existing literal URLs in trust pages
 *     remain canonical defaults.
 *
 * The fallback default is `https://hugrl.betteruptime.com` — the live
 * BetterStack status page (verified 2026-08-22: `/index.json` returns HTTP
 * 200 with real JSON). The previous default, `https://status.corelink.humangr.com`,
 * is a third-level name outside Cloudflare Universal SSL's one-level
 * `*.humangr.com` coverage: the CNAME was never registered at BetterStack and
 * the TLS handshake fails outright (`curl` returns connect failure). Nothing
 * ever surfaced this because the StatusPill's own fetch-failure path rendered
 * a green "All systems operational" pill regardless — see
 * `src/components/StatusPill/StatusPill.tsx`. Do not restore
 * `status.corelink.humangr.com` as the default until Option A above is
 * actually completed and re-verified live.
 *
 * NOTE: This module must be importable from both Node (config-time) and
 * the browser (component-time). Do not introduce runtime-only deps.
 */

export const DEFAULT_STATUSPAGE_URL = "https://hugrl.betteruptime.com";

/**
 * Resolve the operator-configured statuspage URL.
 *
 * On the server side this consults `process.env.STATUSPAGE_URL`; on the
 * client side it reads the value injected at build time via
 * `customFields.statuspageUrl`.
 *
 * @param customFieldsValue - the `siteConfig.customFields.statuspageUrl`
 *   value (pass from a React component using `useDocusaurusContext()`).
 *   Optional — when omitted (Node config-time path), reads from env.
 */
export function getStatuspageUrl(customFieldsValue?: string | null): string {
  if (typeof customFieldsValue === "string" && customFieldsValue.length > 0) {
    return customFieldsValue;
  }
  // Node config-time fallback (docusaurus.config.ts loads this module).
  if (
    typeof process !== "undefined" &&
    typeof process.env === "object" &&
    typeof process.env.STATUSPAGE_URL === "string" &&
    process.env.STATUSPAGE_URL.length > 0
  ) {
    return process.env.STATUSPAGE_URL;
  }
  return DEFAULT_STATUSPAGE_URL;
}
