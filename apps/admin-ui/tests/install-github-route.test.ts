/**
 * GET /api/install/github — route handler tests.
 *
 * Regression cover for the binding-access defect (W9), the same class fixed in
 * `/api/welcome/stream`: the handler fell back to `globalThis.__env__` (the
 * `next-on-pages` convention) while admin-ui runs on `@opennextjs/cloudflare`,
 * whose bindings arrive via `getCloudflareContext()`. `__env__` is never
 * populated by that runtime, so a var bound ONLY as a Worker binding was
 * invisible and the route fell through to its fail-closed
 * `503 runner_install_not_configured` — the "Install GitHub App" button dead.
 *
 * The load-bearing test is "reads … from the Cloudflare context": it supplies
 * the vars ONLY on the Cloudflare context (process.env left clean), which is
 * exactly how a deployed `wrangler secret` / `[vars]` binding would appear.
 * Against the old `__env__` read it fails with 503.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Hoisted mocks — registered BEFORE the route module is imported.
const { mockGetCloudflareContext, mockAuth } = vi.hoisted(() => ({
  mockGetCloudflareContext: vi.fn(),
  mockAuth: vi.fn(),
}));

vi.mock("@opennextjs/cloudflare", () => ({
  getCloudflareContext: mockGetCloudflareContext,
}));

vi.mock("@clerk/nextjs/server", () => ({
  auth: mockAuth,
}));

// Import the route AFTER the mocks are registered.
const { GET } = await import("@/app/api/install/github/route");

const TENANT_ID = "11111111-2222-3333-4444-555555555555";
const SIGNING_KEY = "test-install-state-signing-key";
const APP_SLUG = "corelink-runners";

describe("GET /api/install/github", () => {
  beforeEach(() => {
    mockGetCloudflareContext.mockReset();
    mockAuth.mockReset();
    mockAuth.mockResolvedValue({ sessionClaims: { tenant_id: TENANT_ID } });
    // The Cloudflare context is the surface under test — keep process.env
    // clean so nothing masks it.
    delete process.env.INSTALL_STATE_SIGNING_KEY;
    delete process.env.GITHUB_APP_SLUG;
  });

  afterEach(() => {
    vi.clearAllMocks();
    delete process.env.INSTALL_STATE_SIGNING_KEY;
    delete process.env.GITHUB_APP_SLUG;
  });

  it("401s when there is no session tenant", async () => {
    mockAuth.mockResolvedValue({ sessionClaims: {} });
    mockGetCloudflareContext.mockResolvedValue({ env: {} });

    const res = await GET();

    expect(res.status).toBe(401);
    await expect(res.json()).resolves.toMatchObject({
      error: "unauthenticated",
    });
  });

  it("reads INSTALL_STATE_SIGNING_KEY + GITHUB_APP_SLUG from the Cloudflare context and redirects to GitHub", async () => {
    mockGetCloudflareContext.mockResolvedValue({
      env: {
        INSTALL_STATE_SIGNING_KEY: SIGNING_KEY,
        GITHUB_APP_SLUG: APP_SLUG,
      },
    });

    const res = await GET();

    // With the old `globalThis.__env__` read this is 503: the binding is
    // invisible and the route fails closed.
    expect(res.status).toBe(302);
    const location = res.headers.get("location") ?? "";
    expect(location).toContain(
      `https://github.com/apps/${APP_SLUG}/installations/new?state=`,
    );
    // The signed state binds the SERVER-resolved tenant.
    const state = decodeURIComponent(
      new URL(location).searchParams.get("state") ?? "",
    );
    expect(state.startsWith(`${TENANT_ID}.`)).toBe(true);
    expect(state.split(".")).toHaveLength(3);

    // Bindings must come from the opennextjs context, not `globalThis.__env__`.
    // The async overload is required so the call also resolves under `next dev`.
    expect(mockGetCloudflareContext).toHaveBeenCalledWith({ async: true });
  });

  it("still 503s fail-closed when neither source has the vars", async () => {
    mockGetCloudflareContext.mockResolvedValue({ env: {} });

    const res = await GET();

    expect(res.status).toBe(503);
    await expect(res.json()).resolves.toEqual({
      error: "runner_install_not_configured",
    });
  });

  it("503s (not 500) when the Cloudflare context is unavailable", async () => {
    mockGetCloudflareContext.mockRejectedValue(
      new Error("getCloudflareContext has been called without ..."),
    );

    const res = await GET();

    expect(res.status).toBe(503);
    await expect(res.json()).resolves.toEqual({
      error: "runner_install_not_configured",
    });
  });

  it("still honours process.env when the var is mirrored there", async () => {
    process.env.INSTALL_STATE_SIGNING_KEY = SIGNING_KEY;
    process.env.GITHUB_APP_SLUG = APP_SLUG;
    mockGetCloudflareContext.mockResolvedValue({ env: {} });

    const res = await GET();

    expect(res.status).toBe(302);
    expect(res.headers.get("location")).toContain(
      `https://github.com/apps/${APP_SLUG}/installations/new?state=`,
    );
  });
});
