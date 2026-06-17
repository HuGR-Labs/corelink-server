/**
 * DSR erasure queue consumer (WI-S11-008 WP-G).
 *
 * Cloudflare Queue consumer for `dsr.queued.v1` messages produced by the Clerk
 * `user.deleted` webhook (see `clerk.ts::handleUserDeleted`). Each message is
 * forwarded to the container's internal erasure endpoint
 * (`POST /_internal/dsr/erase`, gated by `CORELINK_INTERNAL_AUTH_KEY`), which
 * runs the 12-backend erasure orchestrator. This mirrors the existing
 * container-internal call pattern used for `/_internal/pat/mint`.
 *
 * Reliability: a non-2xx (or transport error, or missing internal-auth) →
 * `message.retry()` so Cloudflare redelivers; the erasure orchestrator is
 * idempotent (dedup per `(dsr_id, backend)`), so a redelivered message never
 * double-erases. A 2xx → `message.ack()`.
 */

import type { DsrQueuedV1 } from "./clerk.js";
import { resolveEraseAuthKey } from "../lib/erase-auth-key.js";

/** Minimal env surface the consumer needs (kept independent of the full Worker Env). */
export interface DsrConsumerEnv {
  CORELINK_API_BASE: string;
  /**
   * Dedicated erase/DSR consumer secret (red-team #3 per-consumer split).
   * Preferred for the `x-corelink-internal-auth` header on the erase call so
   * it matches the main-Worker / container erase gate in the full-split config
   * (rt-nuclear #23); falls back to the shared key below.
   */
  CORELINK_ERASE_AUTH_KEY?: string;
  /** Shared secret for the `x-corelink-internal-auth` header on the container call. */
  CORELINK_INTERNAL_AUTH_KEY?: string;
  /** Service binding to the main CoreLink Worker (bypasses CF edge error 1014). */
  CORELINK_API_SVC?: { fetch: typeof fetch };
}

/** Minimal Cloudflare Queue message surface (test-independent of workers-types). */
export interface QueueMessage<T> {
  readonly body: T;
  ack(): void;
  retry(): void;
}
export interface QueueMessageBatch<T> {
  readonly messages: ReadonlyArray<QueueMessage<T>>;
}

/**
 * Forward ONE erasure request to the container's internal erasure endpoint.
 * Returns `{ ok }` — `ok=false` signals the batch handler to retry. NEVER
 * throws on a transport error (caught and reported as `ok:false`).
 */
export async function processErasureMessage(
  msg: DsrQueuedV1,
  env: DsrConsumerEnv,
): Promise<{ ok: boolean; status: number }> {
  // rt-nuclear #23: send the ERASE consumer key (shared-key fallback) so the
  // header matches the main-Worker / container erase gate in the full-split
  // config. Resolution mirrors `erase_auth_key_from_env()` + the main Worker's
  // `resolveConsumerKey(env, "erase")`.
  const eraseAuthKey = resolveEraseAuthKey(env);
  if (!eraseAuthKey) {
    // Cannot authenticate to the container — retry rather than DROP a GDPR
    // erasure obligation (self-heals once the secret is bound).
    console.error(
      `[dsr-consumer] no erase/internal auth key bound — retrying erasure dsr_id=${msg.dsr_id}`,
    );
    return { ok: false, status: 0 };
  }
  const req = new Request(`${env.CORELINK_API_BASE}/_internal/dsr/erase`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-corelink-internal-auth": eraseAuthKey,
    },
    body: JSON.stringify(msg),
  });
  try {
    const resp = env.CORELINK_API_SVC
      ? await env.CORELINK_API_SVC.fetch(req)
      : await fetch(req);
    return { ok: resp.ok, status: resp.status };
  } catch (err) {
    console.error(
      `[dsr-consumer] erasure call threw dsr_id=${msg.dsr_id}: ${(err as Error).message.slice(0, 120)}`,
    );
    return { ok: false, status: 0 };
  }
}

/**
 * Process a batch of `dsr.queued.v1` messages. Each is acked on a 2xx from the
 * container and retried otherwise. Per-message isolation: one failure never
 * blocks the rest of the batch.
 */
export async function handleErasureQueueBatch(
  batch: QueueMessageBatch<DsrQueuedV1>,
  env: DsrConsumerEnv,
): Promise<void> {
  for (const m of batch.messages) {
    const r = await processErasureMessage(m.body, env);
    if (r.ok) {
      m.ack();
    } else {
      m.retry();
    }
  }
}
