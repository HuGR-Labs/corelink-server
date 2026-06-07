/**
 * Test setup file — provides Web API stubs for Node.js test environment.
 *
 * Node.js ≥22 includes most Web APIs natively (fetch, crypto, Request, Response,
 * Headers, URL, structuredClone, etc.). This file provides any remaining stubs
 * and configures the test environment.
 */

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
