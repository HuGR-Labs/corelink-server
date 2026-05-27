/**
 * CoreLink signup-worker — Cloudflare Worker entry point.
 *
 * Routes:
 *   POST /webhooks/clerk  → Clerk `user.created` auto-provision handler
 *
 * Future routes (out of Phase-0 scope, owned by agent C):
 *   POST /webhooks/stripe → `checkout.session.completed` plan-flip handler
 */

import { handleClerkWebhook, defaultApiClient } from "./webhooks/clerk.js";
import type { AutoProvisionEnv } from "./webhooks/clerk.js";
import { withSecurityHeaders } from "./security-headers.js";

async function route(request: Request, env: AutoProvisionEnv): Promise<Response> {
  const url = new URL(request.url);
  if (url.pathname === "/webhooks/clerk") {
    return handleClerkWebhook(request, env, defaultApiClient);
  }
  if (url.pathname === "/health") {
    return Response.json({ ok: true, worker: "signup-worker" });
  }
  return new Response("not_found", { status: 404 });
}

export default {
  async fetch(request: Request, env: AutoProvisionEnv): Promise<Response> {
    // Always wrap responses in security headers (pre-HN-launch hardening).
    return withSecurityHeaders(await route(request, env));
  },
};
