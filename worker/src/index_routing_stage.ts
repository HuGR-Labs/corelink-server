/** Residency routing and trusted container request construction. */
import type { DurableObjectStub, ExecutionContext } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import { stripClientTrustHeaders } from "./index_auth.js";
import type { AuthStageResult } from "./index_auth_stage.js";
import type { RouteMatch } from "./route_match.js";
import { serverGetOpts, reapiError } from "./index_common.js";
import { constantTimeSecretEqual } from "./lib/internal_auth.js";
import { resolveTenantResidency, RESIDENCY_UNRESOLVED } from "./lib/tenant_residency_cache.js";
import { type KvReader } from "./lib/pat_verify_cache.js";
import { coloForMacro } from "./region-map.js";
import { applyCors } from "./index_auth.js";
import { STORAGE_QUOTA_HEADER } from "./lib/quota.js";
import { isPilotSignupPath } from "./route_match.js";

export interface RoutingStageResult {
  readonly primaryRegion: string | undefined;
  readonly stub: DurableObjectStub;
  readonly augmented: Request;
  readonly findMissingBody: string | null;
  readonly findMissingShadowBody: string | null;
  readonly stQResidMs: number;
}

export async function routeTenantRequest(
  request: Request,
  env: Env,
  ctx: ExecutionContext,
  requestId: string,
  route: RouteMatch,
  authStage: AuthStageResult,
  storageQuotaHeader: string | null,
): Promise<RoutingStageResult | Response> {
  const { auth, resolvedTenantId } = authStage;
  let stQResidMs = -1;
    const regionFanoutHeader = request.headers.get("x-corelink-fanout-from");
    const isFanout =
      regionFanoutHeader !== null &&
      typeof env.CORELINK_INTERNAL_AUTH_KEY === "string" &&
      env.CORELINK_INTERNAL_AUTH_KEY.length > 0 &&
      constantTimeSecretEqual(env.CORELINK_INTERNAL_AUTH_KEY, regionFanoutHeader);

    // F-015 (overnight red-team): primaryRegion is hoisted OUT of the block so the
    // LOCAL DO path below can also stamp x-corelink-primary-region (not just the
    // fan-out path). Without that stamp, a client hitting a public regional host
    // (e.g. lhr.corelink-api.humangr.com) directly with a non-matching tenant
    // reaches that region's container with NO residency header → the container
    // guard's absent→Allow → cross-region placement. Stamping it lets the
    // container backstop (residency.rs) reject the mismatch.
    let primaryRegion: string | undefined;
    if (
      resolvedTenantId !== "_anonymous" &&
      resolvedTenantId !== "_system" &&
      resolvedTenantId !== "_pending"
    ) {
      // Residency is resolved through the three-tier cache (L1 isolate → L2 KV
      // `tres:` → L3 D1) so a far-from-D1 (e.g. SAM) caller no longer pays a
      // synchronous D1-PRIMARY round-trip per request for this near-immutable
      // value (latency WP — the `wdb` phase). FAIL-CLOSED is preserved: an
      // unresolved region (D1 fault with no cached fallback) still 503s rather
      // than IAD-leaking; a stale cached region cannot SILENTLY leak because the
      // container residency backstop (residency.rs) 409s any real cross-region
      // mismatch.
      const residencyKv = (env as unknown as { METADATA_KV?: KvReader }).METADATA_KV;
      const residStart = Date.now();
      const residency = await resolveTenantResidency(env.CONFIG_DB, resolvedTenantId, {
        ...(residencyKv ? { kv: residencyKv } : {}),
        waitUntil: ctx.waitUntil.bind(ctx),
      });
      stQResidMs = Date.now() - residStart;
      if (residency === RESIDENCY_UNRESOLVED) {
        // D1 hiccup with no cached region: we cannot establish residency.
        // FAIL-CLOSED — refuse rather than risk routing an EU tenant to US
        // storage on a transient error.
        return applyCors(
          new Response(
            JSON.stringify({
              error: "RESIDENCY_UNAVAILABLE",
              message: "could not resolve tenant data-residency region",
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
      // A `null` region (no tenant row / no pin) ⇒ `undefined`, preserving the
      // existing `primaryRegion === undefined` IAD-local fall-through below.
      primaryRegion = residency.region ?? undefined;

      const colo = primaryRegion !== undefined ? coloForMacro(primaryRegion) : undefined;
      // Non-IAD residency: must fan-out to the matching regional Service Binding.
      // colo === undefined here means an unknown/unprovisioned macro (e.g. afr) —
      // also fail-closed (never serve such a tenant from IAD).
      // F-014 (overnight red-team): gate the fan-out on !isFanout. A request that
      // ALREADY arrived as a fan-out (isFanout — running on a regional Worker)
      // must NOT re-route: the regional envs lack the PROD_* service bindings, so
      // re-entering this branch would hit `regionalBinding === undefined` → 503 for
      // every non-IAD tenant. A fanned-out request falls through to the local DO
      // path on the regional Worker (which serves its own region) instead.
      if (!isFanout && primaryRegion !== undefined && colo !== "iad") {
        let regionalBinding: { fetch: typeof fetch } | undefined;
        if (colo === "lhr") regionalBinding = env.PROD_LHR;
        else if (colo === "sam") regionalBinding = env.PROD_SAM;
        else if (colo === "nrt") regionalBinding = env.PROD_NRT;

        if (regionalBinding === undefined) {
          // FAIL-CLOSED: non-IAD residency but the regional binding is missing
          // (or the macro is unprovisioned/unknown). Refuse — do NOT fall
          // through to IAD, which would store the tenant's data cross-border.
          return applyCors(
            new Response(
              JSON.stringify({
                error: "RESIDENCY_UNAVAILABLE",
                message: `data-residency region '${primaryRegion}' is not currently servable`,
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

        // Forward verbatim to the regional Worker. The regional Worker re-runs
        // PAT validation (PAT_SIGNING_KEY is shared across envs) + writes to
        // its regional R2 bucket. Service Binding bypasses CF edge error 1014
        // (CNAME Cross-User Banned). See cf_worker_to_worker_service_binding.
        const regionalReq = new Request(request, {
          headers: (() => {
            const h = new Headers(request.headers);
            // Strip ALL client-suppliable trust headers BEFORE the Worker
            // sets its own (delete-then-set). The Worker legitimately sets
            // x-corelink-fanout-from + x-corelink-primary-region below from
            // server-trusted values.
            stripClientTrustHeaders(h);
            h.set("x-request-id", requestId);
            h.set("x-corelink-route-kind", route.routeKind);
            h.set("x-corelink-token-prefix", auth.tokenPrefix);
            h.set("x-corelink-tenant-id", resolvedTenantId);
            // Storage-quota fail-open fix: forward the resolved per-tier storage
            // cap so the regional container seeds a fresh tenant_storage_state
            // row with the REAL cap (not uncapped `0`). Omitted when null
            // (system/anon/pending or D1-error tier) ⇒ container fails closed.
            // stripClientTrustHeaders above already deleted any client value.
            if (storageQuotaHeader !== null) {
              h.set(STORAGE_QUOTA_HEADER, storageQuotaHeader);
            }
            // H1: forward the D1-resolved PAT scope as a server-trust header.
            // stripClientTrustHeaders above already deleted any client value.
            h.set("x-corelink-scope", auth.scope);
            // WP5a: forward the NARROWED runner-job markers when the resolved PAT
            // carries a non-NULL runner_job_ac_key. Same posture as x-corelink-scope:
            // stripClientTrustHeaders above already deleted any client-supplied copies
            // of both headers (the Worker is the sole setter, from trusted D1). A
            // normal PAT (null) sets NEITHER header.
            if (auth.runnerJobAcKey !== null) {
              h.set("x-corelink-runner-job", "1");
              h.set("x-corelink-ac-key-allow", auth.runnerJobAcKey);
              // anti AC-squat: EVERY runner-job cred is create-only (deny-overwrite)
              // on the AC — the AC analog of the deny-DELETE narrowing at the SAME
              // chokepoint. The container (scope.rs::RunnerJob::ac_create_only) reads
              // this as first-writer-wins: CREATE ok, OVERWRITE of an existing entry
              // ⇒ 409. Stripped above, so only the Worker's value reaches the container.
              h.set("x-corelink-ac-create-only", "1");
            }
            // F1: forward CF's unforgeable client IP as x-corelink-client-ip
            // (the client-forgeable x-forwarded-for was stripped above) so the
            // regional Worker/container rate-limits signup off a trusted IP.
            h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
            // #11 (forgery-safe): mark the internal fan-out with the shared
            // server-to-server secret CORELINK_INTERNAL_AUTH_KEY, NOT a guessable
            // literal. The regional Worker accepts the fan-out (and so SKIPS
            // re-metering) only on a constant-time match against this same secret
            // (see the isFanout gate above) — a client-forged x-corelink-fanout-from
            // can never match, so it can never bypass metering. The value flows
            // ONLY over this service binding (set AFTER stripClientTrustHeaders),
            // never back to a client. If the secret is unbound the header is
            // omitted ⇒ the regional Worker meters (fail-SAFE over-count, never a
            // bypass).
            if (env.CORELINK_INTERNAL_AUTH_KEY && env.CORELINK_INTERNAL_AUTH_KEY.length > 0) {
              h.set("x-corelink-fanout-from", env.CORELINK_INTERNAL_AUTH_KEY);
            }
            // backlog #29: the trusted residency macro the container's residency
            // guard cross-checks against its own R2_CAS_REGION (defence-in-depth
            // against a mis-bound regional Worker). Set AFTER the strip so no
            // client value survives.
            h.set("x-corelink-primary-region", primaryRegion as string);
            return h;
          })(),
        });
        const regionalResp = await regionalBinding.fetch(regionalReq);
        return applyCors(regionalResp, request);
      }
      // colo === "iad" (wnam/enam) or primaryRegion undefined (tenant with no
      // row): fall through to the local IAD DO path below.
    }

    // Anonymous signup is global policy, not tenant-resident data. Every
    // regional Worker has a distinct CORELINK_SERVER Durable Object namespace,
    // so using its local `_anonymous` DO would give an IP five attempts per
    // region. The IAD Worker is the canonical authority; regional public
    // hosts must cross the authenticated Service Binding before any local DO
    // lookup. A missing binding fails closed rather than silently restoring a
    // per-region bypass. Unrelated `/v1/signup/*` flows keep their existing
    // regional routing and do not consume the pilot quota.
    if (
      request.method === "POST" &&
      route.routeKind === "signup" &&
      isPilotSignupPath(route.pathSuffix) &&
      env.R2_CAS_REGION !== "iad" &&
      (env.R2_CAS_REGION !== undefined || env.ENVIRONMENT.startsWith("prod-"))
    ) {
      const globalBinding = env.PROD_IAD;
      if (globalBinding === undefined) {
        return applyCors(
          reapiError("SERVICE_UNAVAILABLE", "signup authority unavailable", 503, requestId),
          request,
        );
      }
      const globalRequest = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", route.routeKind);
          h.set("x-corelink-token-prefix", auth.tokenPrefix);
          h.set("x-corelink-tenant-id", resolvedTenantId);
          h.set("x-corelink-scope", auth.scope);
          h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
          return h;
        })(),
      });
      return applyCors(await globalBinding.fetch(globalRequest), request);
    }

    // Route to the per-tenant DO. idFromName(resolvedTenantId) guarantees
    // each tenant gets its own isolated DO — never the shared "_pending_auth".
    const doId = env.CORELINK_SERVER.idFromName(resolvedTenantId);
    const stub = env.CORELINK_SERVER.get(doId, serverGetOpts(env));

    // F1 SHADOW (findMissingBlobs): the forward below reuses this request's body
    // stream, so a clone has to be taken HERE — after it, the body is gone. Only
    // under the shadow flag, and only for the one route, so no other request
    // pays for buffering. Nothing from it is logged (INV-NO-BODY-IN-LOGS); it
    // exists to be re-parsed by the comparison.
    const isFindMissing =
      request.method === "POST" &&
      route.routeKind === "bazel_v2" &&
      route.pathSuffix.endsWith("/findMissingBlobs");
    const findMissingBody =
      (env.EDGE_FIND_MISSING === "shadow" || env.EDGE_FIND_MISSING === "on") &&
      isFindMissing
        ? await request.clone().text()
        : null;
    const findMissingShadowBody =
      env.EDGE_FIND_MISSING === "shadow" ? findMissingBody : null;

    // Augment request with correlation headers (no body inspection — INV-NO-BODY-IN-LOGS)
    const augmented = new Request(request, {
      headers: (() => {
        const h = new Headers(request.headers);
        // Security (H4): strip ALL client-suppliable server-trust headers on the
        // data-plane forward. The Worker does NOT set internal-auth here, so the
        // container's internal-auth-gated admin routes become Worker-unreachable
        // by design — admin is operator-only (internal-auth path) posture.
        stripClientTrustHeaders(h);
        h.set("x-request-id", requestId);
        h.set("x-corelink-route-kind", route.routeKind);
        // Pass token prefix for DO-side audit correlation (NOT the raw token).
        h.set("x-corelink-token-prefix", auth.tokenPrefix);
        // Pass the PAT-resolved tenant to the DO so it can bind tenantId in
        // lifecycle state (resolves the null tenantId — WP-T1 DoD 4). Set AFTER
        // Worker auth; the DO MUST NOT trust any client-supplied value for it
        // (overwritten here unconditionally).
        h.set("x-corelink-tenant-id", resolvedTenantId);
        // Storage-quota fail-open fix: forward the resolved per-tier storage cap
        // so the container's byte-accounting reservation seeds a fresh
        // tenant_storage_state row with the REAL cap (not the legacy uncapped
        // `0`). Omitted when null (system/anon/pending or a D1-error tier) ⇒ the
        // container fails closed on an unseeded tenant (absence ≠ unlimited).
        // stripClientTrustHeaders above already deleted any client value (the
        // Worker is the sole setter — exactly like x-corelink-tenant-id).
        if (storageQuotaHeader !== null) {
          h.set(STORAGE_QUOTA_HEADER, storageQuotaHeader);
        }
        // H1: forward the D1-resolved PAT scope as a server-trust header so the
        // container can ENFORCE it. stripClientTrustHeaders above already deleted
        // any client-supplied x-corelink-scope (the Worker is the sole setter).
        h.set("x-corelink-scope", auth.scope);
        // WP5a: forward the NARROWED runner-job markers when the resolved PAT
        // carries a non-NULL runner_job_ac_key so the container (WP5b) enforces
        // deny-DELETE (+ optional exact-key). Same posture as x-corelink-scope:
        // stripClientTrustHeaders above already deleted any client-supplied copies
        // of both headers (the Worker is the sole setter, from trusted D1). A
        // normal PAT (null) sets NEITHER header.
        if (auth.runnerJobAcKey !== null) {
          h.set("x-corelink-runner-job", "1");
          h.set("x-corelink-ac-key-allow", auth.runnerJobAcKey);
          // anti AC-squat: EVERY runner-job cred is create-only (deny-overwrite) on
          // the AC — the AC analog of the deny-DELETE narrowing at the SAME
          // chokepoint. The container (scope.rs::RunnerJob::ac_create_only) reads this
          // as first-writer-wins: CREATE ok, OVERWRITE of an existing entry ⇒ 409.
          // Stripped above, so only the Worker's value reaches the container.
          h.set("x-corelink-ac-create-only", "1");
        }
        // F1: forward Cloudflare's UNFORGEABLE client IP as the server-trusted
        // x-corelink-client-ip so the container's signup rate-limit keys off it
        // (NOT the client-forgeable x-forwarded-for, which stripClientTrustHeaders
        // already deleted above). cf-connecting-ip is set by the CF edge and a
        // client cannot spoof it. The signup routeKind reaches THIS forward.
        h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
        // Keep raw Authorization on the forwarded request: the DO proxies it
        // to the container. NOTE (F3/F17): the native plane (CAS/AC/Bazel/Turbo)
        // does NOT perform Argon2id re-verify — possession rests solely on the
        // Worker's HMAC gate above. The DO is trusted; it never logs the raw
        // value (INV-NO-PII-IN-LOGS enforced in durable_object.ts).
        // TODO(F3): wire adapter_pat::PatVerifier onto the native plane as a
        // container-side second possession layer (Option-B extension).
        // F-015: stamp the trusted residency macro on the LOCAL path too (not only
        // the fan-out path), so the container's residency backstop (residency.rs)
        // can cross-check it against its own R2_CAS_REGION. For an IAD-resident
        // tenant on the IAD container this maps enam/wnam→iad==iad → Allow (no
        // change); for a client that picked a regional public host with a
        // non-matching tenant it maps to a different colo → 409 residency_violation.
        // Omitted for anon/system/pending (primaryRegion undefined). stripClientTrustHeaders
        // already deleted any client-supplied value (the Worker is the sole setter).
        if (primaryRegion !== undefined) {
          h.set("x-corelink-primary-region", primaryRegion);
        }
        return h;
      })(),
    });

    // F3.3 F2 SERVE: on a brew/pip `_public` HIT, serve the bytes from the Worker
    // edge (native CONFIG_DB map + CAS_BUCKET R2) and SKIP the container round-trip
    // entirely — the whole point of F3.3, removing `origin` (~585 ms) from the HIT
    // path. Any miss / revocation / re-hash mismatch / fault yields null and we fall
    // through to the unchanged container path (which owns the upstream fill), so this
    // can only make a HIT faster, never change correctness. Reached only on the
    // tenant's home-region leg, so env.CAS_BUCKET/R2_CAS_REGION are correct here.
    // We deliberately do NOT stamp stOrigin*, so Server-Timing omits `origin` — the
    // absence of that phase IS the wire-level proof the container was bypassed.

  return { primaryRegion, stub, augmented, findMissingBody, findMissingShadowBody, stQResidMs };
}
