// Canonical public URLs for the CoreLink marketing surface.
//
// VERIFIED against the live site on 2026-08-30 — do not "tidy" these into a
// single basePath without re-checking, because the surface is genuinely split:
//
//   https://humangr.com/corelink        200   the CoreLink section root
//   https://humangr.com/pricing         200   the pricing page
//   https://humangr.com/pricing.txt     200
//   https://humangr.com/llms.txt        200
//   https://humangr.com/corelink/pricing      404
//   https://humangr.com/corelink/pricing.txt  404
//   https://humangr.com/corelink/llms.txt     404
//
// The section root lives UNDER `/corelink`; the pricing page and the two text
// endpoints live at the APEX ROOT. Prefixing the latter with the basePath is
// exactly the mistake this module exists to prevent — it produces a 307 into a
// 404 on the buyer funnel, which no build or type check can catch.

export const CORELINK_PUBLIC_ORIGIN = "https://humangr.com";

/** The CoreLink section root. This one IS under a basePath. */
export const CORELINK_BASE_PATH = "/corelink";
export const CORELINK_PUBLIC_BASE_URL = `${CORELINK_PUBLIC_ORIGIN}${CORELINK_BASE_PATH}`;

/** A path inside the CoreLink section (`/corelink/...`). */
export function corelinkPath(path = "/"): string {
  if (path === "" || path === "/") return CORELINK_BASE_PATH;
  return `${CORELINK_BASE_PATH}${path.startsWith("/") ? path : `/${path}`}`;
}

/** An absolute URL inside the CoreLink section. */
export function corelinkUrl(path = "/"): string {
  return `${CORELINK_PUBLIC_ORIGIN}${corelinkPath(path)}`;
}

/**
 * An absolute URL for a page served at the APEX ROOT, with no basePath.
 * Used by the public redirects below — see the verified table above.
 */
export function apexUrl(path: string): string {
  return `${CORELINK_PUBLIC_ORIGIN}${path.startsWith("/") ? path : `/${path}`}`;
}

/** The three public destinations the admin-ui redirects to. */
export const PUBLIC_PRICING_URL = apexUrl("/pricing");
export const PUBLIC_PRICING_TXT_URL = apexUrl("/pricing.txt");
export const PUBLIC_LLMS_TXT_URL = apexUrl("/llms.txt");
