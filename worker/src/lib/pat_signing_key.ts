/**
 * Shared validity predicate for PAT signing keys (WP-F2 / MED-1 closure).
 *
 * A valid key is an EVEN-length, all-hex string of >= 64 chars (>= 32 decoded
 * bytes) — the same contract the container's `adapter_pat::from_env` enforces
 * (`hex::decode` + byte floor).
 *
 * WHY THIS EXISTS AS A SHARED HELPER: the rotation siblings
 * (`PAT_SIGNING_KEY_PREV` / `_NEW`) were validated with this full predicate,
 * but the PRIMARY `PAT_SIGNING_KEY` only by raw length — so a 64+-char
 * non-hex main key sailed through, made `hexDecode()` return null on every
 * request, failed HMAC silently and returned 401 to ALL legitimate clients
 * with no CRITICAL log. One predicate, two call sites: they cannot diverge
 * again.
 */
export function isValidPatSigningKeyHex(key: string): boolean {
  return key.length >= 64 && key.length % 2 === 0 && /^[0-9a-fA-F]+$/.test(key);
}
