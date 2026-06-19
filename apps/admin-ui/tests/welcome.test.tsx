/**
 * Tests for CopyPatButton — clipboard copy and save action invocation.
 *
 * Uses a mocked `clearPatPlaintext` action (module mock) to isolate
 * the client component's behaviour from the server action.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import * as React from "react";

// Must be at top level — vitest hoists these
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

// Mock the welcome actions module for CopyPatButton isolation
vi.mock("@/app/[locale]/(authenticated)/welcome/actions", () => ({
  clearPatPlaintext: vi.fn(),
}));

// ---------------------------------------------------------------------------
// 1. CopyPatButton tests
// ---------------------------------------------------------------------------

describe("CopyPatButton", () => {
  // jsdom doesn't implement navigator.clipboard. We define it directly on
  // the window object (which is globalThis in jsdom). We use a plain object
  // with a vi.fn() per test so call counts are isolated.
  let writeTextMock: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    const actions = await import("@/app/[locale]/(authenticated)/welcome/actions");
    vi.mocked(actions.clearPatPlaintext).mockReset();

    writeTextMock = vi.fn().mockResolvedValue(undefined);
    // Install on the real window.navigator.clipboard so the component can reach it
    Object.defineProperty(window, "navigator", {
      value: Object.create(window.navigator, {
        clipboard: {
          value: { writeText: writeTextMock },
          writable: true,
          configurable: true,
        },
      }),
      configurable: true,
      writable: true,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders the copy button and the saved-token button", async () => {
    const { CopyPatButton } = await import(
      "@/app/[locale]/(authenticated)/welcome/CopyPatButton"
    );
    render(<CopyPatButton pat="corelink_pat_test123" />);

    expect(screen.getByTestId("copy-pat-btn")).toBeInTheDocument();
    expect(screen.getByTestId("pat-saved-btn")).toBeInTheDocument();
  });

  it("copies the PAT to clipboard: clicking copy triggers 'Copied!' feedback", async () => {
    const user = userEvent.setup();
    const { CopyPatButton } = await import(
      "@/app/[locale]/(authenticated)/welcome/CopyPatButton"
    );
    render(<CopyPatButton pat="corelink_pat_abc" />);

    // The button starts as "Copy"
    expect(screen.getByTestId("copy-pat-btn")).toHaveTextContent("Copy");

    await user.click(screen.getByTestId("copy-pat-btn"));

    // After a successful clipboard.writeText, setCopied(true) → "Copied!" shows.
    // This verifies the clipboard path ran to completion.
    await waitFor(() => {
      expect(screen.getByTestId("copy-pat-btn")).toHaveTextContent("Copied!");
    });
  });

  it("shows 'Copied!' feedback after clipboard write", async () => {
    const user = userEvent.setup();
    const { CopyPatButton } = await import(
      "@/app/[locale]/(authenticated)/welcome/CopyPatButton"
    );
    render(<CopyPatButton pat="corelink_pat_feedback" />);

    await user.click(screen.getByTestId("copy-pat-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("copy-pat-btn")).toHaveTextContent("Copied!");
    });
  });

  it("calls clearPatPlaintext action when saved-token button is clicked", async () => {
    const actions = await import("@/app/[locale]/(authenticated)/welcome/actions");
    vi.mocked(actions.clearPatPlaintext).mockResolvedValue(undefined);

    const user = userEvent.setup();
    const { CopyPatButton } = await import(
      "@/app/[locale]/(authenticated)/welcome/CopyPatButton"
    );
    render(<CopyPatButton pat="corelink_pat_save" />);

    await user.click(screen.getByTestId("pat-saved-btn"));

    await waitFor(() => {
      expect(actions.clearPatPlaintext).toHaveBeenCalledOnce();
    });
  });

  it("disables saved-token button while action is pending", async () => {
    const actions = await import("@/app/[locale]/(authenticated)/welcome/actions");
    let resolve!: () => void;
    vi.mocked(actions.clearPatPlaintext).mockReturnValue(
      new Promise<void>((r) => {
        resolve = r;
      }),
    );

    const user = userEvent.setup();
    const { CopyPatButton } = await import(
      "@/app/[locale]/(authenticated)/welcome/CopyPatButton"
    );
    render(<CopyPatButton pat="corelink_pat_pending" />);

    await user.click(screen.getByTestId("pat-saved-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("pat-saved-btn")).toBeDisabled();
    });

    await act(async () => {
      resolve();
    });
  });
});

// ---------------------------------------------------------------------------
// 2. WelcomePage 3-branch rendering tests
// ---------------------------------------------------------------------------

describe("WelcomePage rendering branches", () => {
  beforeEach(async () => {
    const nav = await import("next/navigation");
    vi.mocked(nav.redirect).mockImplementation((path: string) => {
      throw new Error(`REDIRECT:${path}`);
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("branch 1: renders PAT reveal when pat_plaintext present in private_metadata (read via Backend API)", async () => {
    const clerk = await import("@clerk/nextjs/server");
    // public_metadata (session claims) carries ONLY tenant_id + region now.
    vi.mocked(clerk.auth).mockResolvedValue({
      userId: "user_1",
      sessionClaims: {
        publicMetadata: {
          tenant_id: "tenant-abc",
          region: "iad",
        },
      },
    } as never);
    // The PAT plaintext is read SERVER-SIDE from private_metadata via the
    // Clerk Backend API — never from the session JWT.
    const getUser = vi.fn().mockResolvedValue({
      privateMetadata: { pat_plaintext: "corelink_pat_secret" },
    });
    vi.mocked(clerk.clerkClient).mockResolvedValue({
      users: { getUser },
    } as never);

    const pageModule = await import("@/app/[locale]/(authenticated)/welcome/page");
    const WelcomePage = pageModule.default;

    const element = await WelcomePage({
      params: Promise.resolve({ locale: "en" }),
    });
    const { container } = render(element as React.ReactElement);

    expect(
      container.querySelector("[data-testid='welcome-root']"),
    ).toBeTruthy();
    expect(
      container.querySelector("[data-testid='pat-reveal-section']"),
    ).toBeTruthy();
    expect(
      container.querySelector("[data-testid='install-section']"),
    ).toBeTruthy();
    // Confirms the reveal came from the Backend-API read of the user's
    // private_metadata — not from session claims.
    expect(getUser).toHaveBeenCalledWith("user_1");
  });

  it("branch 2: renders already-retrieved panel when private_metadata has no pat_plaintext", async () => {
    const clerk = await import("@clerk/nextjs/server");
    vi.mocked(clerk.auth).mockResolvedValue({
      userId: "user_2",
      sessionClaims: {
        publicMetadata: {
          tenant_id: "tenant-def",
          region: "ord",
        },
      },
    } as never);
    // Backend API returns a user whose private_metadata has no pat_plaintext
    // (already cleared) → already-retrieved branch.
    const getUser = vi.fn().mockResolvedValue({ privateMetadata: {} });
    vi.mocked(clerk.clerkClient).mockResolvedValue({
      users: { getUser },
    } as never);

    const pageModule = await import("@/app/[locale]/(authenticated)/welcome/page");
    const WelcomePage = pageModule.default;

    const element = await WelcomePage({
      params: Promise.resolve({ locale: "en" }),
    });
    const { container } = render(element as React.ReactElement);

    expect(getUser).toHaveBeenCalledWith("user_2");
    expect(
      container.querySelector("[data-testid='welcome-already-retrieved']"),
    ).toBeTruthy();
    expect(
      container.querySelector("[data-testid='already-retrieved-notice']"),
    ).toBeTruthy();
    expect(
      container.querySelector("[data-testid='rotate-keys-link']"),
    ).toBeTruthy();
  });

  it("branch 3: redirects to the locale-less /sign-in when tenant_id absent", async () => {
    // The redirect target is the locale-less /sign-in (the only real sign-in
    // route — there is no [locale]/sign-in), matching the /upgrade page. A
    // session with no tenant (webhook mid-flight or no session) goes there to
    // re-establish, rather than the old /en/sign-up (which 404'd for the
    // authenticated case). See the /en/welcome 500 fix.
    const clerk = await import("@clerk/nextjs/server");
    vi.mocked(clerk.auth).mockResolvedValue({
      userId: "user_3",
      sessionClaims: {
        publicMetadata: {},
      },
    } as never);

    const pageModule = await import("@/app/[locale]/(authenticated)/welcome/page");
    const WelcomePage = pageModule.default;

    await expect(
      WelcomePage({ params: Promise.resolve({ locale: "en" }) }),
    ).rejects.toThrow("REDIRECT:/sign-in");
  });

  it("redirects to /sign-in when auth() throws (no middleware context — never 500s)", async () => {
    // Defensive: if Clerk's `auth()` throws (edge context unavailable), the page
    // must redirect to sign-in, not crash with a 500.
    const clerk = await import("@clerk/nextjs/server");
    vi.mocked(clerk.auth).mockRejectedValue(
      new Error("auth() called but Clerk can't detect clerkMiddleware()"),
    );

    const pageModule = await import("@/app/[locale]/(authenticated)/welcome/page");
    const WelcomePage = pageModule.default;

    await expect(
      WelcomePage({ params: Promise.resolve({ locale: "en" }) }),
    ).rejects.toThrow("REDIRECT:/sign-in");
  });
});
