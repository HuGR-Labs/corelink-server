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
 * Reliability (M2 — poison-message classification): the container response is
 * CLASSIFIED, not blanket-retried. A 2xx → `ack()`. A *permanent* 4xx
 * (malformed body 400/409, or erasure REJECTED 422) can NEVER succeed on
 * redelivery → `ack()` it as a poison message (with a loud structured
 * `console.error`) rather than burning the full 10-retry budget then silently
 * dead-lettering. Only genuinely transient faults — 5xx, transport/network
 * errors, and self-healing misconfig (401/403/404 auth-key/route, 408/429
 * backpressure) — `retry()`, because dropping those would abandon a GDPR
 * Art.17 erasure obligation. The orchestrator is idempotent (dedup per
 * `(dsr_id, backend)`), so a redelivered message never double-erases.
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
 * Returns `{ ok, status }` — the batch handler classifies `status` via
 * `classifyErasureStatus` into ack-success / ack-poison / retry. NEVER throws
 * on a transport error (caught and reported as `{ ok:false, status:0 }`, which
 * classifies as `retry`).
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

/** Queue disposition for one erasure attempt (M2). */
export type ErasureDisposition = "ack-success" | "ack-poison" | "retry";

/**
 * Classify a container erase HTTP status into a queue disposition (M2).
 *
 * - `2xx` → `ack-success`.
 * - PERMANENT 4xx — malformed/duplicate body (`400`/`409`) or erasure REJECTED
 *   by the legitimacy gate (`422`) — → `ack-poison`: redelivery can never
 *   change the outcome, so we consume it (with a loud log) instead of burning
 *   the 10-retry budget then silently dead-lettering.
 * - Self-healing misconfig (`401`/`403` auth-key, `404` route-not-mounted) and
 *   backpressure (`408`/`429`) → `retry`: these clear once the secret binds /
 *   route mounts / limit resets; dropping them would abandon a GDPR Art.17
 *   obligation.
 * - `5xx`, transport/network faults (`status === 0`), and any unknown status →
 *   `retry` (genuinely transient; fail toward NOT dropping the obligation).
 */
export function classifyErasureStatus(status: number): ErasureDisposition {
  if (status >= 200 && status < 300) return "ack-success";
  if (status === 0) return "retry"; // transport / network fault
  if (status >= 500) return "retry"; // transient server fault
  if (status === 401 || status === 403 || status === 404 || status === 408 || status === 429) {
    return "retry"; // auth/route misconfig or backpressure — self-heals
  }
  if (status >= 400 && status < 500) return "ack-poison"; // permanent reject/malformed
  return "retry"; // unknown — fail toward not dropping the erasure obligation
}

/**
 * Process a batch of `dsr.queued.v1` messages. Each is classified
 * (`classifyErasureStatus`): acked on success, acked-as-poison (loud log) on a
 * permanent 4xx, retried on a transient fault. Per-message isolation: one
 * failure never blocks the rest of the batch.
 */
export async function handleErasureQueueBatch(
  batch: QueueMessageBatch<DsrQueuedV1>,
  env: DsrConsumerEnv,
): Promise<void> {
  for (const m of batch.messages) {
    const r = await processErasureMessage(m.body, env);
    const disposition = classifyErasureStatus(r.status);
    if (disposition === "retry") {
      m.retry();
      continue;
    }
    if (disposition === "ack-poison") {
      // PERMANENT failure — consume (don't retry) but make it loudly visible.
      console.error(
        JSON.stringify({
          level: "error",
          severity: "high",
          event: "dsr.erasure.poison",
          component: "dsr-consumer",
          status: r.status,
          dsr_id: m.body.dsr_id,
          tenant_id: m.body.tenant_id,
          action: "ack_poison",
          note: "permanent 4xx erasure failure — acked WITHOUT retry; manual review of this GDPR Art.17 request required",
        }),
      );
    }
    m.ack();
  }
}

// ── M3: dead-letter-queue handler (corelink-dsr-erasure-dlq) ──────────────────

/**
 * Body of a re-enqueued erasure: the original `dsr.queued.v1` plus a bounded
 * re-enqueue counter. The container's `DsrQueuedV1` deserializer accepts and
 * ignores unknown fields, so this extra marker is safe on the erase call.
 */
export type DsrDlqBody = DsrQueuedV1 & { _dlq_requeue?: number };

