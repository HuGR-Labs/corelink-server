import { describe, expect, it } from "vitest";
import { matchRoute } from "../src/route_match.js";

describe("runner credential lifecycle routes", () => {
  const path = "/internal/v1/runner/credentials/close-generation";

  it("matches the close-generation route exactly before generic routes", () => {
    const route = matchRoute(new URL(`https://api.example${path}`));
    expect(route).toEqual({ tenantId: "_system", pathSuffix: path, routeKind: "runner_close_generation" });
  });

  it("does not broaden the close-generation route", () => {
    expect(
      matchRoute(new URL("https://api.example/internal/v1/runner/credentials/close-generation/"))
        .routeKind,
    ).toBe("not_found");
  });
});
