/** Canonical PAT expiry semantics shared by every Worker auth path. */

/**
 * Return true when a PAT is live at `nowMs`.
 *
 * `expires_ms = 0` is the persisted never-expiring sentinel. Positive values
 * are absolute Unix milliseconds and are strictly greater than `nowMs`; a
 * zero/positive comparison is intentionally not duplicated at call sites so
 * native Worker auth cannot drift from the container SQL predicate.
 */
export function isPatExpiryLive(expiresMs: number, nowMs: number = Date.now()): boolean {
  return expiresMs === 0 || expiresMs > nowMs;
}