const MAX_DLQ_REQUEUES = 1;
const PAGERDUTY_EVENTS_V2_URL = "https://events.pagerduty.com/v2/enqueue";
const DLQ_EVENT_NAME = "dsr.erasure.dead_letter";

/**
 * Queue payloads are external input. Normalize the marker before making a
 * retry decision: NaN, fractions, negatives, and strings must never bypass
 * the one-requeue cap or produce misleading operator counts.
 */
function normalizedDlqRequeueCount(value: unknown): number {
  // An omitted marker is the first DLQ generation. A present-but-malformed
  // marker is treated as exhausted, never as a fresh message: otherwise a
  // forged string/NaN could bypass the one-requeue bound.
  if (value === undefined) return 0;
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    return MAX_DLQ_REQUEUES;
  }
  return Math.min(value, MAX_DLQ_REQUEUES);
}

/**
 * Stable correlation for one DLQ generation. The deterministic value is
 * intentionally derived from the immutable DSR identity and the bounded
 * requeue generation, so redelivery of the same DLQ message keeps one event
 * id while the one allowed requeue gets a distinct id on a later DLQ arrival.
 */
function dlqEventId(body: DsrDlqBody, requeueCount: number): string {
  const dsrId = typeof body.dsr_id === "string" && body.dsr_id.length > 0 ? body.dsr_id : "unknown";
  return `dsr-erasure-dlq:${encodeURIComponent(dsrId)}:${requeueCount}`;
}

/** Env the DLQ handler needs: the MAIN-queue producer binding for re-enqueue. */
export interface DsrDlqEnv {
  /** Producer binding to the MAIN erasure queue (one bounded re-enqueue). */
  DSR_QUEUE?: { send(message: unknown): Promise<void> };
  /** Canonical PagerDuty Events API v2 production routing key. */
  PAGERDUTY_ROUTING_KEY?: string;
  /** Injectable fetch seam for focused tests; production uses the Worker fetch. */
  PAGERDUTY_FETCH?: typeof fetch;
}

type PagingResult =
  | { status: "delivered" }
  | { status: "not_configured" }
  | { status: "failed"; error: string };

/** Deliver one critical DLQ page through the canonical PagerDuty procedure. */
async function pageDlqEvent(
  body: DsrDlqBody,
  eventId: string,
  requeueCount: number,
  env: DsrDlqEnv,
): Promise<PagingResult> {
  const routingKey = env.PAGERDUTY_ROUTING_KEY?.trim();
  if (!routingKey) return { status: "not_configured" };

  const fetcher = env.PAGERDUTY_FETCH ?? fetch;
  try {
    const response = await fetcher(PAGERDUTY_EVENTS_V2_URL, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        routing_key: routingKey,
        event_action: "trigger",
        dedup_key: eventId,
        payload: {
          summary: `DSR erasure dead-letter exhausted (${body.dsr_id})`,
          source: "corelink-signup-worker",
          severity: "critical",
          custom_details: {
            event_id: eventId,
            dsr_id: body.dsr_id,
            tenant_id: body.tenant_id,
            exhausted: true,
            requeue_count: requeueCount,
          },
        },
      }),
    });
    if (!response.ok) {
      return { status: "failed", error: `pagerduty_http_${response.status}` };
    }
    return { status: "delivered" };
  } catch (err) {
    return {
      status: "failed",
      error: `pagerduty_transport_${(err as Error).message.slice(0, 120)}`,
    };
  }
}

function logDlqEvent(fields: Record<string, unknown>): void {
  // Keep this as one JSON object: external log shipping and the owner packet
  // correlate by event_id and must never have to reconstruct a split message.
  const canonicalEvent = { event: "dsr.erasure.dead_letter" };
  if (fields.event === undefined) fields.event = canonicalEvent.event;
  console.error(JSON.stringify(fields));
}

/**
 * Dead-letter handler for `corelink-dsr-erasure-dlq` (M3, GDPR Art.17).
 *
 * A message reaches here only after exhausting the main queue's 10-retry budget
 * on a *sustained transient* fault (container down, erase-auth key never bound)
 * — permanent 4xx failures are now poison-acked upstream and never dead-letter.
 * For EACH dead-lettered erasure we:
 *   1. emit a STRUCTURED, loud ALERT log (severity, dsr_id, tenant, the
 *      failure) so the obligation is visible beyond the hourly verify cron, and
 *   2. give it ONE bounded extra attempt by re-enqueuing onto the main erasure
 *      queue — IFF it has not already been re-enqueued AND the producer binding
 *      is present. The `_dlq_requeue` marker caps this at a single re-enqueue so
 *      a hard-down backend can never create a DLQ⇄main loop. Otherwise the
 *      message is left DEAD (acked) with the loud alert for operator action.
 */
