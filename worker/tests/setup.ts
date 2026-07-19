/**
 * Test setup file — provides Web API stubs for Node.js test environment.
 *
 * Node.js ≥22 includes most Web APIs natively (fetch, crypto, Request, Response,
 * Headers, URL, structuredClone, etc.). This file provides any remaining stubs
 * and configures the test environment.
 */

import { beforeEach } from "vitest";
import { __resetPatVerifyCacheForTest } from "../src/lib/pat_verify_cache.js";

// Node.js v18+ has global fetch, crypto, Request, Response, Headers, URL natively.
// Node.js v22.17.1 is installed — no additional polyfills needed.

// Ensure globalThis.crypto.randomUUID is available (Node.js 22 has it)
if (typeof globalThis.crypto === "undefined") {
  const { webcrypto } = await import("node:crypto");
  (globalThis as unknown as { crypto: typeof webcrypto }).crypto = webcrypto;
}

// `crypto.subtle.timingSafeEqual` is a Cloudflare Workers runtime extension and
// is absent from Node's webcrypto. Polyfill it so tests that exercise the
// internal-auth constant-time verify path can run under the Node vitest pool.
{
  const subtle = globalThis.crypto.subtle as unknown as {
    timingSafeEqual?: (a: ArrayBufferView, b: ArrayBufferView) => boolean;
  };
  if (typeof subtle.timingSafeEqual !== "function") {
    subtle.timingSafeEqual = (a: ArrayBufferView, b: ArrayBufferView): boolean => {
      const ua = new Uint8Array(a.buffer, a.byteOffset, a.byteLength);
      const ub = new Uint8Array(b.buffer, b.byteOffset, b.byteLength);
      if (ua.length !== ub.length) return false;
      let diff = 0;
      for (let i = 0; i < ua.length; i++) diff |= ua[i] ^ ub[i];
      return diff === 0;
    };
  }
}

// ──────────────────────────────────────────────────────────────────────────────
// Shared test PAT signing key + minting helper (House #2 — honest auth harness).
//
// WHY THIS EXISTS: the native plane (extractAuth in src/index.ts) fails CLOSED
// with a 503 when PAT_SIGNING_KEY is absent or < 64 hex chars (< 32 bytes). For
// a long time the unit-test env left this key UNSET, so every PAT-gated test
// short-circuited to 503 BEFORE reaching any auth/route logic — a 503 (misconfig)
// is indistinguishable from a real 401 (bad/forged/expired PAT), and "green"
// tests were passing on the misconfig, not on the logic they claimed to cover
// (test theater, adversarial-audit finding).
//
// The fix: ship a FIXED, valid test signing key here (64 hex chars = 32 bytes,
// the minimum extractAuth accepts) and mint canonical PATs whose HMAC-SHA256 sig
// verifies under it, so PAT-gated tests reach the REAL auth/route logic. The
// fail-closed (no-key ⇒ 503) path is still covered — but now by an EXPLICIT
// negative test (tests/index.test.ts) instead of being the silent default.
//
// This key is a test fixture ONLY. It is never a real secret: production binds
// PAT_SIGNING_KEY as a write-only Cloudflare secret.
// ──────────────────────────────────────────────────────────────────────────────

/** Fixed 64-hex-char (32-byte) test signing key — meets extractAuth's ≥32-byte floor. */
export const TEST_PAT_SIGNING_KEY = "ab".repeat(32);

/** Canonical token_id used by the shared D1 mocks (16 Crockford-b32 chars). */
export const TEST_PAT_TOKEN_ID = "AAAAAAAAAAAAAAAA";

/** Canonical 43-char base64url random_secret segment. */
export const TEST_PAT_SECRET = "A".repeat(43);

/** Decode a hex string to bytes. */
function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

/** Encode bytes to base64url-no-pad (matches the Worker's `base64url`). */
function b64urlNoPad(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/**
 * Mint a canonical PAT whose HMAC-SHA256 sig verifies under `signingKeyHex`
 * using the SAME algorithm the Worker verifies with: a 128-bit (16-byte)
 * truncated HMAC-SHA256 over the preimage `<token_id>.<random_secret>`.
 *
 * Defaults reproduce the canonical fixture (TEST_PAT_TOKEN_ID / TEST_PAT_SECRET
 * signed by TEST_PAT_SIGNING_KEY). Override to mint mismatched/foreign tokens.
 */
export async function mintTestPat(opts?: {
  signingKeyHex?: string;
  tokenId?: string;
  secret?: string;
}): Promise<string> {
  const signingKeyHex = opts?.signingKeyHex ?? TEST_PAT_SIGNING_KEY;
  const tokenId = opts?.tokenId ?? TEST_PAT_TOKEN_ID;
  const secret = opts?.secret ?? TEST_PAT_SECRET;
  const preimage = new TextEncoder().encode(`${tokenId}.${secret}`);
  const key = await crypto.subtle.importKey(
    "raw",
    hexToBytes(signingKeyHex),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const macBuf = await crypto.subtle.sign("HMAC", key, preimage);
  const sig16 = new Uint8Array(macBuf, 0, 16); // 128-bit truncated MAC
  return `corelink_pat_${tokenId}.${secret}.${b64urlNoPad(sig16)}`;
}

// ──────────────────────────────────────────────────────────────────────────────
// Per-case isolation of the per-isolate PAT-verify cache (perf #99).
//
// `extractAuth` now serves the `pat` row from a module-level, per-isolate cache
// (src/lib/pat_verify_cache.ts) keyed by token_id. In PRODUCTION a token_id is a
// permanent identity mapping to exactly ONE immutable row, so this is correct.
// In TESTS, many cases reuse the SAME fixture token_id while supplying DIFFERENT
// synthetic D1 rows (e.g. runner_job_header_forward.test.ts varies
// runner_job_ac_key per case) — so a row cached by one case would leak into the
// next within the 5 s TTL. Clear the singleton before every test so each case
// starts from a cold cache (mirrors how tenant_suspend_gate.test.ts resets its
// own cache; done globally here so no current or future test file can silently
// collide). This does NOT weaken the perf-win test — that asserts across two
// requests INSIDE one case, and beforeEach runs only BETWEEN cases.
// ──────────────────────────────────────────────────────────────────────────────
beforeEach(() => {
  __resetPatVerifyCacheForTest();
});
