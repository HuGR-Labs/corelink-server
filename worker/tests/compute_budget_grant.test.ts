import { beforeAll, describe, expect, it } from "vitest";
import { issueComputeGrant } from "../src/lib/compute_budget_grant.js";
import { prepareDevenvCompute } from "../src/lib/devenv_compute.js";

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

describe("prepareDevenvCompute", () => {
  function deps(row: Record<string, unknown> | null = { max_concurrency: 1, max_vcpu_h: 240 }) {
    const calls: unknown[] = [];
    const db = { prepare: (sql: string) => ({ bind: (tenant: string) => ({ first: async () => { calls.push([sql, tenant]); return row; } }) }) } as never;
    const rpc = { prepareAuthorizedCompute: async (binding: unknown) => { calls.push(binding); } };
    return { env: { CONFIG_DB: db, ...env }, rpc, calls };
  }
  it("passes a tenant/session-bound fixed-shape token to the RPC", async () => {
    const d = deps();
    await expect(prepareDevenvCompute(d.env, d.rpc, input.tenantId, input.reservationId, now)).resolves.toBe(input.reservationId);
    const binding = d.calls[1] as Record<string, unknown>;
    expect(binding).toMatchObject({ tenantId: input.tenantId, reservationId: input.reservationId, workloadId: input.reservationId, workloadKind: "devenv", vcpuCount: 4, maximumWallMs: 28_800_000 });
    const payload = JSON.parse(new TextDecoder().decode(decode(String(binding.token).split(".")[0]))) as Record<string, unknown>;
    expect(payload.tenant_id).toBe(input.tenantId);
    expect(payload.reservation_id).toBe(input.reservationId);
  });
  it("fails closed for missing entitlement or ceiling", async () => {
    const noEntitlement = deps(null);
    await expect(prepareDevenvCompute(noEntitlement.env, noEntitlement.rpc, input.tenantId, input.reservationId, now)).rejects.toThrow("invalid DevEnv runners entitlement");
    const noCeiling = deps({ max_concurrency: 1, max_vcpu_h: null });
    await expect(prepareDevenvCompute(noCeiling.env, noCeiling.rpc, input.tenantId, input.reservationId, now)).resolves.toBeNull();
    expect(noCeiling.calls).toHaveLength(1);
  });
  it("propagates RPC rejection for relay-owned compensation", async () => {
    const d = deps();
    d.rpc.prepareAuthorizedCompute = async () => { throw new Error("runner unavailable"); };
    await expect(prepareDevenvCompute(d.env, d.rpc, input.tenantId, input.reservationId, now)).rejects.toThrow("runner unavailable");
  });
});
