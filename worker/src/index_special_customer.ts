/** DevEnv and customer portal special routes. */
import type { Env } from "./index_common.js";
import { applyCors, extractAuth, stripClientTrustHeaders } from "./index_auth.js";
import { parsePat, MFA_FVA_FRESH_MAX_MINUTES } from "./index_auth.js";
import type { RouteMatch } from "./route_match.js";
import { reapiError, serverGetOpts } from "./index_common.js";
import { verifyClerkSessionAndResolveTenant } from "./lib/clerk_auth.js";
import { emailHashCandidates, redeemTeamInvitation, verifiedPrimaryEmail } from "./lib/team_invitation.js";
import { isProductionEnvironment } from "../../config/production_environment.js";
import type { DurableObjectStub } from "@cloudflare/workers-types";
import type { RunnerDevEnvRpc } from "./types/devenv_rpc.js";
import { bounded, adoptDevenvOperation, revokeDevenvOperation } from "./lib/devenv_cleanup.js";
import { DEVENV_MAX_TTL_SECONDS, isDevenvStart, mayAccessDevenv, relayAuthorizedDevenvStart } from "./lib/devenv_relay.js";
import { prepareDevenvCompute } from "./lib/devenv_compute.js";
import { readCredentialLifecycle } from "./lib/credential_lifecycle_client.js";
import { mintScopedPat, MintGrant } from "./lib/session_exchange.js";

