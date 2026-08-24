/**
 * CoreLink signup-worker — Cloudflare Worker entry point.
 *
 * Routes:
 *   POST /webhooks/clerk  → Clerk `user.created` auto-provision handler
 *   POST /webhooks/stripe → Stripe subscription lifecycle webhook handler
 *                           (checkout.session.completed → tenant_billing paid;
 *                            customer.subscription.updated / deleted — Stream 2.10)
 *   POST /internal/v1/runner/provision-installation
 *                         → cf-multitenant WP4 identity-gated provisioning
 *                           primitive (installation→tenant map + repo allowlist)
 */

import * as Sentry from "@sentry/cloudflare";
import { scrubSentryEvent } from "./sentry-scrub.js";
import { handleClerkWebhook, defaultApiClient } from "./webhooks/clerk.js";
import type { AutoProvisionEnv, DsrQueuedV1 } from "./webhooks/clerk.js";
import { handleStripeWebhook } from "./webhooks/stripe.js";
import type { StripeWebhookEnv } from "./webhooks/stripe.js";
import { handleInstallationProvision } from "./webhooks/github_provision.js";
import type { InstallationProvisionEnv } from "./webhooks/github_provision.js";
import {
  handleAppManifestForm,
  handleAppManifestCallback,
} from "./webhooks/github_app_manifest.js";
import type { GithubAppManifestEnv } from "./webhooks/github_app_manifest.js";
import { handleInstallGithubCallback } from "./webhooks/github_install_callback.js";
import type { InstallCallbackEnv } from "./webhooks/github_install_callback.js";
import { handleErasureQueueBatch, handleErasureDlqBatch } from "./webhooks/dsr_consumer.js";
import type { QueueMessageBatch, DsrDlqBody } from "./webhooks/dsr_consumer.js";
import { runDsrVerifySweep } from "./webhooks/dsr_verify_cron.js";
import { runPatScrubSweep } from "./webhooks/pat_scrub_cron.js";
import { runAuditDrainSweep } from "./webhooks/audit_drain_cron.js";
import { runAuditArchiveSweep } from "./webhooks/audit_archive_cron.js";
import { withSecurityHeaders } from "./security-headers.js";

type WorkerEnv = AutoProvisionEnv &
  StripeWebhookEnv &
  InstallationProvisionEnv &
  GithubAppManifestEnv &
  InstallCallbackEnv;

