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
    stop: async (..._args: never[]) => { events.push("stop"); return { sessionUuid, status: "stopped" as const }; },
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
    expect(events.indexOf("stop")).toBeLessThan(events.indexOf("abandon"));
  });

  it("stops after ACK before compensating when adoption fails", async () => {
    const events: string[] = [];
    const response = await relayAuthorizedDevenvStart(new Request("https://x/v1/customer/devenv", { method: "POST", body: JSON.stringify({ workspace_name: "demo" }) }), tenantId, deps(events, { adopt: async () => { events.push("adopt"); return false; } }));
    expect(response.status).toBe(503);
    expect(events).toEqual(["prepare", "mint", "compute", "start", "adopt", "stop", "abandon", "revoke"]);
  });

  it("attempts stop even when the stop RPC rejects, without leaking secrets", async () => {
    const events: string[] = [];
    const response = await relayAuthorizedDevenvStart(new Request("https://x/v1/customer/devenv", { method: "POST", body: JSON.stringify({ workspace_name: "demo" }) }), tenantId, deps(events, { stop: async () => { events.push("stop"); throw new Error("token_plaintext=secret"); }, adopt: async () => { events.push("adopt"); return false; } }));
    expect(response.status).toBe(503);
    expect(events.slice(-3)).toEqual(["stop", "abandon", "revoke"]);
  });

  it("stops after a start RPC throws, covering delayed side effects", async () => {
    const events: string[] = [];
    const response = await relayAuthorizedDevenvStart(new Request("https://x/v1/customer/devenv", { method: "POST", body: JSON.stringify({ workspace_name: "demo" }) }), tenantId, deps(events, { start: async () => { events.push("start"); throw new Error("runner unavailable"); } }));
    expect(response.status).toBe(503);
    expect(events.slice(-3)).toEqual(["stop", "abandon", "revoke"]);
  });

  it("does not stop before the start RPC is attempted", async () => {
    const events: string[] = [];
    const response = await relayAuthorizedDevenvStart(new Request("https://x/v1/customer/devenv", { method: "POST", body: JSON.stringify({ workspace_name: "demo" }) }), tenantId, deps(events, { mint: async () => { events.push("mint"); return new Response(null, { status: 503 }); } }));
    expect(response.status).toBe(503);
    expect(events).not.toContain("stop");
  });

  it.each([
    ["absent", undefined],
    ["forged small", "1"],
  ])("rejects an oversized body with %s content length", async (_label, length) => {
    const headers = length === undefined ? undefined : { "content-length": length };
    const body = JSON.stringify({ workspace_name: "x".repeat(4090) });
    const response = await relayAuthorizedDevenvStart(new Request("https://x/v1/customer/devenv", { method: "POST", headers, body }), tenantId, deps([]));
    expect(response.status).toBe(400);
  });

  it("rejects malformed content length", async () => {
    const response = await relayAuthorizedDevenvStart(new Request("https://x/v1/customer/devenv", { method: "POST", headers: { "content-length": "bogus" }, body: JSON.stringify({ workspace_name: "demo" }) }), tenantId, deps([]));
    expect(response.status).toBe(400);
  });
});
