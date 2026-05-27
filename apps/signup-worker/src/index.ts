/**
 * CoreLink signup-worker — Cloudflare Worker entry point.
 *
 * Routes:
 *   POST /webhooks/clerk  → Clerk `user.created` auto-provision handler
 *
 * Future routes (out of Phase-0 scope, owned by agent C):
 *   POST /webhooks/stripe → `checkout.session.completed` plan-flip handler
 */

import * as Sentry from "@sentry/cloudflare";
import { handleClerkWebhook, defaultApiClient } from "./webhooks/clerk.js";
import type { AutoProvisionEnv } from "./webhooks/clerk.js";

/**
 * Sentry-wrapped signup worker. The webhook path mutates auth state
 * (Clerk → tenant provision); errors here have outsized blast radius
 * so we capture every throw + every webhook signature-failure. The
 * Authorization scrub is critical for this Worker — Clerk webhooks
 * carry a `svix-signature` header which is a HMAC secret-bearing token.
 */
type SignupEnv = AutoProvisionEnv & {
  SENTRY_DSN?: string;
  SENTRY_RELEASE?: string;
  ENVIRONMENT?: string;
};

const baseHandler: ExportedHandler<SignupEnv> = {
  async fetch(request: Request, env: SignupEnv): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (url.pathname === "/webhooks/clerk") {
        return handleClerkWebhook(request, env, defaultApiClient);
      }
      if (url.pathname === "/health") {
        return Response.json({ ok: true, worker: "signup-worker" });
      }
      return new Response("not_found", { status: 404 });
    } catch (err) {
      Sentry.captureException(err);
      return new Response("internal_error", { status: 500 });
    }
  },
};

const SENSITIVE_HEADER_PATTERN =
  /^(authorization|cookie|set-cookie|x-api-key|proxy-authorization|svix-signature|svix-id|svix-timestamp)$/i;

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
