/**
 * Unit tests for the /v1/onboarding/* Clerk edge-verification bridge (GAP-5).
 *
 * The browser presents a Clerk SESSION JWT (not a CoreLink PAT). The worker
 * verifies it at the edge, resolves the CoreLink tenant from the verified Clerk
 * user id (CONFIG_DB.tenant.clerk_user_id), and forwards to the tenant DO with
 * the internal-auth contract the container's tier_select route requires —
 * stripping any client-supplied trust headers first (CRITICAL-2).
 *
 * @clerk/backend's verifyToken is mocked per-test; the DO stub captures the
 * forwarded request so we can assert the exact headers the edge injects.
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";

// Mock the Clerk edge verifier (hoisted) — controlled per test.
vi.mock("@clerk/backend", () => ({ verifyToken: vi.fn() }));
import { verifyToken } from "@clerk/backend";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

const mockVerifyToken = vi.mocked(verifyToken);

const INTERNAL_KEY = "test-internal-auth-key-0123456789"; // ≥16 chars
const CLERK_SECRET = "sk_test_clerk_secret";

function makeCtx(): ExecutionContext {
  return {
    waitUntil: (_p: Promise<unknown>) => {},
    passThroughOnException: () => {},
  } as unknown as ExecutionContext;
}

/** CONFIG_DB mock: returns `{ tenant_id }` for a known clerk_user_id, else null. */
function makeConfigDb(clerkUserToTenant: Map<string, string>): D1Database {
  return {
    prepare: (_sql: string) => ({
      bind: (...args: unknown[]) => ({
        first: async <T>() => {
          const clerkUserId = args[0] as string;
          const tenantId = clerkUserToTenant.get(clerkUserId);
          return (tenantId ? { tenant_id: tenantId } : null) as T | null;
        },
      }),
    }),
  } as unknown as D1Database;
}

/** CORELINK_SERVER DO namespace that CAPTURES the forwarded request. */
function makeCaptureNamespace(captured: { req?: Request }): DurableObjectNamespace {
  const stub = {
    fetch: async (req: Request): Promise<Response> => {
      captured.req = req;
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    },
  };
  const namespace = {
    idFromName: (_n: string) => ({ toString: () => "stub-id" }),
    get: (_id: unknown) => stub,
    idFromString: (_s: string) => ({ toString: () => "stub-id" }),
    newUniqueId: () => ({ toString: () => "stub-unique" }),
    jurisdiction: (_j: string) => namespace,
  } as unknown as DurableObjectNamespace;
  return namespace;
}

function makeOnbEnv(opts: {
  captured: { req?: Request };
  clerkUserToTenant: Map<string, string>;
  withClerkSecret?: boolean;
  withInternalKey?: boolean;
}): Env {
  return {
    CORELINK_SERVER: makeCaptureNamespace(opts.captured),
    ENVIRONMENT: "test",
    CONFIG_DB: makeConfigDb(opts.clerkUserToTenant),
    CLERK_SECRET_KEY: opts.withClerkSecret === false ? undefined : CLERK_SECRET,
    CORELINK_INTERNAL_AUTH_KEY: opts.withInternalKey === false ? undefined : INTERNAL_KEY,
  } as Env;
}

async function onbFetch(env: Env, headers: Record<string, string>): Promise<Response> {
  const req = new Request("http://localhost/v1/onboarding/tier-select", {
    method: "POST",
    headers: { "Content-Type": "application/json", ...headers },
    body: JSON.stringify({
      tier: "starter",
      success_url: "https://corelink-admin.humangr.com/en/upgraded",
      cancel_url: "https://corelink-admin.humangr.com/en/pricing",
    }),
  });
  return workerHandler.fetch!(req, env, makeCtx());
}

