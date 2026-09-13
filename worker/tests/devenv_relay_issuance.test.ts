import { describe, expect, it, vi } from "vitest";
import { DEVENV_MAX_TTL_SECONDS, isDevenvStart, relayAuthorizedDevenvStart } from "../src/lib/devenv_relay.js";
import type { AuthorizedDevenvInput } from "../src/types/devenv_rpc.js";

const tenant = "00000000-0000-4000-8000-000000000001";
const session = "00000000-0000-4000-8000-000000000002";
const patId = "00000000-0000-4000-8000-000000000003";
const body = () => new Request("https://api.invalid/v1/customer/devenv", { method: "POST", body: JSON.stringify({ workspace_name: "workspace" }) });
function deps() {
  return {
    lifecycleGeneration: "7",
    prepareCompute: vi.fn(async () => null), abandonCompute: vi.fn(async () => {}),
    prepare: vi.fn(async () => true), mint: vi.fn(async () => Response.json({ tenant, pat_id: patId, token_plaintext: "secret", expires_ms: 1000 + DEVENV_MAX_TTL_SECONDS * 1000 })),
    start: vi.fn(async (input: AuthorizedDevenvInput) => ({ sessionUuid: input.grant.sessionUuid, status: "starting" as const })),
    adopt: vi.fn(async () => true), revoke: vi.fn(async () => true), cleanupFailed: vi.fn(), now: () => 1000, sessionId: () => session,
  };
}
describe("bounded DevEnv issuance", () => {
  it("recognizes only the exact start endpoint", () => {
    expect(isDevenvStart("POST", "/v1/customer/devenv/")).toBe(true);
    expect(isDevenvStart("POST", "/v1/customer/devenv/stop")).toBe(false);
  });
  it("prepares, mints, starts, then adopts with generation", async () => {
    const d = deps();
    expect((await relayAuthorizedDevenvStart(body(), tenant, d)).status).toBe(201);
    expect(d.prepare).toHaveBeenCalledWith(session, tenant, "7");
    expect(d.mint).toHaveBeenCalledWith(session);
    expect(d.start.mock.calls[0][0].grant.lifecycleGeneration).toBe("7");
    expect(d.adopt).toHaveBeenCalledWith(session, patId);
    expect(d.revoke).not.toHaveBeenCalled();
  });
  it("rejects unknown input before credential issuance", async () => {
    const d = deps();
    const response = await relayAuthorizedDevenvStart(new Request("https://api.invalid/v1/devenv", { method: "POST", body: JSON.stringify({ workspace_name: "w", token: "secret" }) }), tenant, d);
    expect(response.status).toBe(400);
    expect(d.mint).not.toHaveBeenCalled();
  });
  it("revokes when start acknowledgement is not accepted", async () => {
    const d = deps(); d.start.mockResolvedValue({ sessionUuid: "wrong", status: "starting" });
    expect((await relayAuthorizedDevenvStart(body(), tenant, d)).status).toBe(503);
    expect(d.revoke).toHaveBeenCalledWith(session);
  });
  it("binds and abandons a reservation when start fails", async () => {
    const d = deps();
    d.prepareCompute.mockResolvedValue(session);
    d.start.mockRejectedValue(new Error("runner unavailable"));
    expect((await relayAuthorizedDevenvStart(body(), tenant, d)).status).toBe(503);
    expect(d.start.mock.calls[0][0].grant.computeReservationId).toBe(session);
    expect(d.abandonCompute).toHaveBeenCalledWith(session);
    expect(d.revoke).toHaveBeenCalledWith(session);
  });
});
