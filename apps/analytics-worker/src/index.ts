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

import * as Sentry from "@sentry/cloudflare";
import type { Env } from "./types";
import { handleIngest } from "./ingest";
import { runWeeklyDigest, computeThreeNumbers } from "./cron/weekly-email";

/**
 * Sentry wrapper — see `get-corelink-worker/src/index.ts` for the rationale.
 * The wrapper is a no-op when `env.SENTRY_DSN` is unset so unit tests do
 * not need to provide a DSN.
 */
const baseHandler: ExportedHandler<Env> = {
    async fetch(request: Request, env: Env, _ctx: ExecutionContext): Promise<Response> {
        try {
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
        } catch (err) {
            Sentry.captureException(err);
            return new Response("Internal Server Error", { status: 500 });
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

const SENSITIVE_HEADER_PATTERN = /^(authorization|cookie|set-cookie|x-api-key|proxy-authorization)$/i;

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
        beforeSend(event: Sentry.ErrorEvent) {
            return scrubAuthorization(event);
        },
    }),
    // `@sentry/cloudflare` re-bundles `@cloudflare/workers-types`; cast keeps
    // both type graphs happy without weakening the inner handler's types.
    baseHandler as unknown as Parameters<typeof Sentry.withSentry>[1],
) as ExportedHandler<Env>;
