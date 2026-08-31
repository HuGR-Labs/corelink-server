/**
 * The public OpenAPI routes.
 *
 * These exist because for a while `GET /openapi.json` — the only API contract
 * CoreLink publishes — served the DevEnv sub-surface and nothing else. Verified
 * against production before the fix, not inferred from the source:
 *
 *   $ curl -s https://corelink-api.humangr.com/openapi.json
 *   200, 7707 bytes, info.title = "CoreLink DevEnv API", 8 paths,
 *   every one of them /v1/customer/devenv*
 *
 * The route had never served anything else: it was introduced by the DevEnv
 * package (#1432) and took the canonical public path with it. So the assertions
 * below are deliberately about IDENTITY (which contract comes back), not merely
 * about a 200 — a status-only test would have passed throughout the defect.
 *
 * The DevEnv spec now lives at its own path AND is gated on the same binding
 * its endpoints are gated on, so a published contract cannot outlive the
 * feature it describes.
 */

import { describe, it, expect } from "vitest";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

/** Minimal Env: no bindings at all — the shape a fresh environment has. */
function envWithoutDevenv(): Env {
  return {} as unknown as Env;
}

/** Env where the DevEnv DO binding exists (its presence is all that is read). */
function envWithDevenv(): Env {
  return { RUNNER_DEVENV_DO: {} } as unknown as Env;
}

async function get(path: string, env: Env): Promise<Response> {
  return workerHandler.fetch(
    new Request(`https://corelink-api.humangr.com${path}`, { method: "GET" }),
    env,
    { waitUntil: () => {}, passThroughOnException: () => {} } as unknown as ExecutionContext,
  );
}

describe("GET /openapi.json", () => {
  it("serves the CoreLink API contract, not the DevEnv sub-surface", async () => {
    const resp = await get("/openapi.json", envWithoutDevenv());
    expect(resp.status).toBe(200);
    expect(resp.headers.get("Content-Type")).toBe("application/json");

    const spec = (await resp.json()) as {
      openapi: string;
      info: { title: string };
      paths: Record<string, unknown>;
    };

    expect(spec.openapi).toMatch(/^3\.1/);

    // The regression, named: the published contract must not BE the DevEnv one.
    expect(spec.info.title).not.toContain("DevEnv");

    // ...and it must not merely be "some other spec": it has to carry the
    // product's actual surface. A title assertion alone would pass on an empty
    // document.
    expect(Object.keys(spec.paths).length).toBeGreaterThan(20);
    expect(spec.paths).toHaveProperty("/v1/cas/{tenant}/{hash}");
    expect(spec.paths).toHaveProperty("/v1/customer/keys");

    // No DevEnv path may leak into the general contract either.
    const devenvPaths = Object.keys(spec.paths).filter((p) => p.includes("devenv"));
    expect(devenvPaths).toEqual([]);
  });

  it("is served at the /v1 alias too", async () => {
    const resp = await get("/v1/openapi.json", envWithoutDevenv());
    expect(resp.status).toBe(200);
    const spec = (await resp.json()) as { info: { title: string } };
    expect(spec.info.title).not.toContain("DevEnv");
  });

  it("does not require authentication", async () => {
    // The contract is public by design; a 401 here would be a regression that
    // only an unauthenticated probe can see.
    const resp = await get("/openapi.json", envWithoutDevenv());
    expect(resp.status).not.toBe(401);
  });
});

describe("GET /openapi/devenv.json", () => {
  it("404s where DevEnv is not wired, rather than publishing a dead contract", async () => {
    const resp = await get("/openapi/devenv.json", envWithoutDevenv());
    expect(resp.status).toBe(404);
    const body = (await resp.json()) as { error: string; message: string };
    expect(body.error).toBe("NOT_FOUND");
    // The refusal must SAY why — "not found" alone sends nobody anywhere.
    expect(body.message).toContain("DevEnv");
  });

  it("serves the DevEnv spec where the binding exists", async () => {
    const resp = await get("/openapi/devenv.json", envWithDevenv());
    expect(resp.status).toBe(200);
    const spec = (await resp.json()) as {
      info: { title: string };
      paths: Record<string, unknown>;
    };
    expect(spec.info.title).toContain("DevEnv");
    expect(Object.keys(spec.paths).length).toBeGreaterThan(0);
    // Positive control on the gate itself: the two branches must actually
    // differ, or the 404 test above would be proving nothing.
    expect(Object.keys(spec.paths).every((p) => p.includes("devenv"))).toBe(true);
  });
});
