/**
 * Resolve the internal-auth key the signup-worker must present on the DSR /
 * CAS-erase internal calls (`POST /_internal/dsr/{erase,verify}`).
 *
 * # Why this exists (rt-nuclear #23)
 *
 * The erase/DSR internal surface is gated by a PER-CONSUMER key split (red-team
 * #3): the dedicated `CORELINK_ERASE_AUTH_KEY`, falling back to the shared
 * `CORELINK_INTERNAL_AUTH_KEY` when the dedicated one is unset. Both peers that
 * sit on the erase path already resolve the key this way — erase-first,
 * shared-fallback:
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
 * This resolver mirrors the peers' resolution so the key always matches: prefer
 * `CORELINK_ERASE_AUTH_KEY`, fall back to the shared `CORELINK_INTERNAL_AUTH_KEY`.
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
