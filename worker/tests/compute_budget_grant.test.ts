import { describe, expect, it } from "vitest";
import { issueComputeGrant } from "../src/lib/compute_budget_grant.js";

const env = { COMPUTE_GRANT_SIGNING_KEY: "%%not-base64%%", COMPUTE_GRANT_KEY_ID: "compute-v1" };
const input = { tenantId: "00000000-0000-4000-8000-000000000001", workloadKind: "devenv" as const, workloadId: "00000000-0000-4000-8000-000000000002", reservationId: "00000000-0000-4000-8000-000000000002", maxVcpuHours: 1, vcpuCount: 4, maximumWallMs: 28_800_000 };
describe("compute grant validation", () => {
  it("rejects malformed issuer key material", async () => { await expect(issueComputeGrant(env, input, Date.now())).rejects.toThrow("invalid compute grant request"); });
  it("rejects invalid dates, ceilings, and identifiers", async () => {
    await expect(issueComputeGrant(env, { ...input, maxVcpuHours: 0 }, Date.now())).rejects.toThrow();
    await expect(issueComputeGrant(env, { ...input, tenantId: "bad" }, Date.now())).rejects.toThrow();
    await expect(issueComputeGrant(env, input, -1)).rejects.toThrow();
  });
});
