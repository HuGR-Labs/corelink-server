import { beforeAll, describe, expect, it } from "vitest";
import { issueComputeGrant } from "../src/lib/compute_budget_grant.js";

const input = { tenantId: "11111111-1111-4111-8111-111111111111", workloadKind: "devenv" as const, workloadId: "session-1", reservationId: "22222222-2222-4222-8222-222222222222", maxVcpuHours: 1, vcpuCount: 4, maximumWallMs: 28_800_000 };
const now = Date.parse("2026-09-14T12:00:00.000Z");
let env: { COMPUTE_GRANT_SIGNING_KEY: string; COMPUTE_GRANT_KEY_ID: string };
let publicKey: CryptoKey;
function b64(bytes: ArrayBuffer): string { let s = ""; for (const b of new Uint8Array(bytes)) s += String.fromCharCode(b); return btoa(s); }
function decode(value: string): Uint8Array { const s = value.replaceAll("-", "+").replaceAll("_", "/") + "===".slice((value.length + 3) % 4); return Uint8Array.from(atob(s), (c) => c.charCodeAt(0)); }
beforeAll(async () => {
  const keys = await crypto.subtle.generateKey({ name: "Ed25519", namedCurve: "Ed25519" }, true, ["sign", "verify"]);
  if (!("privateKey" in keys)) throw new Error("unexpected key shape");
  publicKey = keys.publicKey;
  env = { COMPUTE_GRANT_SIGNING_KEY: b64(await crypto.subtle.exportKey("pkcs8", keys.privateKey)), COMPUTE_GRANT_KEY_ID: "compute-v1" };
});
describe("compute grant validation", () => {
  it("emits a verifiable fixed-bound DevEnv grant", async () => {
    const token = await issueComputeGrant(env, input, now);
    const [encoded, signature] = token.split(".");
    const payloadBytes = decode(encoded);
    const payload = JSON.parse(new TextDecoder().decode(payloadBytes)) as Record<string, unknown>;
    expect(payload).toMatchObject({ v: 1, key_id: "compute-v1", tenant_id: input.tenantId, workload_kind: "devenv", workload_id: "session-1", reservation_id: input.reservationId, period_key: 202609, ceiling_vcpu_ms: "3600000", vcpu_count: 4, maximum_wall_ms: 28_800_000, issued_at_ms: now, expires_at_ms: now + 90_000 });
    await expect(crypto.subtle.verify({ name: "Ed25519" }, publicKey, decode(signature), payloadBytes)).resolves.toBe(true);
  });
  it("fails closed for malformed issuer material", async () => {
    await expect(issueComputeGrant({ COMPUTE_GRANT_SIGNING_KEY: "bad", COMPUTE_GRANT_KEY_ID: "compute-v1" }, input, now)).rejects.toThrow("invalid compute grant request");
  });
  it.each([["zero ceiling", 0], ["fractional ceiling", 1.5], ["bad tenant", "bad"]])("rejects %s", async (_label, value) => {
    const modified = typeof value === "string" ? { ...input, tenantId: value } : { ...input, maxVcpuHours: value };
    await expect(issueComputeGrant({ COMPUTE_GRANT_SIGNING_KEY: "bad", COMPUTE_GRANT_KEY_ID: "compute-v1" }, modified, Date.now())).rejects.toThrow();
  });
});
