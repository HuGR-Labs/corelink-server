/** Public health and OpenAPI route domain. */
import type { Env } from "./index_common.js";
import { applyCors } from "./index_auth.js";
import type { RouteMatch } from "./route_match.js";
import { reapiError, serverGetOpts } from "./index_common.js";
import { requireDedicatedAdminAuth } from "./lib/internal_auth.js";

export async function handlePublicHealthRoute(
  request: Request,
  env: Env,
  requestId: string,
  route: RouteMatch,
): Promise<Response | null> {
    // Health check — no auth required, no DO forwarding
    // Legacy /health keeps {"status":"ok",...}; /api/health returns "SERVING"
    // to match the e2e suite's Journey 1 exact-string assertion
    // (tests/e2e-user-journeys/src/main.rs:151).
    if (route.routeKind === "health" || route.routeKind === "health_serving") {
      const statusLiteral = route.routeKind === "health_serving" ? "SERVING" : "ok";
      // F19: omit `env` — deployment environment must not be disclosed on
      // unauthenticated endpoints. Serve env detail only on internal/authed paths.
      const body = JSON.stringify({ status: statusLiteral });
      const resp = new Response(body, {
        status: 200,
        headers: {
          "Content-Type": "application/json",
          "X-Request-Id": requestId,
        },
      });
      return applyCors(resp, request);
    }

    // Public OpenAPI 3.1 schema — /openapi.json (WP-08)
    if (route.routeKind === "openapi") {
      if (request.method !== "GET" && request.method !== "HEAD") {
        return new Response("Method Not Allowed", { status: 405 });
      }
      // The canonical contract. `openapi_v1.ts` is GENERATED from
      // `openapi/corelink-v1.yaml` by `scripts/openapi_sync.py`, and
      // `--check` fails the PR on drift — so this adds no second source of
      // truth to keep in step: it is a projection of the one we review.
      //
      // Dynamic so the spec is not parsed on isolate start for the requests
      // that never ask for it.
      const { corelinkV1Json } = await import("./lib/openapi_v1.js");
      const resp = new Response(corelinkV1Json, {
        status: 200,
        headers: {
          "Content-Type": "application/json",
          "Cache-Control": "public, max-age=300",
          "X-Request-Id": requestId,
        },
      });
      return applyCors(resp, request);
    }

    // DevEnv sub-surface spec — published ONLY where the feature is wired.
    //
    // Every one of the eight DevEnv endpoints answers 503 unless the
    // `RUNNER_DEVENV_DO` binding exists (see the `devenv_v1` arm below), and
    // that binding is absent from every deployed environment — measured against
    // the live Worker through the Cloudflare API, not inferred from the repo:
    // `corelink-prod` has 97 bindings and 6 Durable Object bindings
    // (CORELINK_SERVER, EVENT_LOG_DO, REPLICATION_COORDINATOR_DO,
    // REQUEST_METER_COORDINATOR_DO, REQUEST_METER_SHARD_DO, ROLLOUT_DO), and
    // RUNNER_DEVENV_DO is not among them.
    //
    // So the spec is gated on the same binding the endpoints are gated on. A
    // contract cannot outlive the thing it describes: where DevEnv is wired the
    // spec is published, and where it is not, asking for it is a 404 rather
    // than a document promising eight endpoints that cannot answer. When the
    // binding is deployed this starts serving on its own — nothing to remember.
    if (route.routeKind === "openapi_devenv") {
      if (request.method !== "GET" && request.method !== "HEAD") {
        return new Response("Method Not Allowed", { status: 405 });
      }
      if (!env.RUNNER_DEVENV_DO) {
        return applyCors(
          reapiError(
            "NOT_FOUND",
            "DevEnv is not enabled in this environment; its API contract is not published here.",
            404,
            requestId,
          ),
          request,
        );
      }
      const { devenvOpenApiSpec } = await import("./lib/openapi_devenv.js");
      const resp = new Response(JSON.stringify(devenvOpenApiSpec), {
        status: 200,
        headers: {
          "Content-Type": "application/json",
          "Cache-Control": "public, max-age=300",
          "X-Request-Id": requestId,
        },
      });
      return applyCors(resp, request);
    }

    // Container health deep-probe — both variants forward to the container's
    // /_health via the _system DO. The authenticated variant is the only path
    // that may return storage backing/topology diagnostics.
    if (
      route.routeKind === "health_container" ||
      route.routeKind === "health_container_authed"
    ) {
      const includeStorage = route.routeKind === "health_container_authed";
      if (includeStorage) {
        const authError = requireDedicatedAdminAuth(request, env, requestId);
        if (authError !== null) {
          return applyCors(authError, request);
        }
        if (request.method !== "GET" && request.method !== "HEAD") {
          return applyCors(
            reapiError("METHOD_NOT_ALLOWED", "health probe requires GET", 405, requestId),
            request,
          );
        }
      }
      const systemDoId = env.CORELINK_SERVER.idFromName("_system");
      const systemStub = env.CORELINK_SERVER.get(systemDoId, serverGetOpts(env));
      const containerHealthUrl = new URL(request.url);
      containerHealthUrl.pathname = "/_health";
      // The container health handler has no query parameters. Never carry a
      // caller-controlled query string (especially a possible secret) onward.
      containerHealthUrl.search = "";
      const containerReq = new Request(containerHealthUrl.toString(), {
        method: "GET",
        headers: (() => {
          const h = new Headers();
          h.set("x-request-id", requestId);
          h.set(
            "x-corelink-route-kind",
            includeStorage ? "health_container_authed" : "health_container",
          );
          h.set("x-corelink-tenant-id", "_system");
          return h;
        })(),
      });
      let containerResp: Response;
      try {
        containerResp = await systemStub.fetch(containerReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] health_container DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "container health upstream error", 500, requestId),
          request,
        );
      }
      const containerHeaders = new Headers(containerResp.headers);
      if (!containerHeaders.has("x-request-id")) {
        containerHeaders.set("x-request-id", requestId);
      }
      if (includeStorage) {
        // The dedicated admin key was verified above. Preserve the deep probe
        // body, including `storage`, without forwarding the credential.
        containerHeaders.set("Cache-Control", "no-store");
        return applyCors(
          new Response(containerResp.body, {
            status: containerResp.status,
            statusText: containerResp.statusText,
            headers: containerHeaders,
          }),
          request,
        );
      }
      // Strip storage backing and topology fields (L1/B-082): parse JSON,
      // delete both diagnostics, then re-serialize. If the body is not valid
      // JSON (container returned an error body or non-JSON), use a generic safe
      // body instead of streaming potentially sensitive upstream content.
      let redactedBody: BodyInit;
      try {
        const raw = await containerResp.json() as Record<string, unknown>;
        delete raw["storage"];
        delete raw["topology"];
        redactedBody = JSON.stringify(raw);
        containerHeaders.set("Content-Type", "application/json");
      } catch {
        // Non-JSON body (e.g. container down, returned plain-text error):
        // preserve status/liveness semantics without exposing raw content.
        redactedBody = JSON.stringify({ status: "unavailable" });
        containerHeaders.set("Content-Type", "application/json");
      }
      return applyCors(
        new Response(redactedBody, {
          status: containerResp.status,
          statusText: containerResp.statusText,
          headers: containerHeaders,
        }),
        request,
      );
    }

  return null;
}
