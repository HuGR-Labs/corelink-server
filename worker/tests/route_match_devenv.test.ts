import { describe, expect, it } from "vitest";
import { matchRoute } from "../src/route_match.js";

describe("DevEnv route boundaries", () => {
  it.each(["/v1/customer/devenv", "/v1/customer/devenv/status", "/v1/devenv", "/v1/devenv/status"])("routes %s as DevEnv", path => {
    expect(matchRoute(new URL(`https://corelink.test${path}`)).routeKind).toBe("devenv_v1");
  });

  it.each(["/v1/customer/devenvfoo", "/v1/devenvfoo"])("does not route false-prefix %s as DevEnv", path => {
    expect(matchRoute(new URL(`https://corelink.test${path}`)).routeKind).not.toBe("devenv_v1");
  });
});
