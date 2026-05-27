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

import type { Env } from "./types";
import { handleIngest } from "./ingest";
import { runWeeklyDigest, computeThreeNumbers } from "./cron/weekly-email";

export default {
    async fetch(request: Request, env: Env, _ctx: ExecutionContext): Promise<Response> {
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
    },

    async scheduled(_controller: ScheduledController, env: Env, ctx: ExecutionContext): Promise<void> {
        // Cron handler — runs Mondays 08:00 UTC per wrangler.toml [triggers].
        // Wrap in waitUntil so the runtime keeps the worker alive while
        // Resend RTTs back; otherwise the schedule callback returns
        // before the HTTP fetch completes.
        ctx.waitUntil(runWeeklyDigest(env));
    },
} satisfies ExportedHandler<Env>;