async function route(request: Request, env: WorkerEnv, ctx: ExecutionContext): Promise<Response> {
  const url = new URL(request.url);
  if (url.pathname === "/webhooks/clerk") {
    return handleClerkWebhook(request, env, defaultApiClient);
  }
  if (url.pathname === "/webhooks/stripe") {
    return handleStripeWebhook(request, env, ctx);
  }
  if (url.pathname === "/internal/v1/runner/provision-installation") {
    return handleInstallationProvision(request, env);
  }
  if (url.pathname === "/install/github/app/new" && request.method === "GET") {
    return handleAppManifestForm(request, env);
  }
  if (url.pathname === "/install/github/app/created" && request.method === "GET") {
    return handleAppManifestCallback(request, env);
  }
  if (url.pathname === "/install/github/callback" && request.method === "GET") {
    return handleInstallGithubCallback(request, env);
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

  // DSR erasure queue consumer. ONE handler serves BOTH bound consumers,
  // dispatched on `batch.queue`:
  //   • corelink-dsr-erasure      → handleErasureQueueBatch (dsr.queued.v1 →
  //     container /_internal/dsr/erase; per-message ack/poison-ack/retry).
  //   • corelink-dsr-erasure-dlq  → handleErasureDlqBatch (M3: structured
  //     Art.17 alert + ONE bounded re-enqueue onto the main queue).
  // Per-message disposition lives in the handlers; a thrown error here is
  // captured + rethrown so the queue runtime redelivers the whole batch (the
  // erasure orchestrator is idempotent, so redelivery is safe).
  async queue(batch, env: SignupEnv): Promise<void> {
    try {
      if (batch.queue === "corelink-dsr-erasure-dlq") {
        await handleErasureDlqBatch(
          batch as unknown as QueueMessageBatch<DsrDlqBody>,
          env,
        );
      } else {
        await handleErasureQueueBatch(
          batch as unknown as QueueMessageBatch<DsrQueuedV1>,
          env,
        );
      }
    } catch (err) {
      Sentry.captureException(err);
      throw err;
    }
  },

  // Hourly Cron Trigger (`0 * * * *`). Drives three independent sweeps:
  //   1. DSR 24h verification sweep — re-fingerprints every DSR past its 24h
  //      SLA deadline via the container /_internal/dsr/verify endpoint (inert
  //      until CORELINK_INTERNAL_AUTH_KEY is bound, task #46).
  //   2. PAT-plaintext scrub (CTRL-CRED-001) — clears `private_metadata.
  //      pat_plaintext` for any Clerk user whose reveal is older than the TTL,
  //      so an un-visited /welcome cannot leave the secret resident (inert
  //      until CLERK_SECRET_KEY is bound).
  //   3. S-09 audit-chain drain — seals pending audit_outbox rows into the
  //      tamper-evident hash chain via the container /_internal/audit/drain
  //      endpoint (idempotent; inert until the erase/internal-auth key is
  //      bound, task #46).
  async scheduled(_event, env: SignupEnv, ctx: ExecutionContext): Promise<void> {
    const nowMs = Date.now();

    const db = env.CONFIG_DB;
    if (db) {
      ctx.waitUntil(
        runDsrVerifySweep({ ...env, CONFIG_DB: db }, nowMs)
          .then((r) => {
            if (r.skipped) {
              // A sweep that could not authenticate must never look like a
              // sweep with nothing to do (B-020).
              console.warn(
                "[dsr-verify-cron] skipped=true reason=internal-auth-key-unbound",
              );
            } else {
              console.log(
                `[dsr-verify-cron] swept=${r.swept} failed=${r.failed}`,
              );
            }
          })
          .catch((err: unknown) => {
            Sentry.captureException(err);
          }),
      );
    } else {
      console.warn("[dsr-verify-cron] skipped=true reason=CONFIG_DB-unbound");
    }

    ctx.waitUntil(
      runPatScrubSweep(env, nowMs)
        .then((r) => {
          if (r.skipped) {
            console.warn(
              "[pat-scrub-cron] skipped=true reason=CLERK_SECRET_KEY-unbound",
            );
          } else {
            console.log(
              `[pat-scrub-cron] scanned=${r.scanned} scrubbed=${r.scrubbed} failed=${r.failed}`,
            );
          }
        })
        .catch((err: unknown) => {
          Sentry.captureException(err);
        }),
    );

    ctx.waitUntil(
      runAuditDrainSweep(env, nowMs)
        .then((r) => {
          if (r.skipped) {
            console.warn(
              "[audit-drain-cron] skipped=true reason=erase-auth-key-unbound",
            );
          } else {
            console.log(
              `[audit-drain-cron] ok=${r.ok} status=${r.status} sealed=${r.sealed} partitions=${r.partitions}`,
            );
          }
        })
        .catch((err: unknown) => {
          Sentry.captureException(err);
        }),
    );

    // Runs alongside the drain, not after it: the archive only ever touches
    // rows the drain has ALREADY sealed, so ordering between the two ticks does
    // not matter — rows sealed this hour are archived next hour at the latest.
    ctx.waitUntil(
      runAuditArchiveSweep(env, nowMs)
        .then((r) => {
          if (r.skipped) {
            console.warn(
              "[audit-archive-cron] skipped=true reason=erase-auth-key-unbound",
            );
          } else {
            console.log(
              `[audit-archive-cron] ok=${r.ok} status=${r.status} rows=${r.rowsArchived} chunks=${r.chunksCreated} failed_partitions=${r.partitionsFailed} incomplete=${r.incomplete}`,
            );
          }
        })
        .catch((err: unknown) => {
          Sentry.captureException(err);
        }),
    );
  },
};

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
    // Scrub PII/secrets from message + exception bodies + extra/contexts
    // VALUES (not just header keys) before any event leaves the Worker.
    beforeSend(event: Sentry.ErrorEvent) {
      return scrubSentryEvent(event);
    },
    beforeSendTransaction(event) {
      return scrubSentryEvent(event);
    },
  }),
  // `@sentry/cloudflare` re-bundles `@cloudflare/workers-types`; cast keeps
  // both type graphs happy without weakening the inner handler's types.
  baseHandler as unknown as Parameters<typeof Sentry.withSentry>[1],
);