export async function handleSpecialCustomerRoute(
  request: Request,
  env: Env,
  requestId: string,
  route: RouteMatch,
): Promise<Response | null> {
    // DevEnv cloud development environments handler (WP-08)
    if (route.routeKind === "devenv_v1") {
      const devAuthz = request.headers.get("authorization") ?? "";
      const devToken = devAuthz.startsWith("Bearer ")
        ? devAuthz.slice("Bearer ".length).trim()
        : "";

      let devTenantId: string;
      let devRole: string = "member";
      let devScope: string = "read-write";
      let devTokenPrefix: string = "pat";

      if (parsePat(devToken) === null) {
        const devClerkAuth = await verifyClerkSessionAndResolveTenant(request, env, requestId);
        if (!devClerkAuth.ok) {
          return applyCors(devClerkAuth.response, request);
        }
        devTenantId = devClerkAuth.tenantId;
        devRole = devClerkAuth.role;
        devScope = devRole === "viewer" ? "read-only" : "read-write";
        devTokenPrefix = "clerk";
      } else {
        const auth = await extractAuth(request, env);
        if (!auth.ok) {
          return applyCors(reapiError("UNAUTHORIZED", "Invalid PAT", 401, requestId), request);
        }
        devTenantId = auth.tenantId;
        devScope = auth.scope || "read-write";
        devRole = "member";
        devTokenPrefix = auth.tokenPrefix;
      }

      if (!env.RUNNER_DEVENV_DO) {
        return applyCors(
          reapiError("SERVICE_UNAVAILABLE", "RUNNER_DEVENV_DO binding not configured", 503, requestId),
          request,
        );
      }

      if (!mayAccessDevenv(request, devScope)) return applyCors(reapiError("FORBIDDEN", "DevEnv scope required", 403, requestId), request);
      const isStart = isDevenvStart(request.method, route.pathSuffix);
      if (isStart) {
        try {
          const { checkDevenvQuota } = await import("./lib/devenv_guard.js");
          const quota = await bounded(checkDevenvQuota(env, devTenantId));
          if (!quota.allowed) return applyCors(reapiError("QUOTA_EXCEEDED", "DevEnv quota exceeded", 403, requestId), request);
        } catch { return applyCors(reapiError("SERVICE_UNAVAILABLE", "DevEnv admission unavailable", 503, requestId), request); }
      }
      let devStub: DurableObjectStub<RunnerDevEnvRpc>;
      try { devStub = env.RUNNER_DEVENV_DO.get(env.RUNNER_DEVENV_DO.idFromName(devTenantId)) as DurableObjectStub<RunnerDevEnvRpc>; }
      catch { return applyCors(reapiError("SERVICE_UNAVAILABLE", "DevEnv unavailable", 503, requestId), request); }
      if (isStart) {
        let lifecycleGeneration: string;
        try { lifecycleGeneration = (await readCredentialLifecycle(env, devTenantId)).generation; }
        catch { return applyCors(reapiError("SERVICE_UNAVAILABLE", "DevEnv issuer unavailable", 503, requestId), request); }
        const mintKey = env.CORELINK_PAT_MINT_AUTH_KEY, revokeKey = env.CORELINK_RUNNER_MINT_AUTH_KEY;
        if (!mintKey || !revokeKey) return applyCors(reapiError("SERVICE_UNAVAILABLE", "DevEnv issuer unavailable", 503, requestId), request);
        const principalSource = devTokenPrefix === "clerk" ? `devenv-clerk:${devTenantId}` : `devenv-pat:${devTenantId}`;
        const result = await relayAuthorizedDevenvStart(request, devTenantId, {
          lifecycleGeneration,
          prepareCompute: (sessionUuid, tier) => prepareDevenvCompute(env, devStub, devTenantId, sessionUuid, Date.now()),
          abandonCompute: reservationId => devStub.abandonAuthorizedCompute(reservationId),
          prepare: async (operationId, tenantId, generation) => {
            const system = env.CORELINK_SERVER.get(env.CORELINK_SERVER.idFromName("_system"));
            const response = await bounded(system.fetch(new Request("https://do/_do/devenv-cleanup/prepare", { method: "POST", headers: { "content-type": "application/json", "x-corelink-internal-auth": revokeKey }, body: JSON.stringify({ operationId, tenantId, lifecycleGeneration: generation }) })));
            return response.status === 204;
          },
          mint: operationId => mintScopedPat(env, requestId, MintGrant.fromDevenvSession(devTenantId, principalSource, lifecycleGeneration), DEVENV_MAX_TTL_SECONDS, "cas:rw", mintKey, undefined, undefined, undefined, { operationId, tenantId: devTenantId, principalSource, lifecycleGeneration }),
          start: input => devStub.startAuthorizedDevenv(input),
          revoke: operationId => revokeDevenvOperation(env.CONFIG_DB, env.METADATA_KV, operationId, devTenantId),
          adopt: (operationId, patId) => adoptDevenvOperation(env.CONFIG_DB, operationId, devTenantId, patId),
          now: Date.now, sessionId: () => crypto.randomUUID(), cleanupFailed: () => console.error("devenv_start_revoke_failed"),
        });
        return applyCors(result, request);
      }

      const devAugmented = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);
          h.delete("authorization");
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "devenv_v1");
          h.set("x-corelink-token-prefix", devTokenPrefix);
          h.set("x-corelink-tenant-id", devTenantId);
          h.set("x-corelink-scope", devScope);
          h.set("x-corelink-role", devRole);
          return h;
        })(),
      });

      let devResp: Response;
      try {
        devResp = await devStub.fetch(devAugmented);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        return applyCors(
          reapiError("INTERNAL_ERROR", `devenv upstream error: ${message}`, 500, requestId),
          request,
        );
      }

      return applyCors(devResp, request);
    }

    // Customer portal dual-auth dispatch (dashboard revival WP-1) —
    // /v1/customer/* accepts EITHER a CoreLink PAT (existing path, byte-identical
    // — handled by the generic PAT gate below) OR a Clerk session JWT (the
    // browser dashboard holds a Clerk session, not a PAT).
    //
    // Dispatch guard: parsePat() accepts ONLY the canonical
    // `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` shape — a Clerk JWT
    // (or any non-PAT bearer) can NEVER parse as one, so any parseable PAT
    // (including expired/revoked ones) falls through to the PAT gate exactly as
    // before; the PAT surface is untouched. The token is derived the same way
    // extractAuth derives it (slice "Bearer " + trim) so the dispatch decision
    // and the PAT gate's parse can never disagree.
    //
    // Clerk arm: verify via the SHARED pipeline (lib/clerk_auth.ts — same M1
    // azp re-assert + M2 issuer pin + tenant lookup as onboarding), then
    // forward to the PER-TENANT DO. Mirrors the onboarding forward but
    // deliberately WITHOUT x-corelink-internal-auth — customer routes resolve
    // the tenant from the server-trust x-corelink-tenant-id header and do not
    // need the internal-auth key (least privilege: the dashboard surface must
    // not carry the operator-grade credential).
    //
    // Storage-quota gate is BYPASSED on this arm (deliberate): an over-quota
    // tenant must still see the dashboard to upgrade — same posture as
    // onboarding (which also never passes through the quota gate).
    if (route.routeKind === "customer_v1") {
      const custAuthz = request.headers.get("authorization") ?? "";
      const custToken = custAuthz.startsWith("Bearer ")
        ? custAuthz.slice("Bearer ".length).trim()
        : "";
      if (parsePat(custToken) === null) {
        const custClerkAuth = await verifyClerkSessionAndResolveTenant(request, env, requestId);
        if (!custClerkAuth.ok) {
          return applyCors(custClerkAuth.response, request);
        }
        const custTenantId = custClerkAuth.tenantId;

        // B-073 acceptance is an edge-terminated operation. It deliberately
        // does not forward to a tenant DO: the target tenant comes only from
        // the token-bound invitation row, never from a client header/body.
        if (route.pathSuffix === "/v1/customer/team/accept") {
          if (request.method !== "POST") {
            return applyCors(reapiError("METHOD_NOT_ALLOWED", "invitation acceptance requires POST", 405, requestId), request);
          }
          // Signup-worker and the container share EMAIL_HASH_SALT. A
          // production acceptance without it would compute a different (or
          // reversible) pseudonym, so fail closed before consuming the token.
          if (isProductionEnvironment(env) && !env.EMAIL_HASH_SALT?.trim()) {
            return applyCors(reapiError("SERVICE_UNAVAILABLE", "invitation service unavailable", 503, requestId), request);
          }
          const contentLength = Number(request.headers.get("content-length") ?? "0");
          if (contentLength > 4096) {
            return applyCors(reapiError("BAD_REQUEST", "invitation body too large", 400, requestId), request);
          }
          let body: unknown;
          try {
            body = await request.json();
          } catch {
            return applyCors(reapiError("BAD_REQUEST", "invalid invitation body", 400, requestId), request);
          }
          if (!body || typeof body !== "object" || Array.isArray(body)) {
            return applyCors(reapiError("BAD_REQUEST", "invitation token required", 400, requestId), request);
          }
          const keys = Object.keys(body);
          const token = (body as Record<string, unknown>)["invitation_token"];
          if (keys.length !== 1 || keys[0] !== "invitation_token" || typeof token !== "string" || token.length > 128) {
            return applyCors(reapiError("BAD_REQUEST", "invitation token required", 400, requestId), request);
          }
          if (!env.CONFIG_DB) {
            return applyCors(reapiError("SERVICE_UNAVAILABLE", "invitation service unavailable", 503, requestId), request);
          }
          const email = await verifiedPrimaryEmail(custClerkAuth.clerkUserId, env.CLERK_SECRET_KEY);
          if (!email) {
            return applyCors(reapiError("FORBIDDEN", "verified primary email required", 403, requestId), request);
          }
          try {
            const accepted = await redeemTeamInvitation(env.CONFIG_DB, {
              clerkUserId: custClerkAuth.clerkUserId,
              invitationToken: token,
              emailHashCandidates: await emailHashCandidates(email, env.EMAIL_HASH_SALT),
              nowMs: Date.now(),
            });
            if (!accepted) {
              return applyCors(reapiError("FORBIDDEN", "invalid, expired, or already redeemed invitation", 403, requestId), request);
            }
            return applyCors(Response.json({ ok: true, tenant_id: accepted.tenantId, role: accepted.role }), request);
          } catch (error: unknown) {
            console.error(`[${requestId}] team invitation acceptance failed: ${error instanceof Error ? error.message.slice(0, 80) : "unknown"}`);
            return applyCors(reapiError("INTERNAL_ERROR", "invitation acceptance unavailable", 500, requestId), request);
          }
        }

        // Forward to the tenant's DO. CRITICAL: strip ALL client-supplied trust
        // headers FIRST (the browser must never spoof internal-auth or the
        // tenant id), then set them from server-trusted values. Drop the Clerk
        // JWT — the container trusts the Worker-set x-corelink-tenant-id, and
        // the session token must not travel further than the edge.
        const custDoId = env.CORELINK_SERVER.idFromName(custTenantId);
        const custStub = env.CORELINK_SERVER.get(custDoId, serverGetOpts(env));
        const custAugmented = new Request(request, {
          headers: (() => {
            const h = new Headers(request.headers);
            // Strip the FULL set of client-suppliable trust headers (x-admin-*,
            // fanout-from, scope, tenant-id, internal-auth) BEFORE re-establishing
            // them from server-trusted values (delete-then-set).
            stripClientTrustHeaders(h);
            h.delete("authorization");
            h.set("x-request-id", requestId);
            h.set("x-corelink-route-kind", "customer_v1");
            h.set("x-corelink-token-prefix", "clerk");
            h.set("x-corelink-tenant-id", custTenantId);
            // RBAC scope (team_member role 0074), sole setter:
            //   viewer        → read-only  (no write, no billing)
            //   member        → read-write (cache write; NO `billing`)
            //   owner / admin → read-write billing (H17: the billing capability a
            //                   cache PAT never carries — only an owner/admin
            //                   dashboard human clears the F-018 billing/PII gate)
            // The billing carve-out is now OWNER/ADMIN-only: a plain `member` must
            // not open the billing portal / cancel the subscription / read financial
            // PII (RBAC hardening — this is the sole place `billing` is granted).
            // Team-management ops (invite/remove) gate on the `x-corelink-role`
            // header separately in the container.
            h.set(
              "x-corelink-scope",
              custClerkAuth.role === "viewer"
                ? "read-only"
                : custClerkAuth.role === "owner" || custClerkAuth.role === "admin"
                  ? "read-write billing"
                  : "read-write",
            );
            // Team RBAC role (0074): forward the D1-resolved role so the container
            // can gate OWNER-only operations (account deletion erases the WHOLE
            // tenant — a non-owner seat must not trigger it). Sole setter; the
            // client copy was stripped above. `x-corelink-scope` only distinguishes
            // viewer vs the rest, so it cannot express "is owner" — the role does.
            h.set("x-corelink-role", custClerkAuth.role);
            // DSR portal (/v1/privacy/*) destructive-arm MFA step-up: the Worker
            // is the SOLE setter of x-corelink-mfa-verified (stripped above). The
            // container gate (routes/dsr/portal.rs) is fail-CLOSED on this trusted
            // marker. We stamp it ONLY when the Clerk session's factor-verification
            // age is FRESH — `fvaMinutes != null && fvaMinutes <= threshold` — so a
            // stolen/XSS/CSRF long-lived dashboard session can NOT trigger
            // irreversible cross-region erasure with no re-auth. `undefined`
            // (absent/malformed `fva`) ⇒ NOT fresh (fail-CLOSED, never 0), the same
            // freshness signal the session/token-exchange paths forward
            // (lib/session_exchange.ts). When NOT fresh we leave the marker unset:
            // the container gate then fails closed and records the DSR ticket
            // `pending` with `mfa_required=true` (the caller completes step-up via
            // POST /v1/privacy/dsr/{id}/verify-mfa) — matching existing missing-marker
            // behaviour. Only ever stamped for the privacy plane.
            if (
              route.pathSuffix.startsWith("/v1/privacy/") &&
              custClerkAuth.fvaMinutes !== undefined &&
              custClerkAuth.fvaMinutes <= MFA_FVA_FRESH_MAX_MINUTES
            ) {
              h.set("x-corelink-mfa-verified", "1");
            }
            // Deliberately NOT set: x-corelink-internal-auth (least privilege —
            // customer routes don't need the operator-grade credential).
            return h;
          })(),
        });
        let custResp: Response;
        try {
          custResp = await custStub.fetch(custAugmented);
        } catch (err: unknown) {
          const message = err instanceof Error ? err.message : "unknown error";
          console.error(`[${requestId}] customer clerk DO fetch failed: ${message.slice(0, 80)}`);
          return applyCors(
            reapiError("INTERNAL_ERROR", "customer upstream error", 500, requestId),
            request,
          );
        }
        const custHeaders = new Headers(custResp.headers);
        if (!custHeaders.has("x-request-id")) {
          custHeaders.set("x-request-id", requestId);
        }
        return applyCors(
          new Response(custResp.body, {
            status: custResp.status,
            statusText: custResp.statusText,
            headers: custHeaders,
          }),
          request,
        );
      }
      // else: the bearer parses as a canonical PAT — fall through to the
      // generic PAT gate below (byte-identical to the pre-WP-1 behavior).
    }

  return null;
}
