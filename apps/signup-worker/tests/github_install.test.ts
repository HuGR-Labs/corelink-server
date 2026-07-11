import { afterEach, describe, expect, it, vi } from "vitest";

import {
  INSTALL_STATE_TTL_MS,
  signInstallState,
  verifyInstallState,
} from "../src/webhooks/github_install_state.js";
import {
  exchangeOAuthCode,
  mintAppJwt,
  userControlsInstallation,
} from "../src/webhooks/github_install_callback.js";

const KEY = "test-install-state-signing-key-0123456789";
const TENANT = "11111111-1111-4111-8111-111111111111";
const NOW = 1_800_000_000_000;

describe("install state sign/verify", () => {
  it("round-trips the tenant id within the TTL", async () => {
    const state = await signInstallState(TENANT, KEY, NOW);
    const v = await verifyInstallState(state, KEY, NOW + 1000);
    expect(v).toEqual({ tenantId: TENANT });
  });

  it("rejects a tampered signature", async () => {
    const state = await signInstallState(TENANT, KEY, NOW);
    const tampered = `${state.slice(0, -1)}${state.endsWith("a") ? "b" : "a"}`;
    expect(await verifyInstallState(tampered, KEY, NOW + 1000)).toBeNull();
  });

  it("rejects a swapped tenant id (signature no longer matches)", async () => {
    const state = await signInstallState(TENANT, KEY, NOW);
    const forged = state.replace(TENANT, "22222222-2222-4222-8222-222222222222");
    expect(await verifyInstallState(forged, KEY, NOW + 1000)).toBeNull();
  });

  it("rejects after expiry", async () => {
    const state = await signInstallState(TENANT, KEY, NOW);
    expect(await verifyInstallState(state, KEY, NOW + INSTALL_STATE_TTL_MS + 1)).toBeNull();
  });

  it("rejects a wrong signing key", async () => {
    const state = await signInstallState(TENANT, KEY, NOW);
    expect(await verifyInstallState(state, "different-key", NOW + 1000)).toBeNull();
  });

  it("rejects malformed input", async () => {
    expect(await verifyInstallState("garbage", KEY, NOW)).toBeNull();
    expect(await verifyInstallState("a.b", KEY, NOW)).toBeNull();
    expect(await verifyInstallState("a.notanumber.c", KEY, NOW)).toBeNull();
  });
});

/** Wrap raw PKCS#8 DER bytes into a PEM string (what the secret would hold). */
function pkcs8Pem(der: ArrayBuffer): string {
  const bytes = new Uint8Array(der);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  const b64 = btoa(bin).replace(/(.{64})/g, "$1\n");
  return `-----BEGIN PRIVATE KEY-----\n${b64}\n-----END PRIVATE KEY-----\n`;
}

function b64urlToBytes(s: string): Uint8Array {
  const b64 = s.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat((4 - (s.length % 4)) % 4);
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

describe("App JWT (RS256)", () => {
  it("produces a JWT whose signature verifies against the public key", async () => {
    const kp = (await crypto.subtle.generateKey(
      {
        name: "RSASSA-PKCS1-v1_5",
        modulusLength: 2048,
        publicExponent: new Uint8Array([1, 0, 1]),
        hash: "SHA-256",
      },
      true,
      ["sign", "verify"],
    )) as CryptoKeyPair;

    const pem = pkcs8Pem(await crypto.subtle.exportKey("pkcs8", kp.privateKey));
    const jwt = await mintAppJwt("424242", pem, NOW);

    const parts = jwt.split(".");
    expect(parts).toHaveLength(3);
    const [headerB64, payloadB64, sigB64] = parts as [string, string, string];

    // Verify the RS256 signature over `${header}.${payload}` with the public key.
    const signingInput = new TextEncoder().encode(`${headerB64}.${payloadB64}`);
    const ok = await crypto.subtle.verify(
      "RSASSA-PKCS1-v1_5",
      kp.publicKey,
      b64urlToBytes(sigB64),
      signingInput,
    );
    expect(ok).toBe(true);

    // Claims: iss = app id, exp within GitHub's 10-minute cap, iat backdated.
    const payload = JSON.parse(new TextDecoder().decode(b64urlToBytes(payloadB64))) as {
      iss: string;
      iat: number;
      exp: number;
    };
    expect(payload.iss).toBe("424242");
    const nowS = Math.floor(NOW / 1000);
    expect(payload.iat).toBeLessThan(nowS); // backdated for clock skew
    expect(payload.exp - payload.iat).toBeLessThanOrEqual(10 * 60); // <= GitHub cap
    expect(payload.exp).toBeGreaterThan(nowS);
  });
});

/** Build a minimal `Response`-like stub for the mocked fetch. */
function jsonResp(status: number, body: unknown): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
  } as unknown as Response;
}

describe("install ownership proof (OAuth) — the public-flip isolation gate", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("exchangeOAuthCode returns the user access token on success", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => {
        expect(url).toBe("https://github.com/login/oauth/access_token");
        return jsonResp(200, { access_token: "ghu_usertoken123" });
      }),
    );
    expect(await exchangeOAuthCode("Iv23liXXX", "secret", "code123")).toBe("ghu_usertoken123");
  });

  it("exchangeOAuthCode fails CLOSED (null) on a non-ok response", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => jsonResp(401, { error: "bad_verification_code" })));
    expect(await exchangeOAuthCode("Iv23liXXX", "secret", "bad")).toBeNull();
  });

  it("exchangeOAuthCode fails CLOSED (null) when no access_token is returned", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => jsonResp(200, { error: "no_token" })));
    expect(await exchangeOAuthCode("Iv23liXXX", "secret", "code")).toBeNull();
  });

  it("userControlsInstallation is TRUE when the installation is in the user's list", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResp(200, { installations: [{ id: 999 }, { id: 144561227 }] })),
    );
    expect(await userControlsInstallation("ghu_tok", "144561227")).toBe(true);
  });

  it("userControlsInstallation is FALSE (hijack blocked) when the installation is NOT the caller's", async () => {
    // The attacker presents a victim's installation id they do not administer.
    vi.stubGlobal("fetch", vi.fn(async () => jsonResp(200, { installations: [{ id: 999 }] })));
    expect(await userControlsInstallation("ghu_attacker", "144561227")).toBe(false);
  });

  it("userControlsInstallation fails CLOSED (false) on a non-ok response", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => jsonResp(403, {})));
    expect(await userControlsInstallation("ghu_revoked", "144561227")).toBe(false);
  });

  it("userControlsInstallation paginates and finds an installation on a later page", async () => {
    const calls: string[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => {
        calls.push(url);
        // page 2 holds the target; page 1 is a full page of 100 non-matching ids
        // (a full page forces the loop to fetch the next). NB: match "&page=2"
        // precisely — "per_page=100" contains the substring "page=1".
        if (url.includes("&page=2")) {
          return jsonResp(200, { installations: [{ id: 144561227 }] });
        }
        return jsonResp(200, { installations: Array.from({ length: 100 }, (_, i) => ({ id: i })) });
      }),
    );
    expect(await userControlsInstallation("ghu_tok", "144561227")).toBe(true);
    expect(calls.some((u) => u.includes("page=2"))).toBe(true);
  });
});
