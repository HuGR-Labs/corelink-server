/**
 * CoreLink signup-worker — Cloudflare Worker entry point.
 *
 * Routes:
 *   POST /webhooks/clerk  → Clerk `user.created` auto-provision handler
 *   POST /webhooks/stripe → Stripe subscription lifecycle webhook handler
 *                           (checkout.session.completed → tenant_billing paid;
 *                            customer.subscription.updated / deleted — Stream 2.10)
 */

import * as Sentry from "@sentry/cloudflare";
import { handleClerkWebhook, defaultApiClient } from "./webhooks/clerk.js";
import type { AutoProvisionEnv, DsrQueuedV1 } from "./webhooks/clerk.js";
import { handleStripeWebhook } from "./webhooks/stripe.js";
import type { StripeWebhookEnv } from "./webhooks/stripe.js";
import { handleErasureQueueBatch } from "./webhooks/dsr_consumer.js";
import type { QueueMessageBatch } from "./webhooks/dsr_consumer.js";
import { runDsrVerifySweep } from "./webhooks/dsr_verify_cron.js";
import { withSecurityHeaders } from "./security-headers.js";

type WorkerEnv = AutoProvisionEnv & StripeWebhookEnv;

async function route(request: Request, env: WorkerEnv, ctx: ExecutionContext): Promise<Response> {
  const url = new URL(request.url);
  if (url.pathname === "/webhooks/clerk") {
    return handleClerkWebhook(request, env, defaultApiClient);
  }
  if (url.pathname === "/webhooks/stripe") {
    return handleStripeWebhook(request, env, ctx);
  }
  if (url.pathname === "/health") {
    return Response.json({ ok: true, worker: "signup-worker" });
  }
  return new Response("not_found", { status: 404 });
}

/**
 * Sentry-wrapped signup worker. The webhook path mutates auth state
 * (Clerk → tenant provision); errors here have outsized blast radius
 * so we capture every throw + every webhook signature-failure. The
 * Authorization scrub is critical for this Worker — Clerk webhooks
 * carry a `svix-signature` header which is a HMAC secret-bearing token;
 * Stripe webhooks carry a `stripe-signature` header with HMAC material.
 *
 * All responses are additionally wrapped in `withSecurityHeaders`
 * (pre-HN-launch hardening).
 */
type SignupEnv = WorkerEnv & {
  SENTRY_DSN?: string;
  SENTRY_RELEASE?: string;
  ENVIRONMENT?: string;
};

const baseHandler: ExportedHandler<SignupEnv> = {
  async fetch(request: Request, env: SignupEnv, ctx: ExecutionContext): Promise<Response> {
    try {
      return withSecurityHeaders(await route(request, env, ctx));
    } catch (err) {
      Sentry.captureException(err);
      return withSecurityHeaders(new Response("internal_error", { status: 500 }));
    }
  },

  // DSR erasure queue consumer (dsr.queued.v1 → container /_internal/dsr/erase).
  // Per-message ack/retry lives in handleErasureQueueBatch; a thrown error here
  // is captured + rethrown so the queue runtime redelivers the whole batch
  // (the erasure orchestrator is idempotent, so redelivery is safe).
  async queue(batch, env: SignupEnv): Promise<void> {
    try {
      await handleErasureQueueBatch(
        batch as unknown as QueueMessageBatch<DsrQueuedV1>,
        env,
      );
    } catch (err) {
      Sentry.captureException(err);
      throw err;
    }
  },

  // DSR 24h verification sweep (Cron Trigger). Re-fingerprints every DSR past
  // its 24h SLA deadline via the container /_internal/dsr/verify endpoint.
  // Inert until CORELINK_INTERNAL_AUTH_KEY is bound (task #46).
  async scheduled(_event, env: SignupEnv, ctx: ExecutionContext): Promise<void> {
    const db = env.CONFIG_DB;
    if (!db) {
      return;
    }
    ctx.waitUntil(
      runDsrVerifySweep({ ...env, CONFIG_DB: db }, Date.now())
        .then((r) => {
          if (!r.skipped) {
            console.log(
              `[dsr-verify-cron] swept=${r.swept} failed=${r.failed}`,
            );
          }
        })
        .catch((err: unknown) => {
          Sentry.captureException(err);
        }),
    );
  },
};

const SENSITIVE_HEADER_PATTERN =
  /^(authorization|cookie|set-cookie|x-api-key|proxy-authorization|svix-signature|svix-id|svix-timestamp|stripe-signature)$/i;

function scrubAuthorization(event: Sentry.ErrorEvent): Sentry.ErrorEvent {
  if (event.request?.headers) {
    const h = event.request.headers as Record<string, string>;
    for (const k of Object.keys(h)) {
      if (SENSITIVE_HEADER_PATTERN.test(k)) {
        h[k] = "[Filtered]";
      }
    }
  }
  return event;
}

export default Sentry.withSentry(
  (env: SignupEnv) => ({
    // Empty string when secret unset → Sentry SDK treats as init no-op.
    dsn: env.SENTRY_DSN ?? "",
    environment: env.ENVIRONMENT,
    // Default release tag to "unknown" so the strict type accepts it; the
    // real value flows from `SENTRY_RELEASE` Worker var set at deploy time.
    release: env.SENTRY_RELEASE ?? "unknown",
    sendDefaultPii: false,
    tracesSampleRate: 0.1,
    sampleRate: 1.0,
    beforeSend(event: Sentry.ErrorEvent) {
      return scrubAuthorization(event);
    },
  }),
  // `@sentry/cloudflare` re-bundles `@cloudflare/workers-types`; cast keeps
  // both type graphs happy without weakening the inner handler's types.
  baseHandler as unknown as Parameters<typeof Sentry.withSentry>[1],
);
