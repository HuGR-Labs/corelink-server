import { resolveApiBaseUrl } from "@/lib/api-base-url";

export type ConsentCategory = "functional" | "analytics" | "marketing";

export interface ConsentState {
  functional: true; // always-on
  analytics: boolean;
  marketing: boolean;
}

export const DEFAULT_CONSENT: ConsentState = {
  functional: true,
  analytics: false,
  marketing: false,
};

/**
 * POST consent grants to the server. Aligned with CTRL-PRIV-CONSENT-001.
 * Returns the parsed response (or throws on non-2xx).
 *
 * `/v1/...` is a path on the SEPARATE `corelink-api` origin, not a route in
 * this Next app, so it must be prefixed with the resolved API base. A bare
 * `/v1/consent` from the browser resolved against the apex `humangr.com` —
 * the hugr-site MARKETING app — which answers `200 text/html`; the fetch then
 * succeeded and only the JSON parse failed, so the cookie banner reported a
 * generic save error instead of a routing fault.
 *
 * NOTE: like the rest of the consent surface, `/v1/consent` has NO handler
 * anywhere in the repo (see the header of `lib/consent-api.ts`). This makes
 * the request correctly addressed, not answered.
 */
export async function postConsent(
  state: ConsentState,
  fetchImpl: typeof fetch = fetch
): Promise<{ accepted_at: string }> {
  const response = await fetchImpl(`${resolveApiBaseUrl()}/v1/consent`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(state),
  });
  if (!response.ok) {
    throw new Error(`Consent POST failed: ${response.status}`);
  }
  return (await response.json()) as { accepted_at: string };
}
