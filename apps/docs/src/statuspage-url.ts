/**
 * statuspage-url — DEBT-016 closure (R-prep wave-24).
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
 * The fallback default is `https://status.corelink.humangr.com` (the canonical
 * wave-19 commit value referenced from 5 trust MDX pages × 4 locales and
 * 20+ internal runbooks).
 *
 * NOTE: This module must be importable from both Node (config-time) and
 * the browser (component-time). Do not introduce runtime-only deps.
 */

export const DEFAULT_STATUSPAGE_URL = "https://status.corelink.humangr.com";

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
