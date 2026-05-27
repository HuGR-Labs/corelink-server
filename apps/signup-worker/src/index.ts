// `corelink-signup-worker` entry point.
//
// Phase 0.G seeded this app with the webhook handlers wired to emit the
// PLG §7.1 analytics events. Auto-provision logic, Clerk Svix verification,
// and Stripe signature verification land in Phases 0.F and 0.C respectively.

import { handleClerkWebhook, type ClerkWebhookEnv } from "./webhooks/clerk";
import { handleStripeWebhook, type StripeWebhookEnv } from "./webhooks/stripe";

interface Env extends ClerkWebhookEnv, StripeWebhookEnv {
    ENVIRONMENT: string;
}

export default {
    async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
        const url = new URL(request.url);

        if (url.pathname === "/healthz") {
            return Response.json({ ok: true, env: env.ENVIRONMENT, ts: new Date().toISOString() });
        }
        if (url.pathname === "/webhooks/clerk" && request.method === "POST") {
            return handleClerkWebhook(request, env, ctx);
        }
        if (url.pathname === "/webhooks/stripe" && request.method === "POST") {
            return handleStripeWebhook(request, env, ctx);
        }
        return new Response("Not Found", { status: 404 });
    },
} satisfies ExportedHandler<Env>;
