import type { Env } from "../index.js";
import { normalizePatSigningKeyEnv } from "./pat_signing_key.js";

/**
 * Build the optional PAT rotation siblings for every container start.
 *
 * Cloudflare's Durable Object container binding is used for both ordinary
 * tenant instances and the shared `_oci` instance, so one helper keeps the
 * forwarding/default contract identical across both classes. Empty is the
 * intentional unset sentinel consumed by the Rust PAT decoder; whitespace is
 * converted to a deterministic malformed marker so rotation config fails
 * closed instead of becoming an absent sibling inside the container.
 */
export function patRotationEnv(env: Env): {
  PAT_SIGNING_KEY: string;
  PAT_SIGNING_KEY_PREV: string;
  PAT_SIGNING_KEY_NEW: string;
} {
  return {
    PAT_SIGNING_KEY: normalizePatSigningKeyEnv(env.PAT_SIGNING_KEY),
    PAT_SIGNING_KEY_PREV: normalizePatSigningKeyEnv(env.PAT_SIGNING_KEY_PREV),
    PAT_SIGNING_KEY_NEW: normalizePatSigningKeyEnv(env.PAT_SIGNING_KEY_NEW),
  };
}
