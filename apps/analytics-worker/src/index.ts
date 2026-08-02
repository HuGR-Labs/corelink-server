// CoreLink analytics worker entry-point.
//
// Routes:
//   POST /v1/event   → ingest (see ingest.ts)
//   GET  /healthz    → 200 + { ok: true } (used by Phase 0.G acceptance §2)
//   GET  /v1/digest/preview — debug-only (ENVIRONMENT==='dev'): returns the
//                           three-numbers digest body as JSON without sending
//                           an email. Lets the operator dry-run the cron.
//
// Cron:
//   Mondays 08:00 UTC → src/cron/weekly-email.ts
//
// RPC (service binding):
//   `AnalyticsIngest.ingestServerEvent` — the keyless, trusted-by-construction
//   path for server-only events (see the class doc below).

import { WorkerEntrypoint } from "cloudflare:workers";
import * as Sentry from "@sentry/cloudflare";
import { scrubSentryEvent } from "./sentry-scrub";
import type { Env, EventPayload } from "./types";
import { handleIngest, ingestEvents, type IngestResult } from "./ingest";
import { runWeeklyDigest, computeThreeNumbers } from "./cron/weekly-email";
import { withSecurityHeaders } from "./security-headers";

async function route(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === "/v1/event") {
        return handleIngest(request, env);
    }
    if (url.pathname === "/healthz") {
        return Response.json({
            ok: true,
            env: env.ENVIRONMENT,
            ts: new Date().toISOString(),
        });
    }
    if (url.pathname === "/v1/digest/preview" && env.ENVIRONMENT === "dev") {
        const numbers = await computeThreeNumbers(env);
        return Response.json(numbers);
    }
    return new Response("Not Found", { status: 404 });
}

/**
 * Sentry wrapper — see `get-corelink-worker/src/index.ts` for the rationale.
 * The wrapper is a no-op when `env.SENTRY_DSN` is unset so unit tests do
 * not need to provide a DSN.
 */
const baseHandler: ExportedHandler<Env> = {
    async fetch(request: Request, env: Env, _ctx: ExecutionContext): Promise<Response> {
        // Always wrap responses in security headers (pre-HN-launch hardening).
        // Sentry captures any thrown error before we 500.
        try {
            return withSecurityHeaders(await route(request, env));
        } catch (err) {
            Sentry.captureException(err);
            return withSecurityHeaders(new Response("Internal Server Error", { status: 500 }));
        }
    },

    async scheduled(_controller: ScheduledController, env: Env, ctx: ExecutionContext): Promise<void> {
        // Cron handler — runs Mondays 08:00 UTC per wrangler.toml [triggers].
        // Wrap in waitUntil so the runtime keeps the worker alive while
        // Resend RTTs back; otherwise the schedule callback returns
        // before the HTTP fetch completes.
        ctx.waitUntil(
            runWeeklyDigest(env).catch((err: unknown) => {
                Sentry.captureException(err);
                throw err;
            }),
        );
    },
} satisfies ExportedHandler<Env>;

// Extend Env with Sentry-specific bindings without modifying the canonical
// types module (keeps `types.ts` focused on analytics-domain bindings).
interface EnvWithSentry extends Env {
    SENTRY_DSN?: string;
    SENTRY_RELEASE?: string;
}

export default Sentry.withSentry(
    (env: EnvWithSentry) => ({
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
) as ExportedHandler<Env>;

/**
 * `AnalyticsIngest` — the RPC entrypoint for TRUSTED, SERVER-ONLY events.
 *
 * Bound by a caller Worker as:
 *
 *     [[env.prod.services]]
 *     binding    = "ANALYTICS_SVC"
 *     service    = "corelink-analytics-prod"
 *     entrypoint = "AnalyticsIngest"       # ← required: without it the binding
 *                                          #   resolves to the default `fetch`
 *                                          #   export, which has no RPC methods
 *
 * and called as `await env.ANALYTICS_SVC.ingestServerEvent({...})`.
 *
 * ── WHY THERE IS NO KEY CHECK HERE (this is NOT a missing check) ───────────
 * `ingestServerEvent` passes `trusted = true` unconditionally. That is an
 * authentication CONCLUSION, not a skipped step:
 *
 *   - A service binding is not a URL. It is resolved by the Cloudflare control
 *     plane at deploy time from the CALLER's own config, and the call never
 *     leaves the runtime — there is no hostname to point at, no socket to open,
 *     no header to forge. The ONLY way to obtain this stub is to be a Worker in
 *     this account whose deployed config declares the binding above; that
 *     declaration is itself an authenticated act (a `wrangler deploy` with an
 *     account-scoped token). The platform authenticates the caller's IDENTITY,
 *     which is strictly stronger than a shared static secret that both sides
 *     must hold, that lives in two secret stores, and that leaks on any log.
 *   - A browser can NEVER reach this method. RPC methods are not exposed over
 *     HTTP: the public surface of this Worker is the default `fetch` export on
 *     the `humangr.com` routes, and that path still enforces the original
 *     `Origin`-allow-list-OR-`X-Corelink-Ingest-Key` split
 *     (`ingest.ts:159-168`) and still rejects `SERVER_ONLY_EVENT_NAMES` on the
 *     spoofable-`Origin` browser path (`ingest.ts:120-122`). Nothing about the
 *     HTTP contract changes because this class exists.
 *
 * The invariant that keeps the above true: this class must never be made
 * reachable by an untrusted caller — do NOT add a `fetch()` method to it, do
 * NOT put a route on it, and do NOT widen it beyond the analytics-event shape.
 * If any of those changes, `trusted = true` stops being sound and the caller
 * must be authenticated explicitly again.
 *
 * The pre-existing `INGEST_KEY` HTTP path is untouched and still serves its
 * current callers (signup-worker, cas-worker, get-corelink-worker); this is an
 * ADDITIONAL, key-free path for Workers that can hold a binding.
 */
export class AnalyticsIngest extends WorkerEntrypoint<Env> {
    /**
     * Write ONE trusted server event. Deliberately narrow: a single event, the
     * `EventPayload` shape, nothing else. It shares the SAME validator and the
     * SAME `INSERT OR IGNORE` write as `POST /v1/event` (`ingestEvents`,
     * `ingest.ts`), so the two surfaces cannot drift.
     *
     * Idempotency is the caller's lever: pass a DETERMINISTIC `id` and the
     * `analytics_events` primary key coalesces every re-send (the table's only
     * uniqueness constraint). Omit `created_at` and ingest stamps its own clock.
     *
     * Never throws — an invalid event comes back as `{accepted:0, rejected:1}`
     * with a reason, and a D1 fault as a `d1_error:*` reason. Callers emit this
     * on a hot path (`ctx.waitUntil`) and must not be able to fail because
     * analytics did. A throw would cross the RPC boundary as an exception in
     * the CALLER's isolate, so the catch below is load-bearing, not decorative.
     */
    async ingestServerEvent(event: EventPayload): Promise<IngestResult> {
        try {
            return await ingestEvents([event], this.env, true);
        } catch (err) {
            Sentry.captureException(err);
            return {
                accepted: 0,
                rejected: 1,
                errors: [{ reason: `rpc_error:${(err as Error).message}` }],
            };
        }
    }
}
