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
import { D1RecoveryStore, type RecoveryStore } from "./dsr_dlq_redrive.js";

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
  /** Cloudflare delivery attempt number; absent only in narrow unit-test fakes. */
  readonly attempts?: number;
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
const MAX_DLQ_PAGING_ATTEMPTS = 3;
const PAGERDUTY_EVENTS_V2_URL = "https://events.pagerduty.com/v2/enqueue";
const DLQ_EVENT_NAME = "dsr.erasure.dead_letter";

type DlqReceiptStatus =
  | "paging_pending"
  | "paging_claimed"
  | "paging_retry"
  | "paging_ambiguous"
  | "paging_recovered"
  | "paged"
  | "requeue_claimed"
  | "requeued"
  | "terminal"
  | "delivery_exhausted"
  | "requeue_ambiguous";

const DLQ_RECEIPT_STATUSES = new Set<DlqReceiptStatus>([
  "paging_pending", "paging_claimed", "paging_retry", "paging_ambiguous",
  "paging_recovered", "paged", "requeue_claimed", "requeued", "terminal",
  "delivery_exhausted", "requeue_ambiguous",
]);

export interface DsrDlqReceipt {
  status: DlqReceiptStatus;
  paging_claimed: number;
  requeue_claimed: number;
}

function isValidDlqReceipt(value: DsrDlqReceipt): boolean {
  return DLQ_RECEIPT_STATUSES.has(value.status)
    && (value.paging_claimed === 0 || value.paging_claimed === 1)
    && (value.requeue_claimed === 0 || value.requeue_claimed === 1);
}

export interface DsrDlqReceiptStore {
  find(eventId: string): Promise<DsrDlqReceipt | null>;
  record(eventId: string, status: DlqReceiptStatus, nowMs: number): Promise<void>;
  claimPaging(eventId: string, nowMs: number): Promise<boolean>;
  claimRequeue(eventId: string, nowMs: number): Promise<boolean>;
}

interface D1Statement {
  bind(...values: unknown[]): D1Statement;
  first<T = unknown>(): Promise<T | null>;
  run(): Promise<{ success: boolean; meta?: { changes?: number } }>;
}

interface D1ReceiptDatabase {
  prepare(sql: string): D1Statement;
}

class D1DsrDlqReceiptStore implements DsrDlqReceiptStore {
  constructor(private readonly db: D1ReceiptDatabase) {}

  async find(eventId: string): Promise<DsrDlqReceipt | null> {
    return this.db
      .prepare("SELECT status, paging_claimed, requeue_claimed FROM dsr_dlq_delivery_receipts WHERE event_id = ?1")
      .bind(eventId)
      .first<DsrDlqReceipt>();
  }

  async record(eventId: string, status: DlqReceiptStatus, nowMs: number): Promise<void> {
    const result = await this.db
      .prepare(
        "INSERT INTO dsr_dlq_delivery_receipts (event_id, status, updated_at_ms) VALUES (?1, ?2, ?3) " +
          "ON CONFLICT(event_id) DO UPDATE SET status = excluded.status, " +
          "paging_claimed = CASE WHEN excluded.status = 'paging_retry' THEN 0 ELSE paging_claimed END, " +
          "updated_at_ms = excluded.updated_at_ms",
      )
      .bind(eventId, status, nowMs)
      .run();
    if (!result.success) throw new Error("dsr_dlq_receipt_write_failed");
  }

  async claimPaging(eventId: string, nowMs: number): Promise<boolean> {
    const result = await this.db
      .prepare(
        "UPDATE dsr_dlq_delivery_receipts SET paging_claimed = 1, status = 'paging_claimed', updated_at_ms = ?2 " +
          "WHERE event_id = ?1 AND paging_claimed = 0",
      )
      .bind(eventId, nowMs)
      .run();
    if (!result.success) throw new Error("dsr_dlq_receipt_paging_claim_failed");
    return result.meta?.changes === 1;
  }

