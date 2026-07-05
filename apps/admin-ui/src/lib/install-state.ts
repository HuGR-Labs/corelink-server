/**
 * Signed GitHub-App install `state` — admin-ui MINT side.
 *
 * ## Contract mirror (load-bearing)
 *
 * This is a byte-for-byte MIRROR of the mint half of
 * `apps/signup-worker/src/webhooks/github_install_state.ts`. The two live in
 * SEPARATE deployables (admin-ui = Cloudflare Pages edge, signup-worker =
 * Worker) so they cannot import a shared module — but they MUST compute an
 * identical HMAC-SHA256 over the identical `${tenantId}.${exp}` preimage with
 * the SAME `INSTALL_STATE_SIGNING_KEY`, or the signup-worker callback's
 * `verifyInstallState` rejects every state this mints.
 *
 * Any edit here MUST be mirrored there. The cross-deployable agreement is
 * pinned by `apps/admin-ui/src/lib/install-state.test.ts` (which reimplements
 * the worker's verify and round-trips against it).
 *
 * Format (URL-safe; `tenant_id` is a UUID, `exp` is decimal ms):
 *   `<tenant_id>.<exp_ms>.<hex HMAC-SHA256(key, "<tenant_id>.<exp_ms>")>`
 *
 * The HMAC covers BOTH the tenant_id and the expiry, so neither can be tampered
 * without invalidating the signature; the expiry bounds replay. Runs on Web
 * Crypto (`crypto.subtle`) — available in the edge runtime, no Node Buffer.
 */

/** State lifetime: 10 minutes (an install redirect is near-immediate). MUST match the worker. */
export const INSTALL_STATE_TTL_MS = 10 * 60 * 1000;

async function hmacHex(key: string, message: string): Promise<string> {
  const enc = new TextEncoder();
  const cryptoKey = await crypto.subtle.importKey(
    "raw",
    enc.encode(key),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const sig = new Uint8Array(await crypto.subtle.sign("HMAC", cryptoKey, enc.encode(message)));
  return Array.from(sig)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/**
 * Mint a signed install state for `tenantId`, valid for `ttlMs` from `nowMs`.
 * The returned string is what the "Install GitHub App" flow hands to GitHub as
 * `?state=`; the signup-worker callback verifies it back to `tenantId`.
 */
export async function signInstallState(
  tenantId: string,
  key: string,
  nowMs: number,
  ttlMs: number = INSTALL_STATE_TTL_MS,
): Promise<string> {
  const exp = nowMs + ttlMs;
  const payload = `${tenantId}.${exp}`;
  const sig = await hmacHex(key, payload);
  return `${payload}.${sig}`;
}
