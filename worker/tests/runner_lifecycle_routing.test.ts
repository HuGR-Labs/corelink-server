import { describe, expect, it } from "vitest";
import type { Env } from "../src/index.js";
import { handleSpecialMiscRoute } from "../src/index_special_misc.js";
import { matchRoute } from "../src/route_match.js";

const KEY = "runner-route-test-key-0123456789abcdef";

function request(path: string, init: RequestInit = {}): Request {
  return new Request(`https://worker.test${path}`, { method: "POST", ...init });
}

function env(): Env {
  return {
    CORELINK_RUNNER_MINT_AUTH_KEY: KEY,
  } as unknown as Env;
}

describe("runner close-generation edge routing", () => {
  it("matches only the exact close-generation path", () => {
    expect(matchRoute(new URL("https://worker.test/internal/v1/runner/close-generation"))).toEqual({
      tenantId: "_system",
      pathSuffix: "/internal/v1/runner/close-generation",
      routeKind: "runner_close_generation",
    });
    for (const path of [
      "/internal/v1/runner/close-generation/",
      "/internal/v1/runner/close-generation/extra",
      "/internal/v1/runner/credentials/close-generation",
      "/internal/v1/runner/adopt",
    ]) {
      expect(matchRoute(new URL(`https://worker.test${path}`)).routeKind).not.toBe("runner_close_generation");
    }
  });

  it("dispatches unauthenticated requests to the handler", async () => {
    const req = request("/internal/v1/runner/close-generation", {
      headers: { "x-request-id": "req-close-1" },
    });
    const route = matchRoute(new URL(req.url));
    const response = await handleSpecialMiscRoute(req, env(), "req-close-1", Date.now(), 1, route);
    expect(response?.status).toBe(401);
    expect(response?.headers.get("x-request-id")).toBe("req-close-1");
  });
});
