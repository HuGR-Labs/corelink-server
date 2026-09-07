/** Authentication, request policy, and timing-pad domain for the edge worker. */

export type { AuthResult } from "./index_auth_policy.js";
export {
  resolveOnboardingAuthKey,
  DO_METER_LEASE_BLOCK,
  resolveRequestId,
  applyCors,
  handlePreflight,
  MFA_FVA_FRESH_MAX_MINUTES,
  stripClientTrustHeaders,
  extractBasicAuthPassword,
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
