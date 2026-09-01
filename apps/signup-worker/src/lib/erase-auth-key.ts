/**
 * Resolve the internal-auth key the signup-worker must present on the DSR /
 * CAS-erase internal calls (`POST /_internal/dsr/{erase,verify}`).
 *
 * # Why this exists (rt-nuclear #23)
 *
 * The erase/DSR internal surface is gated by a PER-CONSUMER key split (red-team
 * #3). Older, additive callers still use the legacy erase-first/shared-fallback
 * resolver below. Irreversible audit-drain calls use the separate strict resolver
 * because the container's `/_internal/audit/drain` mount requires the dedicated
 * key with no shared fallback.
 *
 *   - the container's `erase_auth_key_from_env()` (server PR #317), and
 *   - the main Worker's `/_internal/dsr/*` verify gate
 *     (`resolveConsumerKey(env, "erase")` in `worker/src/index.ts`).
 *
 * The signup-worker is the LIVE driver of the DSR path (Clerk `user.deleted` →
 * queue → `/_internal/dsr/erase`, plus the verify cron). Its calls route THROUGH
 * the main Worker (the `CORELINK_API_SVC` service binding to `corelink-prod`),
 * whose erase gate now expects `CORELINK_ERASE_AUTH_KEY` whenever that dedicated
 * secret is provisioned (the "full-split" config). If the signup-worker kept
 * sending only the shared key, the main Worker's gate would 401 the erase call
 * the moment the dedicated erase key is deployed — silently breaking the GDPR
 * erasure path in exactly the full-split config #23 warns about.
 *
 * `resolveEraseAuthKey` mirrors the legacy peers: prefer
 * `CORELINK_ERASE_AUTH_KEY`, fall back to the shared
 * `CORELINK_INTERNAL_AUTH_KEY`. `resolveDedicatedEraseAuthKey` is deliberately
 * stricter and is the only resolver audit-drain may use.
 *
 * The ≥32-char floor is enforced AUTHORITATIVELY at the two verify gates (main
 * Worker + container); this resolver intentionally only selects WHICH key to
 * send (empty/unset ⇒ absent), so a too-short key still degrades to the same
 * fail-closed 401/403 there rather than being silently swapped.
 */

/** Env surface carrying the erase-path internal-auth secrets. */
export interface EraseAuthKeyEnv {
  /** Dedicated erase/DSR consumer secret (red-team #3 split). Preferred. */
  CORELINK_ERASE_AUTH_KEY?: string;
  /** Shared internal-auth secret. Fallback when the dedicated key is unset. */
  CORELINK_INTERNAL_AUTH_KEY?: string;
}

/** The key floor the container enforces before it mounts irreversible routes. */
export const MIN_ERASE_AUTH_KEY_LENGTH = 32;

/**
 * Return a dedicated erase key only when it is non-blank and meets the
 * container's minimum length. The shared internal key is intentionally ignored:
 * possession of it must never authorize an audit-chain drain.
 */
export function resolveDedicatedEraseAuthKey(env: EraseAuthKeyEnv): string | null {
  const dedicated = env.CORELINK_ERASE_AUTH_KEY;
  if (!dedicated || dedicated.trim().length < MIN_ERASE_AUTH_KEY_LENGTH) {
    return null;
  }
  return dedicated;
}

/**
 * Return the key to send on the erase/DSR internal call: the dedicated erase
 * key when set (non-empty), else the shared key, else `null` (no usable key —
 * the caller must NOT make the call and should retry/skip, never drop the
 * obligation). Mirrors the container + main-Worker erase-first, shared-fallback
 * resolution.
 */
export function resolveEraseAuthKey(env: EraseAuthKeyEnv): string | null {
  const dedicated = env.CORELINK_ERASE_AUTH_KEY;
  if (dedicated && dedicated.length > 0) {
    return dedicated;
  }
  const shared = env.CORELINK_INTERNAL_AUTH_KEY;
  if (shared && shared.length > 0) {
    return shared;
  }
  return null;
}
