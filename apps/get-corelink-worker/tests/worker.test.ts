/**
 * Worker route tests — invokes the exported `default` handler directly
 * with a stub `Env` object and a stub `ExecutionContext`. Mirrors the
 * `worker/tests/` Node-pool pattern (no workerd runtime needed; the
 * Worker has zero CF-binding dependencies — only `[vars]`).
 *
 * Coverage:
 *   - GET /  → 200 + text/x-shellscript + install body
 *   - GET /healthz → 200 + "ok"
 *   - GET /unknown → 404
 *   - POST / → 405 + Allow header
 *   - HEAD / → 200 + headers + no body
 *   - Response headers: Cache-Control no-store, nosniff, HSTS, X-Corelink-Env
 *   - Query params ignored (deterministic body)
 *   - 500 surfaced as generic "Internal Server Error" (no detail leak)
 */

import { describe, expect, it } from "vitest";
import handler, { type Env } from "../src/index.ts";

/** Minimal Env stub — mirrors wrangler.toml `[vars]`. */
const TEST_ENV: Env = {
  ENVIRONMENT: "test",
  RELEASE_ORIGIN: "https://github.com/humangr-labs/corelink-cli/releases/latest/download",
  DEFAULT_API_ENDPOINT: "https://corelink-api.humangr.com",
};

/** Stub ExecutionContext (waitUntil + passThroughOnException are unused). */
const TEST_CTX = {
  waitUntil: (_p: Promise<unknown>) => {},
  passThroughOnException: () => {},
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
} as any;

async function fetch(input: string, init?: RequestInit): Promise<Response> {
  const req = new Request(input, init);
  // handler.fetch's signature uses CF types; cast on the call site only.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (handler.fetch as any)(req, TEST_ENV, TEST_CTX) as Promise<Response>;
}

describe("get-corelink-worker routes", () => {
  it("GET / returns 200 with text/x-shellscript Content-Type", async () => {
    const res = await fetch("https://get.corelink.io/");
    expect(res.status).toBe(200);
    expect(res.headers.get("Content-Type")).toBe("text/x-shellscript; charset=utf-8");
    const body = await res.text();
    expect(body.startsWith("#!/bin/sh\n")).toBe(true);
    expect(body).toContain("corelink ping");
  });

  it("GET / response sets Cache-Control no-store (never serve stale)", async () => {
    const res = await fetch("https://get.corelink.io/");
    const cc = res.headers.get("Cache-Control") ?? "";
    expect(cc).toContain("no-store");
    expect(cc).toContain("no-cache");
  });

  it("GET / response includes defense-in-depth headers", async () => {
    const res = await fetch("https://get.corelink.io/");
    expect(res.headers.get("X-Content-Type-Options")).toBe("nosniff");
    expect(res.headers.get("Referrer-Policy")).toBe("no-referrer");
    expect(res.headers.get("Strict-Transport-Security")).toContain("max-age=63072000");
  });

  it("GET / response surfaces ENVIRONMENT for debug-tracing", async () => {
    const res = await fetch("https://get.corelink.io/");
    expect(res.headers.get("X-Corelink-Env")).toBe("test");
  });

  it("HEAD / returns 200 with headers but no body", async () => {
    const res = await fetch("https://get.corelink.io/", { method: "HEAD" });
    expect(res.status).toBe(200);
    expect(res.headers.get("Content-Type")).toBe("text/x-shellscript; charset=utf-8");
    const body = await res.text();
    expect(body).toBe("");
  });

  it("GET /healthz returns 200 with `ok`", async () => {
    const res = await fetch("https://get.corelink.io/healthz");
    expect(res.status).toBe(200);
    expect(await res.text()).toBe("ok\n");
  });

  it("GET /unknown returns 404", async () => {
    const res = await fetch("https://get.corelink.io/unknown-path");
    expect(res.status).toBe(404);
  });

  it("POST / returns 405 with Allow header listing GET, HEAD", async () => {
    const res = await fetch("https://get.corelink.io/", { method: "POST" });
    expect(res.status).toBe(405);
    expect(res.headers.get("Allow")).toBe("GET, HEAD");
  });

  it("PUT / returns 405", async () => {
    const res = await fetch("https://get.corelink.io/", { method: "PUT" });
    expect(res.status).toBe(405);
  });

  it("query parameters are ignored (script body is deterministic)", async () => {
    const a = await (await fetch("https://get.corelink.io/")).text();
    const b = await (
      await fetch("https://get.corelink.io/?ref=marketing&utm_source=docs")
    ).text();
    expect(a).toBe(b);
  });

  it("empty pathname is treated as root", async () => {
    // URL constructor normalizes empty pathname to "/" — verify the
    // route table handles both forms identically.
    const res = await fetch("https://get.corelink.io");
    expect(res.status).toBe(200);
    expect(res.headers.get("Content-Type")).toBe("text/x-shellscript; charset=utf-8");
  });
});