  async claimRequeue(eventId: string, nowMs: number): Promise<boolean> {
    const result = await this.db
      .prepare(
        "UPDATE dsr_dlq_delivery_receipts SET requeue_claimed = 1, status = 'requeue_claimed', updated_at_ms = ?2 " +
          "WHERE event_id = ?1 AND requeue_claimed = 0",
      )
      .bind(eventId, nowMs)
      .run();
    if (!result.success) throw new Error("dsr_dlq_receipt_claim_failed");
    return result.meta?.changes === 1;
  }
}

/**
 * Queue payloads are external input. Normalize the marker before making a
 * retry decision: NaN, fractions, negatives, and strings must never bypass
 * the one-requeue cap or produce misleading operator counts.
 */
function normalizedDlqRequeueCount(value: unknown): number {
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
async function dlqEventId(body: DsrDlqBody, requeueCount: number): Promise<string> {
  const dsrId = typeof body.dsr_id === "string" && body.dsr_id.length > 0 ? body.dsr_id : "unknown";
  const input = new TextEncoder().encode(`corelink/dsr-dlq/v1\u0000${dsrId}\u0000${requeueCount}`);
  const digest = await crypto.subtle.digest("SHA-256", input);
  return `dsr-erasure-dlq:${Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

export interface DsrDlqEnv {
  DSR_QUEUE?: { send(message: unknown): Promise<void> };
  PAGERDUTY_ROUTING_KEY?: string;
  PAGERDUTY_FETCH?: typeof fetch;
  CONFIG_DB?: D1ReceiptDatabase;
  DSR_DLQ_RECEIPTS?: DsrDlqReceiptStore;
  DSR_DLQ_RECOVERY?: RecoveryStore;
}

type PagingResult =
  | { status: "delivered" }
  | { status: "not_configured" }
  | { status: "failed"; error: "http_rejected" | "transport_error" };

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
          summary: "DSR erasure dead-letter exhausted",
          source: "corelink-signup-worker",
          severity: "critical",
          custom_details: {
            event_id: eventId,
            exhausted: true,
            requeue_count: requeueCount,
          },
        },
      }),
    });
    // PagerDuty Events API v2 acknowledges an accepted trigger with 202.  A
    // generic 2xx (for example a proxy-generated 200 page) is not delivery
    // evidence and must not let us consume the DLQ copy.
    if (response.status !== 202) {
      // Do not copy provider response text/status into logs: the response can
      // contain arbitrary data and must never become an operator-log sink.
      return { status: "failed", error: "http_rejected" };
    }
    return { status: "delivered" };
  } catch {
    // The provider/transport exception is intentionally not logged. It may
    // include URLs, credentials, or arbitrary upstream response text.
    return { status: "failed", error: "transport_error" };
  }
}

function logDlqEvent(fields: Record<string, unknown>): void {
  const canonicalEvent = { event: "dsr.erasure.dead_letter" };
  if (fields.event === undefined) fields.event = canonicalEvent.event;
  console.error(JSON.stringify(fields));
}

type ReceiptBoundaryFailure = "not_configured" | "malformed" | "storage_error";

/**
 * A receipt boundary failure must not become a silent platform discard when
 * the DLQ's bounded delivery budget is exhausted. No PagerDuty or requeue side
 * effect is safe without a receipt claim, so emit a redacted terminal alert.
 */
function retryReceiptBoundaryFailure(
  m: QueueMessage<DsrDlqBody>,
  eventId: string,
  requeueCount: number,
  reason: ReceiptBoundaryFailure,
): void {
  if ((m.attempts ?? 0) < MAX_DLQ_PAGING_ATTEMPTS) {
    m.retry();
    return;
  }
  logDlqEvent({
    level: "alert", severity: "critical", event: DLQ_EVENT_NAME,
    component: "dsr-erasure-dlq", event_id: eventId, exhausted: true,
    requeue_count: requeueCount, action: "receipt_boundary_exhausted",
    receipt_boundary: reason,
    note: "DLQ receipt boundary is unavailable at the retry limit; manual operator disposition required",
  });
  m.ack();
}

/**
 * Dead-letter handler for `corelink-dsr-erasure-dlq` (M3, GDPR Art.17).
 *
 * A message reaches here only after exhausting the main queue's 10-retry budget
 * on a *sustained transient* fault (container down, erase-auth key never bound)
 * — permanent 4xx failures are now poison-acked upstream and never dead-letter.
 * For EACH dead-lettered erasure we:
 *   1. emit a STRUCTURED, loud ALERT log keyed by an opaque receipt digest, so
 *      the obligation is visible beyond the hourly verify cron without exposing
 *      data-subject identifiers, and
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
    let eventId: string;
    try {
      eventId = await dlqEventId(body, priorRequeues);
    } catch {
      m.retry();
      continue;
    }
    const store = env.DSR_DLQ_RECEIPTS ?? (env.CONFIG_DB ? new D1DsrDlqReceiptStore(env.CONFIG_DB) : undefined);
    const recovery = env.DSR_DLQ_RECOVERY ?? (env.CONFIG_DB ? new D1RecoveryStore(env.CONFIG_DB) : undefined);
    if (!store || !recovery) {
      retryReceiptBoundaryFailure(m, eventId, priorRequeues, "not_configured");
      continue;
    }
    let existing: DsrDlqReceipt | null;
    try {
      await recovery.capture({ eventId, dsrId: body.dsr_id, tenantId: body.tenant_id, queuedAtMs: body.queued_at_ms, legalHold: body.legal_hold, requeueCount: priorRequeues }, Date.now());
      existing = await store.find(eventId);
      if (existing && !isValidDlqReceipt(existing)) {
        retryReceiptBoundaryFailure(m, eventId, priorRequeues, "malformed");
        continue;
      }
      if (existing?.status === "terminal" || existing?.status === "requeued" || existing?.status === "delivery_exhausted" || existing?.status === "requeue_ambiguous" || existing?.status === "paging_ambiguous") {
        if (priorRequeues === 0 && (existing.status === "terminal" || existing.status === "delivery_exhausted" || existing.status === "paging_ambiguous")) await recovery.ready(eventId, Date.now()); else await recovery.close(eventId, Date.now());
        m.ack();
        continue;
      }
      await store.record(eventId, "paging_pending", Date.now());
    } catch {
      retryReceiptBoundaryFailure(m, eventId, priorRequeues, "storage_error");
      continue;
    }
    try {
      const claimed = await store.claimPaging(eventId, Date.now());
      if (!claimed) {
        await store.record(eventId, "paging_ambiguous", Date.now());
        logDlqEvent({
          level: "alert", severity: "critical", event: DLQ_EVENT_NAME,
          component: "dsr-erasure-dlq", event_id: eventId, exhausted: true,
          requeue_count: priorRequeues, action: "paging_ambiguous",
          note: "PagerDuty delivery has an ambiguous durable claim; manual operator disposition required",
        });
        m.ack();
        continue;
      }
    } catch {
      retryReceiptBoundaryFailure(m, eventId, priorRequeues, "storage_error");
      continue;
    }
    const canRequeue = priorRequeues < MAX_DLQ_REQUEUES && !!env.DSR_QUEUE;

    const paging = await pageDlqEvent(body, eventId, priorRequeues, env);
    if (paging.status !== "delivered") {
      if (paging.status === "failed" && paging.error === "transport_error") {
        try {
          await store.record(eventId, "paging_ambiguous", Date.now());
          await recovery.ready(eventId, Date.now());
        } catch {
          m.retry();
          continue;
        }
        logDlqEvent({
          level: "alert", severity: "critical", event: DLQ_EVENT_NAME,
          component: "dsr-erasure-dlq", event_id: eventId, exhausted: true,
          requeue_count: priorRequeues, action: "paging_ambiguous",
          paging_status: paging.status, paging_error: paging.error,
          note: "PagerDuty transport outcome is ambiguous; durable operator disposition required",
        });
        m.ack();
        continue;
      }
      const exhausted = (m.attempts ?? 0) >= MAX_DLQ_PAGING_ATTEMPTS;
      try {
        await store.record(eventId, exhausted ? "delivery_exhausted" : "paging_retry", Date.now());
        if (exhausted && priorRequeues === 0) await recovery.ready(eventId, Date.now());
      } catch {
        m.retry();
        continue;
      }
      logDlqEvent({
        level: "alert",
        severity: "critical",
        event: DLQ_EVENT_NAME,
        component: "dsr-erasure-dlq",
        event_id: eventId,
        exhausted: true,
        requeue_count: priorRequeues,
        action: exhausted ? "delivery_exhausted" : "retry_paging",
        paging_status: paging.status,
        paging_error: paging.status === "failed" ? paging.error : "route_not_configured",
        note: exhausted
          ? "PagerDuty delivery retry budget exhausted; durable operator disposition required"
          : "PagerDuty did not accept the exhausted DSR alert; retaining the DLQ copy for delivery retry",
      });
      if (exhausted) m.ack(); else m.retry();
      continue;
    }

    const recovered = existing?.status === "paging_retry";
    try {
      await store.record(eventId, recovered ? "paging_recovered" : "paged", Date.now());
    } catch {
      m.retry();
      continue;
    }

    if (!canRequeue) {
      logDlqEvent({
        level: "alert",
        severity: "critical",
        event: DLQ_EVENT_NAME,
        component: "dsr-erasure-dlq",
        event_id: eventId,
        exhausted: true,
        requeue_count: priorRequeues,
        action: "left_dead",
        recovered,
        paging_status: paging.status,
        paging_configured: true,
        note: "GDPR Art.17 erasure dead after bounded re-enqueue — MANUAL operator action required",
      });
      try {
        await store.record(eventId, "terminal", Date.now());
        if (priorRequeues === 0) await recovery.ready(eventId, Date.now()); else await recovery.close(eventId, Date.now());
        m.ack();
      } catch {
        m.retry();
      }
      continue;
    }
    try {
      const claimed = await store.claimRequeue(eventId, Date.now());
      if (!claimed) {
        await store.record(eventId, "requeue_ambiguous", Date.now());
        logDlqEvent({
          level: "alert", severity: "critical", event: DLQ_EVENT_NAME,
          component: "dsr-erasure-dlq", event_id: eventId, exhausted: true,
          requeue_count: priorRequeues, action: "requeue_ambiguous",
          recovered,
          paging_status: paging.status, paging_configured: true,
          note: "A bounded DSR re-enqueue has an ambiguous durable claim; manual operator disposition required",
        });
        m.ack();
        continue;
      }
      await env.DSR_QUEUE!.send({ ...body, _dlq_requeue: priorRequeues + 1 });
        await store.record(eventId, "requeued", Date.now());
        await recovery.close(eventId, Date.now());
      logDlqEvent({
        level: "alert",
        severity: "critical",
        event: DLQ_EVENT_NAME,
        component: "dsr-erasure-dlq",
        event_id: eventId,
        exhausted: true,
        requeue_count: priorRequeues + 1,
        action: "requeue_once",
        recovered,
        paging_status: paging.status,
        paging_configured: true,
        note: "GDPR Art.17 erasure exhausted main-queue retries — re-enqueued for ONE bounded final attempt",
      });
      m.ack(); // handed back to the main queue; consume the DLQ copy
    } catch {
      try {
        await store.record(eventId, "requeue_ambiguous", Date.now());
        await recovery.close(eventId, Date.now());
      } catch {
        m.retry();
        continue;
      }
      logDlqEvent({
        level: "alert",
        severity: "critical",
        event: DLQ_EVENT_NAME,
        component: "dsr-erasure-dlq",
        event_id: eventId,
        exhausted: true,
        requeue_count: priorRequeues,
        action: "requeue_ambiguous",
        recovered,
        paging_status: paging.status,
        paging_configured: true,
        requeue_error: "transport_error",
        note: "Bounded DSR re-enqueue is ambiguous; durable operator disposition required",
      });
      m.ack();
    }
  }
}
