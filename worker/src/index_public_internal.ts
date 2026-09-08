/** Internal authenticated route domain. */
import type { ExecutionContext } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import { applyCors, stripClientTrustHeaders } from "./index_auth.js";
import type { RouteMatch } from "./route_match.js";
import { reapiError, serverGetOpts, internalConsumerForPath, INTERNAL_DO_D1_PROBE_PREFIX } from "./index_common.js";
import { resolveConsumerKey } from "./lib/internal_auth.js";
import { ReplicationCoordinatorDO, REPLICATION_COORDINATOR_SINGLETON } from "./replication_coordinator_do.js";
import type { KvReader } from "./lib/pat_verify_cache.js";
import { isDsrEraseFanoutPath } from "./index_observability.js";
import { writePublicBlocklistKv } from "./lib/edge_public_read.js";

export async function handlePublicInternalRoute(
  request: Request,
  env: Env,
  ctx: ExecutionContext,
  requestId: string,
  route: RouteMatch,
): Promise<Response | null> {
    // Internal routes — `/_internal/*` — authenticated by X-Corelink-Internal-Auth.
    // Bypasses PAT auth entirely; DO forwards directly to the container.
    //
    // Security (red-team #3): the gate uses the PER-CONSUMER key split, mirroring
    // the container's Rust split — a leak of one consumer's secret must not unlock
    // every internal surface. The consumer is derived from the path prefix; each
    // consumer key falls back to the shared CORELINK_INTERNAL_AUTH_KEY when its
    // dedicated key is UNSET (resolveConsumerKey) — a dedicated key that is set
    // but under the floor is REFUSED rather than widened to the shared key. If
    // neither qualifies, deny (fail-CLOSED — never open an unauthenticated proxy).
    if (route.routeKind === "internal") {
      const internalConsumer = internalConsumerForPath(route.pathSuffix);
      const internalAuthKey = resolveConsumerKey(env, internalConsumer);
      if (!internalAuthKey || internalAuthKey.length === 0) {
        // No properly sized key bound for this consumer — deny (fail-CLOSED).
        return applyCors(
          reapiError("FORBIDDEN", "internal route unavailable", 403, requestId),
          request,
        );
      }
      // Verify the caller supplied the correct shared secret (constant-time).
      const provided = request.headers.get("x-corelink-internal-auth") ?? "";
      const enc2 = new TextEncoder();
      const expectedBytes = enc2.encode(internalAuthKey);
      const providedBytes = enc2.encode(provided);
      // Constant-time auth WITHOUT a secret-length oracle. The previous
      // `providedBytes.length === expectedBytes.length` branch took a timing
      // path that depended on the PROVIDED length (and the else-branch compared
      // the secret to itself, not to the provided bytes) — both distinguishable
      // → a length oracle. Instead: copy the provided bytes into a fixed buffer
      // sized to the EXPECTED length (pad with zeros / truncate the overflow),
      // run exactly ONE timingSafeEqual over equal-length buffers, then AND with
      // a constant-time length-equality bit. No early branch depends on the
      // provided length. Mirrors the padded Rust internal_pat.rs/admin.rs gates.
      const fixed = new Uint8Array(expectedBytes.length);
      const copyLen =
        providedBytes.length < expectedBytes.length
          ? providedBytes.length
          : expectedBytes.length;
      fixed.set(providedBytes.subarray(0, copyLen));
      const bytesEqual = crypto.subtle.timingSafeEqual(fixed, expectedBytes);
      // Length-equality bit — a length mismatch can never authenticate (a wrong
      // length that pads to the same prefix bytes is still rejected). This is a
      // single integer compare, not a per-character path, so it carries no
      // length oracle: timingSafeEqual already ran over equal-length buffers.
      const lenEqual = providedBytes.length === expectedBytes.length;
      const authOk = bytesEqual && lenEqual;
      if (!authOk) {
        return applyCors(
          reapiError("UNAUTHORIZED", "internal auth required", 401, requestId),
          request,
        );
      }

      // WI-MULTI-REGION-V1 — the replication-coordinator singleton DO.
      // `/_internal/replication/*` is served by ReplicationCoordinatorDO (NOT the
      // container): the SINGLE global instance
      // (idFromName(REPLICATION_COORDINATOR_SINGLETON)) whose single-writer
      // guarantee IS the split-brain-safe promotion lock. We intercept it here —
      // AFTER the shared internal-auth gate above, BEFORE the generic container
      // forward — mapping `/_internal/replication/<op>` → the DO's `/_repl/<op>`.
      if (route.pathSuffix.startsWith("/_internal/replication/")) {
        if (!env.REPLICATION_COORDINATOR_DO) {
          return applyCors(
            reapiError("NOT_IMPLEMENTED", "replication coordinator DO not bound", 501, requestId),
            request,
          );
        }
        const coordId = env.REPLICATION_COORDINATOR_DO.idFromName(REPLICATION_COORDINATOR_SINGLETON);
        const coordStub = env.REPLICATION_COORDINATOR_DO.get(coordId);
        const coordUrl = new URL(request.url);
        coordUrl.pathname = route.pathSuffix.replace("/_internal/replication", "/_repl");
        const coordHeaders = new Headers(request.headers);
        stripClientTrustHeaders(coordHeaders);
        coordHeaders.set("x-request-id", requestId);
        const coordReq = new Request(coordUrl.toString(), {
          method: request.method,
          headers: coordHeaders,
          body:
            request.method === "GET" || request.method === "HEAD"
              // `null`, not `undefined`: with `exactOptionalPropertyTypes`
              // a possibly-undefined `body` is not assignable to RequestInit.
              // `null` is the spec-correct "no body" for GET/HEAD.
              ? null
              : await request.clone().arrayBuffer(),
        });
        return applyCors(await coordStub.fetch(coordReq), request);
      }

      // DO D1-PLACEMENT PROBE — `/_internal/do-d1-probe/{tenant_id}` → that
      // tenant's CoreLinkServer DO `/_do/health`. Same shape as the replication
      // forward above (rewrite the path, strip client trust headers, forward to
      // a DO stub, return its body verbatim), intercepted AFTER the internal-auth
      // gate and BEFORE the generic `_system` container forward.
      //
      // WHY THE TENANT IS IN THE PATH: DO placement is per-DO-id. The id is
      // derived here with `env.CORELINK_SERVER.idFromName(tenantId)` — the
      // IDENTICAL derivation the tenant data path uses ("Route to the per-tenant
      // DO" below), against the same namespace binding in the same Worker, so
      // this reaches the SAME DO instance that serves that tenant's traffic (a
      // fresh DO would measure a different placement and answer nothing).
      //
      // CAVEAT (must be understood before trusting the number): a tenant whose
      // `primary_region` maps to a non-IAD colo is served by a REGIONAL Worker
      // via a Service Binding (see the residency fan-out below), and that
      // regional Worker has its OWN CORELINK_SERVER namespace — so its DO is a
      // different instance living in a different region. `/_internal/*` does NOT
      // fan out. For such a tenant, probe the REGIONAL Worker, not this one.
      if (route.pathSuffix.startsWith(INTERNAL_DO_D1_PROBE_PREFIX)) {
        const rawTenant = route.pathSuffix.slice(INTERNAL_DO_D1_PROBE_PREFIX.length);
        let probeTenantId: string;
        try {
          probeTenantId = decodeURIComponent(rawTenant);
        } catch {
          probeTenantId = "";
        }
        if (probeTenantId.length === 0 || probeTenantId.includes("/")) {
          return applyCors(
            reapiError(
              "INVALID_ARGUMENT",
              "usage: /_internal/do-d1-probe/{tenant_id}[?colo=1]",
              400,
              requestId,
            ),
            request,
          );
        }
        const probeDoId = env.CORELINK_SERVER.idFromName(probeTenantId);
        const probeStub = env.CORELINK_SERVER.get(probeDoId, serverGetOpts(env));
        const probeUrl = new URL(request.url);
        probeUrl.pathname = "/_do/health";
        const probeHeaders = new Headers(request.headers);
        // Strip every client-suppliable trust header (incl. the caller's
        // internal-auth secret and any x-corelink-tenant-id): the probe reads a
        // health page, it must never smuggle authority into the DO.
        stripClientTrustHeaders(probeHeaders);
        probeHeaders.set("x-request-id", requestId);
        // Always GET: /_do/health is a read, and this must not be a body-carrying
        // path into the DO.
        const probeReq = new Request(probeUrl.toString(), {
          method: "GET",
          headers: probeHeaders,
        });
        // Body verbatim. NOTE: /_do/health answers 503 while the tenant's
        // container is not running — the d1_probe numbers are still in the body,
        // so read the BODY, not the status.
        return applyCors(await probeStub.fetch(probeReq), request);
      }

      // Route to the _system DO which hosts the LOCAL (this-region) container.
      const systemDoId = env.CORELINK_SERVER.idFromName("_system");
      const systemStub = env.CORELINK_SERVER.get(systemDoId, serverGetOpts(env));
      // Build server-trusted internal headers (client trust headers stripped, then
      // re-established). `fanoutFrom`, when set, marks a request as ALREADY fanned
      // out so the receiving regional worker does not re-fan (loop guard).
      const buildInternalHeaders = (src: Headers, fanoutFrom?: string): Headers => {
        const h = new Headers(src);
        // Strip ALL client-suppliable trust headers BEFORE re-establishing them
        // (delete-then-set): a client must never smuggle x-admin-* / fanout-from,
        // nor a forged internal-auth.
        stripClientTrustHeaders(h);
        h.set("x-request-id", requestId);
        h.set("x-corelink-route-kind", "internal");
        h.set("x-corelink-tenant-id", "_system");
        h.set("x-corelink-token-prefix", "internal");
        h.set("x-corelink-internal-auth", internalAuthKey);
        if (fanoutFrom) h.set("x-corelink-fanout-from", fanoutFrom);
        return h;
      };

      // ── Operator container force-recycle ──────────────────────────────────────
      // `POST /_internal/admin/recycle-system` destroys the `_system` container so
      // the NEXT request boots it FRESH — the only reliable way to apply a rotated
      // START-TIME secret (e.g. a rotated CORELINK_ERASE_AUTH_KEY) to an
      // already-running DO container. A container-app ROLLOUT does NOT restart a
      // live DO container, and a hot `_system` container does not self-cycle within
      // any useful window (observed: 44+ min uptime past a key rotation), so a
      // rotated erase key silently 401s every fan-out leg until forced. Reuses the
      // existing DO management path `/_do/stop` (handleStop → destroyContainer →
      // container.destroy()); the operator gate is THIS apex internal-auth check
      // (admin consumer key). Best-effort + idempotent: destroying a cold/fresh
      // container is harmless (it just reboots), so — unlike the erase fan-out — it
      // reports per-region status instead of failing closed. Fans to every regional
      // worker so one call converges the whole fleet onto the current start-env.
      if (route.pathSuffix === "/_internal/admin/recycle-system") {
        const stopLocal = async (): Promise<boolean> => {
          const stopUrl = new URL(request.url);
          stopUrl.pathname = "/_do/stop";
          try {
            const r = await systemStub.fetch(
              new Request(stopUrl.toString(), {
                method: "POST",
                headers: buildInternalHeaders(request.headers),
              }),
            );
            return r.ok;
          } catch (err: unknown) {
            const m = err instanceof Error ? err.message : "unknown error";
            console.error(
              `[${requestId}] recycle-system local /_do/stop threw: ${m.slice(0, 80)}`,
            );
            return false;
          }
        };
        const jsonResp = (obj: unknown, status: number): Response =>
          new Response(JSON.stringify(obj), {
            status,
            headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
          });
        // A fan-out target (a regional worker) recycles ONLY its own local container.
        if (request.headers.has("x-corelink-fanout-from")) {
          const ok = await stopLocal();
          return applyCors(
            jsonResp({ recycled: ok, scope: "regional-local", request_id: requestId }, ok ? 200 : 502),
            request,
          );
        }
        const localOk = await stopLocal();
        const recycleRegionals: Array<[string, { fetch: typeof fetch } | undefined]> = [
          ["lhr", env.PROD_LHR],
          ["sam", env.PROD_SAM],
          ["nrt", env.PROD_NRT],
          ["syd", env.PROD_SYD],
        ];
        const regionOutcomes = await Promise.all(
          recycleRegionals.map(async ([region, binding]) => {
            if (binding === undefined) {
              return { region, recycled: false, reason: "binding_absent" };
            }
            try {
              const r = await binding.fetch(
                new Request(request.url, {
                  method: "POST",
                  headers: buildInternalHeaders(request.headers, "iad"),
                }),
              );
              return { region, recycled: r.ok };
            } catch (err: unknown) {
              const m = err instanceof Error ? err.message : "unknown error";
              return { region, recycled: false, reason: m.slice(0, 80) };
            }
          }),
        );
        return applyCors(
          jsonResp(
            { recycled_local: localOk, regions: regionOutcomes, request_id: requestId },
            200,
          ),
          request,
        );
      }

      // ── GDPR Art.17 erasure completeness across residency (CAA-360 CRITICAL) ──
      // A DSR erase MUST run in EVERY jurisdiction the tenant could have data. An
      // EU tenant's CAS/AC bytes live in the prod-lhr container's EU buckets
      // (corelink-cas-eu / corelink-ac-eu), which the local (IAD) container's R2
      // client cannot (and must not) reach. When THIS worker is the erase ORIGIN
      // (not itself a fan-out target), fan the erase out to every regional worker
      // (each erases its own jurisdiction's buckets) and return "complete" (the
      // local 2xx) ONLY when the local AND every regional sweep confirm — else fail
      // CLOSED (502) so the queue consumer retries and NO false VerifiedComplete is
      // ever signed. Fail-closed by construction: a missing binding, transport
      // error, or non-2xx from any region → not complete → retry.
      const isDsrErase = isDsrEraseFanoutPath(route.pathSuffix); // allowlist, NOT startsWith — see fn doc
      const isFanoutTarget = request.headers.has("x-corelink-fanout-from");
      let internalResp: Response;
      if (isDsrErase && !isFanoutTarget) {
        // Buffer the body ONCE — it is replayed to the local container + each region.
        const eraseBody = await request.arrayBuffer();
        let localResp: Response;
        try {
          localResp = await systemStub.fetch(
            new Request(request.url, {
              method: request.method,
              headers: buildInternalHeaders(request.headers),
              body: eraseBody,
            }),
          );
        } catch (err: unknown) {
          const message = err instanceof Error ? err.message : "unknown error";
          console.error(`[${requestId}] dsr-erase local DO fetch failed: ${message.slice(0, 80)}`);
          return applyCors(
            reapiError("INTERNAL_ERROR", "internal upstream error", 500, requestId),
            request,
          );
        }
        const regionals: Array<[string, { fetch: typeof fetch } | undefined]> = [
          ["lhr", env.PROD_LHR],
          ["sam", env.PROD_SAM],
          ["nrt", env.PROD_NRT],
          ["syd", env.PROD_SYD],
        ];
        const regionResults = await Promise.all(
          regionals.map(async ([region, binding]) => {
            if (binding === undefined) {
              // A missing regional binding on the ORIGIN worker means we cannot
              // prove that jurisdiction was erased → fail CLOSED (never assume).
              console.error(
                `[${requestId}] dsr-erase fan-out: PROD_${region.toUpperCase()} binding absent`,
              );
              return { region, ok: false };
            }
            try {
              const resp = await binding.fetch(
                new Request(request.url, {
                  method: request.method,
                  headers: buildInternalHeaders(request.headers, "iad"),
                  body: eraseBody,
                }),
              );
              return { region, ok: resp.ok };
            } catch (err: unknown) {
              const message = err instanceof Error ? err.message : "unknown error";
              console.error(
                `[${requestId}] dsr-erase fan-out to ${region} threw: ${message.slice(0, 80)}`,
              );
              return { region, ok: false };
            }
          }),
        );
        const failedRegions = regionResults.filter((r) => !r.ok).map((r) => r.region);
        if (!localResp.ok || failedRegions.length > 0) {
          console.error(
            `[${requestId}] dsr-erase INCOMPLETE (fail-closed): local_ok=${localResp.ok} failed_regions=${
              failedRegions.join(",") || "none"
            }`,
          );
          return applyCors(
            reapiError(
              "INTERNAL_ERROR",
              "dsr erase incomplete across residency regions; retrying",
              502,
              requestId,
            ),
            request,
          );
        }
        internalResp = localResp;
      } else if (route.pathSuffix === "/_internal/public/revoke") {
        // B1b — collapse the edge revocation window. Forward the revoke to the
        // container (the authoritative D1 blocklist + cache_map delete + R2 erase),
        // and on SUCCESS also write the content_hash-keyed edge blocklist KV so a
        // revoked `_public` hash stops edge-serving within KV propagation
        // (~seconds) instead of the map-cache TTL (~60s). Buffer the body once: it
        // is both forwarded to the container AND parsed here for the content_hash.
        // The KV write is best-effort (ctx.waitUntil) — a KV fault must NEVER fail
        // the revoke, which the container has already applied authoritatively.
        const revokeBody = await request.arrayBuffer();
        try {
          internalResp = await systemStub.fetch(
            new Request(request.url, {
              method: request.method,
              headers: buildInternalHeaders(request.headers),
              body: revokeBody,
            }),
          );
        } catch (err: unknown) {
          const message = err instanceof Error ? err.message : "unknown error";
          console.error(`[${requestId}] public-revoke DO fetch failed: ${message.slice(0, 80)}`);
          return applyCors(
            reapiError("INTERNAL_ERROR", "internal upstream error", 500, requestId),
            request,
          );
        }
        const revokeKv = (env as unknown as { METADATA_KV?: KvReader }).METADATA_KV;
        if (internalResp.ok && revokeKv) {
          // Mark the brew/pip edge `pubblock:<hash>` KV from the RESOLVED
          // content_hash the container returns in its RESPONSE — authoritative for
          // BOTH revoke spaces. Parsing the REQUEST body (as before) missed the
          // revoke-by-`upstream_digest` incident path, whose request carries no
          // `content_hash`, leaving poisoned brew/pip bytes edge-serving for up to
          // ~60 s after an authoritative container revoke (finding F-1). Buffer the
          // body and rebuild the Response so the client leg below still streams it.
          let respText = "";
          try {
            respText = await internalResp.text();
          } catch {
            respText = "";
          }
          internalResp = new Response(respText, {
            status: internalResp.status,
            statusText: internalResp.statusText,
            headers: internalResp.headers,
          });
          try {
            const parsed = JSON.parse(respText) as { content_hash?: unknown };
            const ch =
              typeof parsed.content_hash === "string"
                ? parsed.content_hash.toLowerCase()
                : "";
            if (/^[0-9a-f]{64}$/.test(ch)) {
              ctx.waitUntil(writePublicBlocklistKv(revokeKv, ch));
            }
          } catch {
            // Response was not the expected {content_hash} JSON — the container
            // still revoked authoritatively; the edge falls back to the ~60 s map window.
          }
        }
      } else {
        // Non-erase internal route (pat/mint, admin, …) OR a fan-out target (a
        // regional worker running the erase for its own jurisdiction): single local
        // container hit, byte-identical to the pre-fan-out behaviour.
        const internalAugmented = new Request(request, {
          headers: buildInternalHeaders(request.headers),
        });
        try {
          internalResp = await systemStub.fetch(internalAugmented);
        } catch (err: unknown) {
          const message = err instanceof Error ? err.message : "unknown error";
          console.error(`[${requestId}] internal DO fetch failed: ${message.slice(0, 80)}`);
          return applyCors(
            reapiError("INTERNAL_ERROR", "internal upstream error", 500, requestId),
            request,
          );
        }
      }
      const internalHeaders = new Headers(internalResp.headers);
      if (!internalHeaders.has("x-request-id")) {
        internalHeaders.set("x-request-id", requestId);
      }
      return applyCors(
        new Response(internalResp.body, {
          status: internalResp.status,
          statusText: internalResp.statusText,
          headers: internalHeaders,
        }),
        request,
      );
    }

  return null;
}
