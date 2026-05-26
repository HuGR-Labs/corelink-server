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
