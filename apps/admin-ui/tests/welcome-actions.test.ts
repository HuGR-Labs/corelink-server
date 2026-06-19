/**
 * Tests for clearPatPlaintext server action (welcome/actions.ts).
 *
 * Separate file from welcome.test.tsx because the CopyPatButton tests
 * need `vi.mock("@/app/[locale]/(authenticated)/welcome/actions")` while these
 * tests import the REAL actions module.
 *
 * SECURITY (2026-06-19, CRED-pat-plaintext): the clear now targets Clerk
 * `private_metadata.pat_plaintext` via the Backend API `users.updateUser`
 * (clerkClient), NOT public_metadata via a raw PATCH.
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
  clerkClient: vi.fn(),
}));

// ---------------------------------------------------------------------------
// clearPatPlaintext action tests
// ---------------------------------------------------------------------------

describe("clearPatPlaintext action", () => {
  const MOCK_USER_ID = "user_test123";
  const MOCK_SECRET_KEY = "sk_test_mockkey";
  let updateUser: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    process.env.CLERK_SECRET_KEY = MOCK_SECRET_KEY;

    const clerk = await import("@clerk/nextjs/server");
    vi.mocked(clerk.auth).mockResolvedValue({
      userId: MOCK_USER_ID,
    } as never);

    updateUser = vi.fn().mockResolvedValue({});
    vi.mocked(clerk.clerkClient).mockResolvedValue({
      users: { updateUser },
    } as never);

    const nav = await import("next/navigation");
    vi.mocked(nav.redirect).mockImplementation((_path: string) => {
      throw new Error("NEXT_REDIRECT");
    });
  });

  afterEach(() => {
    delete process.env.CLERK_SECRET_KEY;
    vi.clearAllMocks();
  });

  it("calls Clerk Backend API updateUser with null private_metadata.pat_plaintext", async () => {
    const { clearPatPlaintext } = await import(
      "@/app/[locale]/(authenticated)/welcome/actions"
    );

    await expect(clearPatPlaintext()).rejects.toThrow("NEXT_REDIRECT");

    expect(updateUser).toHaveBeenCalledOnce();
    const [id, params] = updateUser.mock.calls[0] as [
      string,
      { privateMetadata: Record<string, unknown> },
    ];
    expect(id).toBe(MOCK_USER_ID);
    expect(params.privateMetadata).toHaveProperty("pat_plaintext", null);
  });

  it("revalidates /customer and redirects after clearing PAT", async () => {
    const { clearPatPlaintext } = await import(
      "@/app/[locale]/(authenticated)/welcome/actions"
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
    } as never);

    const { clearPatPlaintext } = await import(
      "@/app/[locale]/(authenticated)/welcome/actions"
    );

    await expect(clearPatPlaintext()).rejects.toThrow("no_active_session");
  });

  it("throws if the Clerk Backend API update fails", async () => {
    updateUser.mockRejectedValue(Object.assign(new Error("boom"), { status: 500 }));
    const { clearPatPlaintext } = await import(
      "@/app/[locale]/(authenticated)/welcome/actions"
    );

    await expect(clearPatPlaintext()).rejects.toThrow(
      "clerk_metadata_clear_failed_500",
    );
  });

  it("throws if CLERK_SECRET_KEY is missing", async () => {
    delete process.env.CLERK_SECRET_KEY;
    const { clearPatPlaintext } = await import(
      "@/app/[locale]/(authenticated)/welcome/actions"
    );

    await expect(clearPatPlaintext()).rejects.toThrow(
      "CLERK_SECRET_KEY_not_configured",
    );
  });
});
