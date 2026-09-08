/** Clerk-authenticated onboarding route domain. */
import type { ExecutionContext } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import { applyCors, stripClientTrustHeaders } from "./index_auth.js";
import type { RouteMatch } from "./route_match.js";
import { resolveOnboardingAuthKey } from "./index_auth.js";
import { reapiError, serverGetOpts } from "./index_common.js";
import { verifyClerkSessionAndResolveTenant } from "./lib/clerk_auth.js";

export async function handleSpecialOnboardingRoute(
  request: Request,
  env: Env,
  _ctx: ExecutionContext,
  requestId: string,
  _requestStart: number,
  _requestCounter: number,
  route: RouteMatch,
): Promise<Response | null> {
    if (route.routeKind === "onboarding") {
      const internalAuthKey = resolveOnboardingAuthKey(route.pathSuffix, env);
      const clerkSecretKey = env.CLERK_SECRET_KEY;
      if (!internalAuthKey || internalAuthKey.length === 0 || !clerkSecretKey) {
        // A required server secret is unbound — deny (fail-CLOSED).
        return applyCors(
          reapiError("FORBIDDEN", "onboarding route unavailable", 403, requestId),
          request,
        );
      }

      // Verify the Clerk session + resolve the tenant via the SHARED pipeline
      // (worker/src/lib/clerk_auth.ts — dashboard revival WP-1 extraction).
      // The helper carries the full hardened flow verbatim: bearer extraction
      // (401), verifyToken with the shared azp allowlist (M1 post-verify
      // re-assert), issuer exact-pin / shape-check (M2), claims.sub required,
      // and the tenant lookup by clerk_user_id (no row → 403; D1 error → 500).
      // `clerkSecretKey` presence was already asserted by the arm-level
      // fail-CLOSED guard above, so the helper's own secret guard never fires
      // here (onboarding behavior unchanged).
      const onbClerkAuth = await verifyClerkSessionAndResolveTenant(request, env, requestId);
      if (!onbClerkAuth.ok) {
        return applyCors(onbClerkAuth.response, request);
      }
      const onbTenantId = onbClerkAuth.tenantId;

      // Forward to the tenant's DO. CRITICAL: strip ALL client-supplied trust
      // headers FIRST (the browser must never spoof internal-auth or the tenant
      // id), then set them from server-trusted values. Drop the Clerk JWT — the
      // container authenticates via internal-auth, not the session token.
      const onbDoId = env.CORELINK_SERVER.idFromName(onbTenantId);
      const onbStub = env.CORELINK_SERVER.get(onbDoId, serverGetOpts(env));
      const onbAugmented = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          // Strip the FULL set of client-suppliable trust headers (x-admin-*,
          // fanout-from, scope, tenant-id, internal-auth) BEFORE re-establishing
          // them from server-trusted values (delete-then-set).
          stripClientTrustHeaders(h);
          // Belt-and-braces: x-corelink-tenant-id is now in the strip list so
          // the line above already removed any client value; kept for clarity.
          h.delete("x-corelink-tenant-id");
          h.delete("authorization");
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "onboarding");
          h.set("x-corelink-token-prefix", "clerk");
          h.set("x-corelink-tenant-id", onbTenantId);
          h.set("x-corelink-internal-auth", internalAuthKey);
          return h;
        })(),
      });
      let onbResp: Response;
      try {
        onbResp = await onbStub.fetch(onbAugmented);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] onboarding DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "onboarding upstream error", 500, requestId),
          request,
        );
      }
      const onbHeaders = new Headers(onbResp.headers);
      if (!onbHeaders.has("x-request-id")) {
        onbHeaders.set("x-request-id", requestId);
      }
      return applyCors(
        new Response(onbResp.body, {
          status: onbResp.status,
          statusText: onbResp.statusText,
          headers: onbHeaders,
        }),
        request,
      );
    }

  return null;
}
