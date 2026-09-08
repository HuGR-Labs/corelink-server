import { describe, expect, it } from "vitest";
import { isPilotSignupPath, matchRoute } from "../src/route_match.js";
import { CoreLinkServer } from "../src/durable_object.js";
import { makeEnv, makeMockState } from "./durable_object_part2_test_helpers.js";

describe("pilot signup route matching", () => {
  it("matches the canonical token path and rejects neighboring signup paths", () => {
    expect(isPilotSignupPath("/v1/signup/pilot/abc123")).toBe(true);
    expect(isPilotSignupPath("/v1/signup/pilot")).toBe(true);
    expect(isPilotSignupPath("/v1/signup/pilot/")).toBe(false);
    expect(isPilotSignupPath("/v1/signup/pilot/abc123/extra")).toBe(false);
    expect(isPilotSignupPath("/v1/signup/other/abc123")).toBe(false);
    expect(isPilotSignupPath("/v1/signup/pilot/../other")).toBe(false);

    const route = matchRoute(new URL("https://api.example/v1/signup/pilot/abc123"));
    expect(route.routeKind).toBe("signup");
    expect(route.pathSuffix).toBe("/v1/signup/pilot/abc123");
  });
});

describe("pilot signup durable admission", () => {
  it("enforces the token path at five per hour with an injected clock", async () => {
    let nowMs = 1_700_000_000_000;
    const state = makeMockState();
    const server = new CoreLinkServer(state, makeEnv(), () => nowMs);
    await new Promise<void>((resolve) => setTimeout(resolve, 5));

    const request = (path: string) => new Request(`http://localhost${path}`, {
      method: "POST",
      headers: { "x-corelink-client-ip": "203.0.113.7" },
    });

    // No container is bound in this unit harness, so admitted requests reach
    // the normal 503 startup response. The sixth request must be rejected by
    // the durable gate before container startup.
    for (let i = 0; i < 5; i += 1) {
      expect((await server.fetch(request(`/v1/signup/pilot/token-${i}`))).status).toBe(503);
    }
    expect((await server.fetch(request("/v1/signup/pilot/token-5"))).status).toBe(429);

    nowMs += 60 * 60 * 1000;
    expect((await server.fetch(request("/v1/signup/pilot/token-after-window"))).status).toBe(503);
  });

  it("does not spend the pilot quota on unrelated signup paths", async () => {
    let nowMs = 1_700_000_000_000;
    const state = makeMockState();
    const server = new CoreLinkServer(state, makeEnv(), () => nowMs);
    await new Promise<void>((resolve) => setTimeout(resolve, 5));

    const request = (path: string) => new Request(`http://localhost${path}`, {
      method: "POST",
      headers: { "x-corelink-client-ip": "198.51.100.4" },
    });

    expect((await server.fetch(request("/v1/signup/other"))).status).toBe(503);
    for (let i = 0; i < 5; i += 1) {
      expect((await server.fetch(request(`/v1/signup/pilot/token-${i}`))).status).toBe(503);
    }
    expect((await server.fetch(request("/v1/signup/pilot/token-5"))).status).toBe(429);
  });
});
