/** Clerk, exchange, runner, OCI, and customer route domain. */
import type { ExecutionContext } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import { applyCors, applyTimingPad, extractAuth, stripClientTrustHeaders } from "./index_auth.js";
import { type AuthResult, parsePat, MFA_FVA_FRESH_MAX_MINUTES } from "./index_auth.js";
import type { RouteMatch } from "./route_match.js";
import { resolveOnboardingAuthKey } from "./index_auth.js";
import { reapiError, serverGetOpts } from "./index_common.js";
import { getServerNonce } from "./index_common.js";
import { constantTimeSecretEqual } from "./lib/internal_auth.js";
import { verifyClerkSessionAndResolveTenant } from "./lib/clerk_auth.js";
import { emailHashCandidates, redeemTeamInvitation, verifiedPrimaryEmail } from "./lib/team_invitation.js";
import { handleSessionExchange, handleTokenExchange } from "./lib/session_exchange.js";
import { handleRunnerMint, handleRunnerRevoke } from "./lib/runner_mint.js";
import { handleAuthRotate } from "./lib/auth_rotate.js";
import { handleTenantLookup } from "./lib/tenant_lookup.js";
import { resolveConsumerKey } from "./lib/internal_auth.js";
import { coloForMacro } from "./region-map.js";
import { resolveTenantResidency, RESIDENCY_UNRESOLVED } from "./lib/tenant_residency_cache.js";
import type { KvReader } from "./lib/pat_verify_cache.js";

