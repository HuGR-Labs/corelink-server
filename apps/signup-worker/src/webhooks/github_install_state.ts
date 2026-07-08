/**
 * Signed `state` for the GitHub App install flow (CSRF + identity binding).
 *
 * The raw GitHub `installation:created` webhook carries `installation.id` + repos
 * but NO CoreLink identity — binding a tenant off it alone is the lazy-provision
 * DP3 forbids. The identity must come from the install FLOW: a Clerk-authed
 * tenant clicks "Install" → CoreLink mints a **signed state = tenant_id** →
 * GitHub install → redirect to the setup callback with `installation_id` +
 * `state` → the callback VERIFIES the signature → THAT is the authenticated
 * `tenant_id` for the provisioning primitive.
 *
 * Format (URL-safe, no dots inside the fields — `tenant_id` is a UUID, `exp` is
 * decimal ms):  `<tenant_id>.<exp_ms>.<hex HMAC-SHA256(key, "<tenant_id>.<exp_ms>")>`
 *
 * The HMAC is over the tenant_id AND the expiry, so neither can be tampered
 * without invalidating the signature; the expiry bounds replay. Verification is
 * constant-time. Shared by the admin-ui "Install" mint and the signup-worker
 * callback — both compute the identical HMAC with `INSTALL_STATE_SIGNING_KEY`.
 */

/** Default state lifetime: 10 minutes (an install redirect is near-immediate). */
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

/** Constant-time hex-string compare (folds length into the accumulator). */
function ctEqualHex(a: string, b: string): boolean {
  let diff = a.length ^ b.length;
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i++) {
    diff |= (a.charCodeAt(i) || 0) ^ (b.charCodeAt(i) || 0);
  }
  return diff === 0;
}

/**
 * Mint a signed state for `tenantId`, valid for `ttlMs` from `nowMs`.
 * (Used by the admin-ui "Install GitHub App" mint endpoint.)
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

/**
 * Verify a state string. Returns the bound `tenant_id` on success, or `null`
 * on any failure (malformed, bad signature, or expired) — fail-closed, no
 * distinction leaked to the caller beyond null.
 */
export async function verifyInstallState(
  state: string,
  key: string,
  nowMs: number,
): Promise<{ tenantId: string } | null> {
  const parts = state.split(".");
  if (parts.length !== 3) return null;
  const [tenantId, expStr, sig] = parts;
  if (!tenantId || !expStr || !sig) return null;

  const exp = Number(expStr);
  if (!Number.isFinite(exp)) return null;

  // Recompute + constant-time compare BEFORE the expiry check so a bad-sig and
  // an expired-but-valid-sig take the same path shape.
  const expected = await hmacHex(key, `${tenantId}.${expStr}`);
  if (!ctEqualHex(sig, expected)) return null;

  if (nowMs > exp) return null; // expired
  return { tenantId };
}
