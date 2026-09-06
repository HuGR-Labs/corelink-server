/** Optional edge-native read optimisations. */
import type { ExecutionContext } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import type { RouteMatch } from "./route_match.js";
import type { AuthStageResult } from "./index_auth_stage.js";
import type { RoutingStageResult } from "./index_routing_stage.js";
import { resolveConsumerKey } from "./lib/internal_auth.js";
import { readPublicHit, parseByteRange, PUBLIC_BLOB_CONTENT_TYPE } from "./lib/edge_public_read.js";
import { findMissingResponseBody, serveEdgeFindMissing } from "./lib/edge_find_missing.js";

export async function serveEdgeOptimisations(
  request: Request,
  env: Env,
  ctx: ExecutionContext,
  requestId: string,
  route: RouteMatch,
  authStage: AuthStageResult,
  routing: RoutingStageResult,
): Promise<Response | null> {
  const { auth, resolvedTenantId } = authStage;
  const { stub, findMissingBody } = routing;
    let edgeServed: Response | null = null;
    if (
      env.EDGE_PUBLIC_READ === "serve" &&
      request.method === "GET" &&
      (route.routeKind === "brew" || route.routeKind === "pip")
    ) {
      try {
        const edge = await readPublicHit(env, route.routeKind, route.pathSuffix, ctx);
        if (edge) {
          // $-CEILING EXEMPTION (owner decision 2026-08-16): a `_public` cache
          // HIT served from the edge deliberately does NOT pass through the
          // container's per-op $-ceiling gate (ADR-0068). A shared, deduped,
          // content-addressed public read is ~free to serve, and the product's
          // whole promise is "the cache is cheap+fast" — charging the spend cap
          // on the cheapest, most-shared traffic class is off-brand. Request-count
          // + storage quota still apply (runQuotaBatch, above). Documented as an
          // invariant in the F3.3 ADR; not an accidental bypass.
          const total = edge.bytes.byteLength;
          const baseHeaders: Record<string, string> = {
            "x-cache": "HIT",
            // Faithful type for opaque CAS blobs (bottles/wheels); the container
            // binary read returns the same. Previously dropped on the edge path.
            "Content-Type": PUBLIC_BLOB_CONTENT_TYPE,
            "Accept-Ranges": "bytes",
          };
          const range = parseByteRange(request.headers.get("Range"), total);
          if (range === "unsatisfiable") {
            edgeServed = new Response(null, {
              status: 416,
              headers: { ...baseHeaders, "Content-Range": `bytes */${total}` },
            });
          } else if (range) {
            edgeServed = new Response(edge.bytes.slice(range.start, range.end + 1), {
              status: 206,
              headers: {
                ...baseHeaders,
                "Content-Range": `bytes ${range.start}-${range.end}/${total}`,
              },
            });
          } else {
            edgeServed = new Response(edge.bytes, { status: 200, headers: baseHeaders });
          }
        }
      } catch (e: unknown) {
        // An edge-read fault must NEVER fail a request that the container can serve.
        console.error(
          `[${requestId}] edge_public_serve error: ${String(e).slice(0, 80)}`,
        );
      }
    }

    // F2 SERVE (findMissingBlobs): answer in-colo instead of paying the
    // container's ~13 digests/second. Placed BEFORE the forward because that is
    // the whole point — after it, the cost is already spent. `serveEdgeFindMissing`
    // returns null for every reason to defer, INCLUDING an audit that did not
    // commit, and a deferral just falls into the normal forward below.
    if (
      env.EDGE_FIND_MISSING === "on" &&
      findMissingBody !== null &&
      !edgeServed &&
      resolvedTenantId
    ) {
      try {
        const auditKey = resolveConsumerKey(env, "audit_attempted");
        // No properly sized dedicated key ⇒ the container's route is unmounted
        // too, so there is nothing to call. Defer rather than probe-then-discard.
        if (auditKey) {
          const outcome = await serveEdgeFindMissing(
            env,
            resolvedTenantId,
            // The SAME value the container would have derived for these rows:
            // `bazel_v2.rs::principal` reads `x-corelink-token-prefix` and
            // defaults to `_unknown`. Sending anything else would attribute the
            // edge-served probes to a principal the container never uses, and
            // the audit trail would fork by who answered.
            auth.tokenPrefix || "_unknown",
            findMissingBody,
            async (batch) => {
              const r = await stub.fetch(
                new Request("https://container/_internal/audit/cas-attempted", {
                  method: "POST",
                  headers: {
                    "content-type": "application/json",
                    "x-corelink-internal-auth": auditKey,
                  },
                  body: JSON.stringify(batch),
                }),
              );
              // 204 and ONLY 204 authorizes serving. A 404 (route unmounted),
              // 503 (outbox down), or anything else means the rows are not
              // durable, and the container must answer instead.
              return r.status === 204;
            },
          );
          if (outcome) {
            edgeServed = new Response(findMissingResponseBody(outcome.missing), {
              status: 200,
              headers: { "content-type": "application/json" },
            });
            console.log(
              `[${requestId}] edge_find_missing_served n=${outcome.missing.length} edge_ms=${outcome.edgeMs}`,
            );
          }
        }
      } catch (e: unknown) {
        // Serving is an OPTIMISATION. A fault in it must never fail a request
        // the container can answer.
        console.error(
          `[${requestId}] edge_find_missing_serve error: ${String(e).slice(0, 80)}`,
        );
      }
    }


  return edgeServed;
}
