import { describe, expect, it } from "vitest";
import { relayAuthorizedDevenvStart } from "../src/lib/devenv_relay.js";
import { matchRoute } from "../src/route_match.js";

const tenantId = "11111111-1111-4111-8111-111111111111";
const patId = "22222222-2222-4222-8222-222222222222";
const sessionUuid = "33333333-3333-4333-8333-333333333333";

function deps(events: string[], overrides: Partial<Record<string, (...args: never[]) => unknown>> = {}) {
  return {
    lifecycleGeneration: "7",
    prepare: async (..._args: never[]) => { events.push("prepare"); return true; },
    prepareCompute: async (..._args: never[]) => { events.push("compute"); return sessionUuid; },
    abandonCompute: async (..._args: never[]) => { events.push("abandon"); },
    mint: async (..._args: never[]) => { events.push("mint"); return Response.json({ pat_id: patId, token_plaintext: "secret", tenant: tenantId, expires_ms: Date.now() + 1000 }); },
    revoke: async (..._args: never[]) => { events.push("revoke"); return true; },
    adopt: async (..._args: never[]) => { events.push("adopt"); return true; },
    start: async (..._args: never[]) => { events.push("start"); return { sessionUuid, status: "running" as const }; },
    now: () => Date.now(), sessionId: () => sessionUuid, cleanupFailed: () => events.push("cleanup"), ...overrides,
  };
}

describe("authorized DevEnv issuance", () => {
  it("is reachable through the canonical route matcher", () => {
    expect(matchRoute(new URL("https://corelink.test/v1/customer/devenv")).routeKind).toBe("devenv_v1");
    expect(matchRoute(new URL("https://corelink.test/v1/devenv/status")).routeKind).toBe("devenv_v1");
  });

  it("runs obligation, mint, compute, start, and adoption in order", async () => {
    const events: string[] = [];
    const response = await relayAuthorizedDevenvStart(new Request("https://x/v1/customer/devenv", { method: "POST", body: JSON.stringify({ workspace_name: "demo" }) }), tenantId, deps(events));
    expect(response.status).toBe(201);
    expect(events).toEqual(["prepare", "mint", "compute", "start", "adopt"]);
  });

  it("compensates compute and credential obligation when start fails", async () => {
    const events: string[] = [];
    const response = await relayAuthorizedDevenvStart(new Request("https://x/v1/customer/devenv", { method: "POST", body: JSON.stringify({ workspace_name: "demo" }) }), tenantId, deps(events, { start: async () => { events.push("start"); throw new Error("runner unavailable"); } }));
    expect(response.status).toBe(503);
    expect(events).toContain("abandon");
    expect(events).toContain("revoke");
  });
});
