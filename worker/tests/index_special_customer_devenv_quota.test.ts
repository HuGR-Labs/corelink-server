import { describe, expect, it, vi } from "vitest";

vi.mock("../src/lib/clerk_auth.js", () => ({
  verifyClerkSessionAndResolveTenant: vi.fn(async () => ({ ok: true, tenantId: "tenant-1", role: "member" })),
}));
vi.mock("../src/lib/devenv_relay.js", () => ({
  DEVENV_MAX_TTL_SECONDS: 300,
  isDevenvStart: (method: string, path: string) => method === "POST" && path === "/v1/devenv",
  mayAccessDevenv: () => true,
  relayAuthorizedDevenvStart: vi.fn(),
}));
vi.mock("../src/index_auth.js", () => ({
  applyCors: (response: Response) => response,
  extractAuth: vi.fn(),
  parsePat: () => null,
  stripClientTrustHeaders: () => {},
  MFA_FVA_FRESH_MAX_MINUTES: 15,
}));

import { handleSpecialCustomerRoute } from "../src/index_special_customer.js";
import { checkDevenvQuota } from "../src/lib/devenv_guard.js";

vi.mock("../src/lib/devenv_guard.js", () => ({ checkDevenvQuota: vi.fn() }));

const route = (pathSuffix: string) => ({ tenantId: "", pathSuffix, routeKind: "devenv_v1" as const });
const env = (db: unknown) => ({ CONFIG_DB: db, RUNNER_DEVENV_DO: {} } as never);

describe("DevEnv quota gates every route before the DO", () => {
  it.each(["GET", "POST"])("blocks %s status/use when entitlement is denied", async (method) => {
    vi.mocked(checkDevenvQuota).mockResolvedValueOnce({ allowed: false, reason: "no entitlement" });
    const response = await handleSpecialCustomerRoute(
      new Request("https://corelink.test/v1/devenv/status", { method }),
      env({}),
      "req-denied",
      route("/v1/devenv/status"),
    );
    expect(response?.status).toBe(403);
  });

  it("fails closed on entitlement D1 errors before the DO", async () => {
    vi.mocked(checkDevenvQuota).mockRejectedValueOnce(new Error("D1 unavailable"));
    const response = await handleSpecialCustomerRoute(
      new Request("https://corelink.test/v1/devenv/status"),
      env({}),
      "req-error",
      route("/v1/devenv/status"),
    );
    expect(response?.status).toBe(503);
  });

  it("keeps the start route behind the same entitlement gate", async () => {
    vi.mocked(checkDevenvQuota).mockResolvedValueOnce({ allowed: false });
    const response = await handleSpecialCustomerRoute(
      new Request("https://corelink.test/v1/devenv", { method: "POST" }),
      env({}),
      "req-start",
      route("/v1/devenv"),
    );
    expect(response?.status).toBe(403);
  });
});