export async function handleSpecialRoute(
  request: Request,
  env: Env,
  ctx: ExecutionContext,
  requestId: string,
  requestStart: number,
  requestCounter: number,
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

    // Session→token exchange — POST /v1/session/exchange (hugit-P2 WP-C, seam C).
    // The caller presents a Clerk SESSION JWT; the edge verifies it (shared
    // pipeline) and exchanges it for a short-lived tenant-scoped CoreLink PAT,
    // REUSING the container's audited /_internal/pat/mint via the _system DO.
    // The handler is fully fail-CLOSED (missing secret → 403, bad/expired
    // session → 401, no tenant → 403, upstream fault → 500) and never forwards
    // the session token past the edge. CORS is applied here, mirroring the
    // onboarding/customer arms.
    if (route.routeKind === "session_exchange") {
      const sessResp = await handleSessionExchange(request, env, requestId);
      return applyCors(sessResp, request);
    }

    // githugr authz #3 — tenant lookup (POST /internal/v1/auth/tenant/lookup).
    // Internal-auth gated; parameterized D1 read; fail-CLOSED 404 when no tenant
    // maps to the subject. CORS applied here, mirroring the arms above.
    if (route.routeKind === "tenant_lookup") {
      const lookupResp = await handleTenantLookup(request, env, requestId);
      return applyCors(lookupResp, request);
    }

    // githugr authz #1 — token exchange (POST /internal/v1/auth/token-exchange).
    // Internal-auth + Clerk-session gated; 403 on session.tenant ≠ audience (the
    // cross-tenant-write rejection); mints a ~300s tenant-scoped PAT via the
    // container. Fully fail-CLOSED; never forwards the session token past the edge.
    if (route.routeKind === "token_exchange") {
      const xchgResp = await handleTokenExchange(request, env, requestId);
      return applyCors(xchgResp, request);
    }

    // corelink-runners D-9 — runner PAT mint (POST /internal/v1/runner/mint).
    // Internal-auth gated (pat_mint consumer key + shared fallback) + runners-
    // entitlement checked; mints a job-bounded tenant-scoped PAT via the
    // container's single mint authority. Handled AT the Worker: the handler
    // builds a FRESH server-trusted request to the _system DO (it never forwards
    // the inbound request), so client trust headers can never reach the mint
    // route — the same posture as token-exchange. CORS applied here.
    if (route.routeKind === "runner_mint") {
      const mintResp = await handleRunnerMint(request, env, requestId);
      return applyCors(mintResp, request);
    }

    // corelink-runners D-9 — runner PAT revoke (POST /internal/v1/runner/revoke).
    // Internal-auth gated; revokes by pat_id via the existing
    // `UPDATE pat SET revoked_at_ms` surface (INV-PAT-REVOKE-PROPAGATION). The
    // handler touches CONFIG_DB directly — no inbound headers are forwarded. CORS
    // applied here.
    if (route.routeKind === "runner_revoke") {
      const revokeResp = await handleRunnerRevoke(request, env, requestId);
      return applyCors(revokeResp, request);
    }

    // clw `auth rotate` (POST /internal/v1/auth/rotate). Internal-auth gated
    // (pat_mint consumer key + shared fallback); reads the old PAT's tenant +
    // scope, mints an equivalent new PAT via the container's single mint authority
    // (FRESH server-trusted request to the _system DO — no inbound headers
    // forwarded), then revokes the old pat_id ONLY after the mint succeeds (no
    // zero-valid-PAT window). Fully fail-CLOSED. CORS applied here.
    if (route.routeKind === "auth_rotate") {
      const rotateResp = await handleAuthRotate(request, env, requestId);
      return applyCors(rotateResp, request);
    }

    // Not found — timing-padded to prevent cross-tenant enumeration
    if (route.routeKind === "not_found") {
      await applyTimingPad(
        requestStart,
        requestId,
        requestCounter,
        getServerNonce(),
      );
      const resp = reapiError("NOT_FOUND", "The requested resource does not exist.", 404, requestId);
      return applyCors(resp, request);
    }

    // OCI two-leg auth pass-through — /v2/* and /token.
    // OCI does its OWN auth in the container (Option-B PAT verify at /token,
    // HMAC Bearer on /v2). The Worker does not authorize OCI or inject a tenant;
    // it uses the bearer tenant only to select the jurisdictional service
    // binding, while the container remains the auth/data-plane authority.
    if (route.routeKind === "oci_v2" || route.routeKind === "oci_token") {
      // OCI has no tenant path segment, but a minted bearer carries the
      // canonical tenant UUID. Use that signed-token-shaped value only as a
      // residency ROUTING HINT; the regional container remains the sole HMAC
      // authorization boundary. This closes the old shared-IAD `_oci` path,
      // which stored EU/APAC image layers in whichever region first created the
      // shared DO. `/token` has only a PAT and therefore remains on the shared
      // leg; the subsequent bearer request performs the regional fan-out.
      const ociTenant = ociRoutingTenantId(request);
      const ociFanoutHeader = request.headers.get("x-corelink-fanout-from");
      const ociIsFanout =
        ociFanoutHeader !== null &&
        typeof env.CORELINK_INTERNAL_AUTH_KEY === "string" &&
        env.CORELINK_INTERNAL_AUTH_KEY.length > 0 &&
        constantTimeSecretEqual(env.CORELINK_INTERNAL_AUTH_KEY, ociFanoutHeader);
      if (!ociIsFanout && ociTenant !== undefined) {
        const residencyKv = (env as unknown as { METADATA_KV?: KvReader }).METADATA_KV;
        const ociResidency = await resolveTenantResidency(env.CONFIG_DB, ociTenant, {
          ...(residencyKv ? { kv: residencyKv } : {}),
          waitUntil: ctx.waitUntil.bind(ctx),
        });
        if (ociResidency === RESIDENCY_UNRESOLVED) {
          return applyCors(
            new Response(
              JSON.stringify({
                error: "RESIDENCY_UNAVAILABLE",
                message: "could not resolve OCI tenant data-residency region",
                request_id: requestId,
              }),
              {
                status: 503,
                headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
              },
            ),
            request,
          );
        }
        const primaryRegion = ociResidency.region ?? undefined;
        const colo = primaryRegion !== undefined ? coloForMacro(primaryRegion) : undefined;
        if (primaryRegion !== undefined && colo !== "iad") {
          let regionalBinding: { fetch: typeof fetch } | undefined;
          if (colo === "lhr") regionalBinding = env.PROD_LHR;
          else if (colo === "nrt") regionalBinding = env.PROD_NRT;
          else if (colo === "sam") regionalBinding = env.PROD_SAM;
          if (regionalBinding === undefined) {
            return applyCors(
              new Response(
                JSON.stringify({
                  error: "RESIDENCY_UNAVAILABLE",
                  message: `OCI data-residency region '${primaryRegion}' is not currently servable`,
                  request_id: requestId,
                }),
                {
                  status: 503,
                  headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
                },
              ),
              request,
            );
          }
          const regionalReq = new Request(request, {
            headers: (() => {
              const h = new Headers(request.headers);
              stripClientTrustHeaders(h);
              h.set("x-request-id", requestId);
              h.set("x-corelink-route-kind", route.routeKind);
              h.set("x-corelink-primary-region", primaryRegion);
              // Accepted by the next Worker only over the service binding and
              // only when it equals the shared internal secret.
              if (env.CORELINK_INTERNAL_AUTH_KEY) {
                h.set("x-corelink-fanout-from", env.CORELINK_INTERNAL_AUTH_KEY);
              }
              h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
              return h;
            })(),
          });
          const regionalResp = await regionalBinding.fetch(regionalReq);
          return applyCors(regionalResp, request);
        }
      }
      const ociDoId = env.CORELINK_SERVER.idFromName("_oci");
      const ociStub = env.CORELINK_SERVER.get(ociDoId, serverGetOpts(env));
      const ociReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);          // delete any client-forged x-corelink-*
          // Belt-and-braces: x-corelink-tenant-id is now in the strip list so
          // the delete above already removed any client value. The explicit
          // delete below is kept as belt-and-braces documentation that the OCI
          // pass-through deliberately never sets a tenant-id header.
          h.delete("x-corelink-tenant-id");
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", route.routeKind);
          // F-016 (OCI DoS fairness): the OCI plane has no edge-resolved tenant,
          // so the container's per-request velocity gate falls back to a per-repo
          // key — but to also bound abuse per-SOURCE it needs the UNFORGEABLE
          // client IP. Forward cf-connecting-ip as the server-trusted
          // x-corelink-client-ip (stripClientTrustHeaders above already deleted
          // any client-supplied value, so a client cannot spoof it). This header
          // IS the per-IP enforcement input: the only per-source cap on the OCI
          // plane is the in-container rate-limit bucket PARTITION keyed on it
          // (see `routes::ratelimit_layer`), shipped 2026-08-04. There is no
          // This is an application control, not a claim about dashboard WAF
          // inventory: the container owns the bounded deny decision, while
          // the IP header only partitions the unauthenticated source bucket.
          // An edge per-IP/ASN policy may provide an additional outer layer;
          // it is not the data plane's sole request-rate defense.
          h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
          // Deliberately NOT set: x-corelink-tenant-id / x-corelink-scope.
          // The OCI adapter derives the tenant from the OCI Bearer/PAT and
          // enforces per-op scope from its own HMAC bearer token. The raw
          // Authorization header (OCI Basic at /token, OCI Bearer at /v2) is
          // preserved by the `new Headers(request.headers)` clone above —
          // stripClientTrustHeaders does NOT remove Authorization.
          return h;
        })(),
      });
      let ociResp: Response;
      try {
        ociResp = await ociStub.fetch(ociReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        // Do NOT include error detail that could leak internal topology
        // (parity with the PAT-path DO forward catch below).
        console.error(`[${requestId}] OCI DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(ociResp, request);
    }

    // Stripe billing webhook pass-through — POST /v1/billing/stripe-webhook.
    // Mirrors the OCI carve-out: Stripe authenticates with a `Stripe-Signature`
    // HMAC header (NOT a Bearer PAT), so the Worker is a pure forwarder here —
    // no extractAuth, no tenant/scope injection. The container is the SOLE
    // authority: it re-computes the Stripe HMAC over the EXACT raw body bytes
    // (constant-time, replay-windowed) and derives the tenant from the signed
    // event metadata. Route to the shared "_system" DO (the webhook has no URL
    // tenant). CRITICAL CORRECTNESS: forward the body UNCHANGED — `new Request(
    // request, { headers })` preserves the body stream unread, so the bytes the
    // container hashes are byte-identical to what Stripe signed. We do NOT
    // read/clone/parse the body (a re-serialized body would break the signature).
    if (route.routeKind === "billing_webhook") {
      const billingDoId = env.CORELINK_SERVER.idFromName("_system");
      const billingStub = env.CORELINK_SERVER.get(billingDoId, serverGetOpts(env));
      const billingReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          // Strip any client-forged x-corelink-* server-trust headers BEFORE we
          // set our own (delete-then-set). stripClientTrustHeaders does NOT
          // remove `stripe-signature` (the webhook's auth) nor `authorization`
          // — both are preserved by the `new Headers(request.headers)` clone.
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", route.routeKind);
          // Belt-and-braces: the container derives the tenant SOLELY from the
          // signed Stripe event, never from a header. x-corelink-tenant-id is
          // now in the strip list so the delete above already removed any client
          // value; the explicit delete below documents that the billing-webhook
          // path deliberately never sets a tenant-id header.
          h.delete("x-corelink-tenant-id");
          h.delete("x-corelink-scope");
          // Deliberately NOT set: x-corelink-tenant-id / x-corelink-scope.
          return h;
        })(),
      });
      let billingResp: Response;
      try {
        billingResp = await billingStub.fetch(billingReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        // Do NOT include error detail that could leak internal topology
        // (parity with the OCI / PAT-path DO forward catches).
        console.error(`[${requestId}] billing webhook DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(billingResp, request);
    }

    // PUBLIC erasure-attestation verifier — /v1/public/* (Artifact 1, WP-C1).
    // Pure pass-through to the _anonymous DO → container, which is the SOLE
    // authority. These are UNAUTHENTICATED GETs (an erasure proof is publicly
    // verifiable) — NO PAT gate, NO internal-auth (mirrors the billing-webhook
    // carve-out, minus any signature). Client-forged x-corelink-* trust headers
    // are stripped; the container routes are mounted outside its auth layers.
    if (route.routeKind === "public_attestation") {
      const pubDoId = env.CORELINK_SERVER.idFromName("_anonymous");
      const pubStub = env.CORELINK_SERVER.get(pubDoId, serverGetOpts(env));
      const pubReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "public_attestation");
          h.set("x-corelink-tenant-id", "_anonymous");
          // M22(a): this arm did NOT set the trusted client-IP header (unlike
          // the cache/OCI/signup arms), so the container's scoped per-IP
          // rate limiter on `/v1/public/*` had nothing to key on. Forward
          // CF's unforgeable client IP as x-corelink-client-ip
          // (stripClientTrustHeaders above already deleted any
          // client-supplied value, so a client cannot spoof it).
          h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
          return h;
        })(),
      });
      let pubResp: Response;
      try {
        pubResp = await pubStub.fetch(pubReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] public attestation DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(pubResp, request);
    }

    // corelink-runners fabric introspect — pure pass-through to the _system DO →
    // container, which is the SOLE auth authority (FABRIC_INTROSPECT_AUTH_KEY).
    // The Worker forwards the caller's x-corelink-internal-auth (the FABRIC secret)
    // UNCHANGED and applies NO edge gate (mirrors the billing-webhook carve-out,
    // where the container verifies the Stripe signature). See the matchRoute note.
    if (route.routeKind === "fabric_introspect") {
      const fbDoId = env.CORELINK_SERVER.idFromName("_system");
      const fbStub = env.CORELINK_SERVER.get(fbDoId, serverGetOpts(env));
      // Capture the FABRIC secret BEFORE stripping client trust headers.
      const fabricAuth = request.headers.get("x-corelink-internal-auth") ?? "";
      const fbReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "fabric_introspect");
          h.set("x-corelink-tenant-id", "_system");
          // Re-forward the caller's FABRIC secret unchanged — the container's
          // introspect gate is the sole authority; the Worker never inspects it.
          h.set("x-corelink-internal-auth", fabricAuth);
          return h;
        })(),
      });
      let fbResp: Response;
      try {
        fbResp = await fbStub.fetch(fbReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] fabric introspect DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(fbResp, request);
    }

    // corelink-runners billing usage-push ingest — pure pass-through to the
    // _system DO → container, which is the SOLE auth authority
    // (BILLING_INGEST_AUTH_KEY). The Worker forwards the caller's
    // x-corelink-internal-auth (the ingest secret) UNCHANGED and applies NO edge
    // gate (mirrors the fabric_introspect / billing-webhook carve-outs). See the
    // matchRoute note.
    if (route.routeKind === "billing_ingest") {
      const biDoId = env.CORELINK_SERVER.idFromName("_system");
      const biStub = env.CORELINK_SERVER.get(biDoId, serverGetOpts(env));
      // Capture the INGEST secret BEFORE stripping client trust headers.
      const ingestAuth = request.headers.get("x-corelink-internal-auth") ?? "";
      const biReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "billing_ingest");
          h.set("x-corelink-tenant-id", "_system");
          // Re-forward the caller's INGEST secret unchanged — the container's
          // ingest gate is the sole authority; the Worker never inspects it.
          h.set("x-corelink-internal-auth", ingestAuth);
          return h;
        })(),
      });
      let biResp: Response;
      try {
        biResp = await biStub.fetch(biReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] billing ingest DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(biResp, request);
    }

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

      // Check quota
      const { checkDevenvQuota } = await import("./lib/devenv_guard.js");
      const quota = await checkDevenvQuota(env, devTenantId);
      if (!quota.allowed) {
        return applyCors(
          reapiError("QUOTA_EXCEEDED", quota.reason ?? "DevEnv quota exceeded", 403, requestId),
          request,
        );
      }

      if (!env.RUNNER_DEVENV_DO) {
        return applyCors(
          reapiError("SERVICE_UNAVAILABLE", "RUNNER_DEVENV_DO binding not configured", 503, requestId),
          request,
        );
      }

      const devDoId = env.RUNNER_DEVENV_DO.idFromName(devTenantId);
      const devStub = env.RUNNER_DEVENV_DO.get(devDoId);

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
          if (env.ENVIRONMENT === "prod" && !env.EMAIL_HASH_SALT?.trim()) {
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
          const token = (body as Record<string, unknown>).invitation_token;
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

    // Auth gate — all other routes require a valid Bearer PAT, EXCEPT signup
    // which is pre-tenant: the :token in /v1/signup/pilot/:token IS the auth
    // artifact, not a Bearer PAT. The container validates the path token against
    // the signup-tokens store.
    //
    // For everything else: extractAuth performs parse format → HMAC fast-fail
    // (if PAT_SIGNING_KEY bound) → D1 existence + expiry check → resolve
    // tenant_id. Any token not in the D1 store → 401 (WP-A1 P0-2 fix).
    type AuthOk = Extract<AuthResult, { ok: true }>;

    return null;
}
