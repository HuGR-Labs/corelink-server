/**
 * Tests for clearPatPlaintext server action (welcome/actions.ts).
 *
 * Separate file from welcome.test.tsx because the CopyPatButton tests
 * need `vi.mock("@/app/[locale]/welcome/actions")` while these tests
 * import the REAL actions module.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Top-level mocks — hoisted by vitest
vi.mock("next/navigation", () => ({
  redirect: vi.fn(),
}));

vi.mock("next/cache", () => ({
  revalidatePath: vi.fn(),
}));

vi.mock("@clerk/nextjs/server", () => ({
  auth: vi.fn(),
}));

// ---------------------------------------------------------------------------
// clearPatPlaintext action tests
// ---------------------------------------------------------------------------

describe("clearPatPlaintext action", () => {
  const MOCK_USER_ID = "user_test123";
  const MOCK_SECRET_KEY = "sk_test_mockkey";

  beforeEach(async () => {
    process.env.CLERK_SECRET_KEY = MOCK_SECRET_KEY;

    const clerk = await import("@clerk/nextjs/server");
    vi.mocked(clerk.auth).mockResolvedValue({
      userId: MOCK_USER_ID,
      sessionClaims: {
        publicMetadata: {
          tenant_id: "tenant-uuid-001",
          region: "ord",
        },
      },
    } as never);

    const nav = await import("next/navigation");
    vi.mocked(nav.redirect).mockImplementation((_path: string) => {
      throw new Error("NEXT_REDIRECT");
    });

    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
    });
  });

  afterEach(() => {
    delete process.env.CLERK_SECRET_KEY;
    vi.clearAllMocks();
  });

  it("calls Clerk PATCH API with null pat_plaintext", async () => {
    const { clearPatPlaintext } = await import(
      "@/app/[locale]/welcome/actions"
    );

    await expect(clearPatPlaintext()).rejects.toThrow("NEXT_REDIRECT");

    expect(global.fetch).toHaveBeenCalledOnce();
    const [url, opts] = (global.fetch as ReturnType<typeof vi.fn>).mock
      .calls[0] as [string, RequestInit];
    expect(url).toBe(`https://api.clerk.com/v1/users/${MOCK_USER_ID}`);
    expect(opts.method).toBe("PATCH");
    expect(opts.headers).toMatchObject({
      authorization: `Bearer ${MOCK_SECRET_KEY}`,
      "content-type": "application/json",
    });
    const body = JSON.parse(opts.body as string) as {
      public_metadata: Record<string, unknown>;
    };
    expect(body.public_metadata.pat_plaintext).toBeNull();
    expect(body.public_metadata.tenant_id).toBe("tenant-uuid-001");
    expect(body.public_metadata.region).toBe("ord");
  });

  it("revalidates /customer and redirects after clearing PAT", async () => {
    const { clearPatPlaintext } = await import(
      "@/app/[locale]/welcome/actions"
    );
    const cache = await import("next/cache");
    const nav = await import("next/navigation");

    await expect(clearPatPlaintext()).rejects.toThrow("NEXT_REDIRECT");

    expect(cache.revalidatePath).toHaveBeenCalledWith("/customer");
    expect(nav.redirect).toHaveBeenCalledWith("/customer");
  });

  it("throws if no active session", async () => {
    const clerk = await import("@clerk/nextjs/server");
    vi.mocked(clerk.auth).mockResolvedValue({
      userId: null,
      sessionClaims: {},
    } as never);

    const { clearPatPlaintext } = await import(
      "@/app/[locale]/welcome/actions"
    );

    await expect(clearPatPlaintext()).rejects.toThrow("no_active_session");
  });

  it("throws if Clerk API returns non-200", async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
    });
    const { clearPatPlaintext } = await import(
      "@/app/[locale]/welcome/actions"
    );

    await expect(clearPatPlaintext()).rejects.toThrow(
      "clerk_metadata_clear_failed_500",
    );
  });

  it("throws if CLERK_SECRET_KEY is missing", async () => {
    delete process.env.CLERK_SECRET_KEY;
    const { clearPatPlaintext } = await import(
      "@/app/[locale]/welcome/actions"
    );

    await expect(clearPatPlaintext()).rejects.toThrow(
      "CLERK_SECRET_KEY_not_configured",
    );
  });
});
