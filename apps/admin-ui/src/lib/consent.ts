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
 */
export async function postConsent(
  state: ConsentState,
  fetchImpl: typeof fetch = fetch
): Promise<{ accepted_at: string }> {
  const response = await fetchImpl("/v1/consent", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(state),
  });
  if (!response.ok) {
    throw new Error(`Consent POST failed: ${response.status}`);
  }
  return (await response.json()) as { accepted_at: string };
}
