import { describe, expect, it } from "vitest";

import { INSTALL_STATE_TTL_MS, signInstallState } from "./install-state";

const KEY = "test-install-state-signing-key-0123456789";
const TENANT = "11111111-1111-4111-8111-111111111111";
const NOW = 1_800_000_000_000;

/**
 * Inline reimplementation of the signup-worker `verifyInstallState`
 * (`apps/signup-worker/src/webhooks/github_install_state.ts`). The admin-ui
 * MINT and the signup-worker VERIFY live in separate deployables; this test is
 * the guard that they agree byte-for-byte on the shared HMAC format. If either
 * side drifts (algorithm, preimage, hex encoding, TTL semantics), the
 * round-trip below breaks.
 */
async function verify(
  state: string,
  key: string,
  nowMs: number,
): Promise<{ tenantId: string } | null> {
  const parts = state.split(".");
  if (parts.length !== 3) return null;
  const [tenantId, expStr, sig] = parts;
  if (!tenantId || !expStr || !sig) return null;
  const exp = Number(expStr);
  if (!Number.isFinite(exp)) return null;
  const enc = new TextEncoder();
  const cryptoKey = await crypto.subtle.importKey(
    "raw",
    enc.encode(key),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const raw = new Uint8Array(
    await crypto.subtle.sign("HMAC", cryptoKey, enc.encode(`${tenantId}.${expStr}`)),
  );
  const expected = Array.from(raw)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
  if (sig !== expected) return null;
  if (nowMs > exp) return null;
  return { tenantId };
}

describe("admin-ui install-state mint ↔ signup-worker verify", () => {
  it("round-trips the tenant id within the TTL", async () => {
    const state = await signInstallState(TENANT, KEY, NOW);
    expect(await verify(state, KEY, NOW + 1000)).toEqual({ tenantId: TENANT });
  });

  it("emits the exact <tenant>.<exp_ms>.<hex64> shape", async () => {
    const state = await signInstallState(TENANT, KEY, NOW);
    const parts = state.split(".");
    expect(parts).toHaveLength(3);
    expect(parts[0]).toBe(TENANT);
    expect(Number(parts[1])).toBe(NOW + INSTALL_STATE_TTL_MS);
    expect(parts[2]).toMatch(/^[0-9a-f]{64}$/);
  });

  it("verify rejects after expiry", async () => {
    const state = await signInstallState(TENANT, KEY, NOW);
    expect(await verify(state, KEY, NOW + INSTALL_STATE_TTL_MS + 1)).toBeNull();
  });

  it("verify rejects a wrong signing key", async () => {
    const state = await signInstallState(TENANT, KEY, NOW);
    expect(await verify(state, "different-key", NOW + 1000)).toBeNull();
  });

  it("verify rejects a tampered tenant id (signature no longer matches)", async () => {
    const state = await signInstallState(TENANT, KEY, NOW);
    const forged = state.replace(TENANT, "22222222-2222-4222-8222-222222222222");
    expect(await verify(forged, KEY, NOW + 1000)).toBeNull();
  });
});