export async function handleErasureDlqBatch(
  batch: QueueMessageBatch<DsrDlqBody>,
  env: DsrDlqEnv,
): Promise<void> {
  for (const m of batch.messages) {
    const body = m.body;
    const priorRequeues = normalizedDlqRequeueCount(body._dlq_requeue);
    const eventId = dlqEventId(body, priorRequeues);
    const canRequeue = priorRequeues < MAX_DLQ_REQUEUES && !!env.DSR_QUEUE;

    // A configured pager is part of the failure path: do not acknowledge or
    // requeue the DLQ copy until PagerDuty accepted the page. Missing config is
    // deliberately visible as a blocker, but preserves the existing bounded
    // requeue/manual-follow-up behavior rather than inventing delivery proof.
    const paging = await pageDlqEvent(body, eventId, priorRequeues, env);
    if (paging.status === "failed") {
      logDlqEvent({
        level: "alert",
        severity: "critical",
        event: DLQ_EVENT_NAME,
        component: "dsr-erasure-dlq",
        event_id: eventId,
        exhausted: true,
        requeue_count: priorRequeues,
        dsr_id: body.dsr_id,
        tenant_id: body.tenant_id,
        queued_at_ms: body.queued_at_ms,
        action: "retry_paging",
        paging_status: paging.status,
        paging_error: paging.error,
        note: "PagerDuty rejected the exhausted DSR alert; retaining the DLQ copy for delivery retry",
      });
      m.retry();
      continue;
    }

    if (!canRequeue) {
      // No safe re-enqueue path: leave it DEAD with the alert above. ack so the
      // DLQ does not spin redelivering the same dead message.
      logDlqEvent({
        level: "alert",
        severity: "critical",
        event: DLQ_EVENT_NAME,
        component: "dsr-erasure-dlq",
        event_id: eventId,
        exhausted: true,
        requeue_count: priorRequeues,
        dsr_id: body.dsr_id,
        tenant_id: body.tenant_id,
        queued_at_ms: body.queued_at_ms,
        action: "left_dead",
        paging_status: paging.status,
        paging_configured: paging.status !== "not_configured",
        note: paging.status === "not_configured"
          ? "PagerDuty routing key is not configured; no external page was claimed — MANUAL operator action required"
          : "GDPR Art.17 erasure dead after bounded re-enqueue — MANUAL operator action required",
      });
      m.ack();
      continue;
    }
    try {
      // env.DSR_QUEUE is non-null here (guarded by canRequeue).
      await env.DSR_QUEUE!.send({ ...body, _dlq_requeue: priorRequeues + 1 });
      logDlqEvent({
        level: "alert",
        severity: "critical",
        event: DLQ_EVENT_NAME,
        component: "dsr-erasure-dlq",
        event_id: eventId,
        exhausted: true,
        requeue_count: priorRequeues + 1,
        dsr_id: body.dsr_id,
        tenant_id: body.tenant_id,
        queued_at_ms: body.queued_at_ms,
        action: "requeue_once",
        paging_status: paging.status,
        paging_configured: paging.status !== "not_configured",
        note: paging.status === "not_configured"
          ? "GDPR Art.17 erasure exhausted main-queue retries — re-enqueued once; PagerDuty routing key is not configured, MANUAL operator follow-up required"
          : "GDPR Art.17 erasure exhausted main-queue retries — re-enqueued for ONE bounded final attempt",
      });
      m.ack(); // handed back to the main queue; consume the DLQ copy
    } catch (err) {
      logDlqEvent({
        level: "alert",
        severity: "critical",
        event: DLQ_EVENT_NAME,
        component: "dsr-erasure-dlq",
        event_id: eventId,
        exhausted: true,
        requeue_count: priorRequeues,
        dsr_id: body.dsr_id,
        tenant_id: body.tenant_id,
        queued_at_ms: body.queued_at_ms,
        action: "retry_requeue",
        paging_status: paging.status,
        paging_configured: paging.status !== "not_configured",
        requeue_error: (err as Error).message.slice(0, 120),
        note: "Bounded DSR re-enqueue failed; retaining the DLQ copy for delivery retry",
      });
      m.retry(); // keep the DLQ copy; attempt the re-enqueue again
    }
  }
}
