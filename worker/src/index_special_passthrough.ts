/** OCI, billing, fabric, ingest, and DevEnv special routes. */
import type { ExecutionContext } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import { applyCors, extractAuth, stripClientTrustHeaders } from "./index_auth.js";
import type { RouteMatch } from "./route_match.js";
import { ociRoutingTenantId, reapiError, serverGetOpts } from "./index_common.js";
import { constantTimeSecretEqual } from "./lib/internal_auth.js";
import { coloForMacro } from "./region-map.js";
import { resolveTenantResidency, RESIDENCY_UNRESOLVED } from "./lib/tenant_residency_cache.js";
import type { KvReader } from "./lib/pat_verify_cache.js";

export async function handleSpecialPassThroughRoute(
  request: Request,
  env: Env,
  ctx: ExecutionContext,
  requestId: string,
  route: RouteMatch,
): Promise<Response | null> {
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

  return null;
}