describe("/v1/onboarding/* — Clerk edge-verification bridge (GAP-5)", () => {
  beforeEach(() => {
    mockVerifyToken.mockReset();
  });

  it("verifies Clerk, resolves tenant, injects internal-auth, forwards to the DO", async () => {
    mockVerifyToken.mockResolvedValue({ sub: "user_abc" } as never);
    const captured: { req?: Request } = {};
    const env = makeOnbEnv({ captured, clerkUserToTenant: new Map([["user_abc", "acme-default"]]) });

    const resp = await onbFetch(env, { Authorization: "Bearer clerk.jwt.token" });

    expect(resp.status).toBe(200);
    expect(captured.req).toBeDefined();
    const h = captured.req!.headers;
    expect(h.get("x-corelink-internal-auth")).toBe(INTERNAL_KEY);
    expect(h.get("x-corelink-tenant-id")).toBe("acme-default");
    expect(h.get("x-corelink-route-kind")).toBe("onboarding");
    expect(h.get("x-corelink-token-prefix")).toBe("clerk");
  });

  it("STRIPS client-supplied internal-auth + tenant-id + Clerk JWT before forwarding (CRITICAL-2)", async () => {
    mockVerifyToken.mockResolvedValue({ sub: "user_abc" } as never);
    const captured: { req?: Request } = {};
    const env = makeOnbEnv({ captured, clerkUserToTenant: new Map([["user_abc", "acme-default"]]) });

    await onbFetch(env, {
      Authorization: "Bearer clerk.jwt.token",
      "x-corelink-internal-auth": "SPOOFED-KEY",
      "x-corelink-tenant-id": "victim-tenant",
    });

    const h = captured.req!.headers;
    // Spoofed trust headers replaced by server-trusted values — never passed through.
    expect(h.get("x-corelink-internal-auth")).toBe(INTERNAL_KEY);
    expect(h.get("x-corelink-internal-auth")).not.toBe("SPOOFED-KEY");
    expect(h.get("x-corelink-tenant-id")).toBe("acme-default");
    expect(h.get("x-corelink-tenant-id")).not.toBe("victim-tenant");
    // The Clerk JWT is not forwarded to the container (it uses internal-auth).
    expect(h.get("authorization")).toBeNull();
  });

  it("returns 401 on an invalid / expired Clerk token (never forwards)", async () => {
    mockVerifyToken.mockRejectedValue(new Error("token expired"));
    const captured: { req?: Request } = {};
    const env = makeOnbEnv({ captured, clerkUserToTenant: new Map() });

    const resp = await onbFetch(env, { Authorization: "Bearer bad.token" });

    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("returns 401 when no Bearer token is present", async () => {
    const captured: { req?: Request } = {};
    const env = makeOnbEnv({ captured, clerkUserToTenant: new Map() });

    const resp = await onbFetch(env, {});

    expect(resp.status).toBe(401);
    expect(captured.req).toBeUndefined();
  });

  it("returns 403 when the verified session has no CoreLink tenant", async () => {
    mockVerifyToken.mockResolvedValue({ sub: "user_no_tenant" } as never);
    const captured: { req?: Request } = {};
    const env = makeOnbEnv({ captured, clerkUserToTenant: new Map() }); // no mapping

    const resp = await onbFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
  });

  it("fail-CLOSED: 403 when CLERK_SECRET_KEY is unbound (no verification possible)", async () => {
    const captured: { req?: Request } = {};
    const env = makeOnbEnv({ captured, clerkUserToTenant: new Map(), withClerkSecret: false });

    const resp = await onbFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
  });

  it("fail-CLOSED: 403 when CORELINK_INTERNAL_AUTH_KEY is unbound (cannot authorize to container)", async () => {
    mockVerifyToken.mockResolvedValue({ sub: "user_abc" } as never);
    const captured: { req?: Request } = {};
    const env = makeOnbEnv({
      captured,
      clerkUserToTenant: new Map([["user_abc", "acme-default"]]),
      withInternalKey: false,
    });

    const resp = await onbFetch(env, { Authorization: "Bearer clerk.jwt" });

    expect(resp.status).toBe(403);
    expect(captured.req).toBeUndefined();
  });
});
