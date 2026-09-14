/** Authentication, request policy, and timing-pad domain for the edge worker. */

/** Decode a pip/uv Basic credential and return only its password (the PAT). */
export function extractBasicAuthPassword(b64: string): string | null {
  let decoded: string;
  try {
    decoded = atob(b64);
  } catch {
    return null;
  }
  const colon = decoded.indexOf(":");
  if (colon < 0) return null;
  const password = decoded.slice(colon + 1);
  return password.length === 0 ? null : password;
}

/** Stable auth-stage seam used by the verifier. */
export function extractBasicAuthForAuthStage(b64: string): string | null {
  return extractBasicAuthPassword(b64);
}

export type { AuthResult } from "./index_auth_policy.js";
export {
  resolveOnboardingAuthKey,
  DO_METER_LEASE_BLOCK,
  resolveRequestId,
  applyCors,
  handlePreflight,
  MFA_FVA_FRESH_MAX_MINUTES,
  stripClientTrustHeaders,
} from "./index_auth_policy.js";
export { extractAuth } from "./index_auth_verify.js";
export {
  parsePat,
  verifyPatHmacMulti,
  verifyPatHmac,
  hexDecode,
  base64url,
} from "./index_auth_pat.js";
export { applyTimingPad } from "./index_auth_timing.js";
