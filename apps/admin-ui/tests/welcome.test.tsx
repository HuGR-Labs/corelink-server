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
}));

// Mock the welcome actions module for CopyPatButton isolation
vi.mock("@/app/[locale]/welcome/actions", () => ({
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
    const actions = await import("@/app/[locale]/welcome/actions");
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
      "@/app/[locale]/welcome/CopyPatButton"
    );
    render(<CopyPatButton pat="corelink_pat_test123" />);

    expect(screen.getByTestId("copy-pat-btn")).toBeInTheDocument();
    expect(screen.getByTestId("pat-saved-btn")).toBeInTheDocument();
  });

  it("copies the PAT to clipboard: clicking copy triggers 'Copied!' feedback", async () => {
    const user = userEvent.setup();
    const { CopyPatButton } = await import(
      "@/app/[locale]/welcome/CopyPatButton"
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
      "@/app/[locale]/welcome/CopyPatButton"
    );
    render(<CopyPatButton pat="corelink_pat_feedback" />);

    await user.click(screen.getByTestId("copy-pat-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("copy-pat-btn")).toHaveTextContent("Copied!");
    });
  });

  it("calls clearPatPlaintext action when saved-token button is clicked", async () => {
    const actions = await import("@/app/[locale]/welcome/actions");
    vi.mocked(actions.clearPatPlaintext).mockResolvedValue(undefined);

    const user = userEvent.setup();
    const { CopyPatButton } = await import(
      "@/app/[locale]/welcome/CopyPatButton"
    );
    render(<CopyPatButton pat="corelink_pat_save" />);

    await user.click(screen.getByTestId("pat-saved-btn"));

    await waitFor(() => {
      expect(actions.clearPatPlaintext).toHaveBeenCalledOnce();
    });
  });

  it("disables saved-token button while action is pending", async () => {
    const actions = await import("@/app/[locale]/welcome/actions");
    let resolve!: () => void;
    vi.mocked(actions.clearPatPlaintext).mockReturnValue(
      new Promise<void>((r) => {
        resolve = r;
      }),
    );

    const user = userEvent.setup();
    const { CopyPatButton } = await import(
      "@/app/[locale]/welcome/CopyPatButton"
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

  it("branch 1: renders PAT reveal when pat_plaintext is present", async () => {
    const clerk = await import("@clerk/nextjs/server");
    vi.mocked(clerk.auth).mockResolvedValue({
      userId: "user_1",
      sessionClaims: {
        publicMetadata: {
          tenant_id: "tenant-abc",
          region: "iad",
          pat_plaintext: "corelink_pat_secret",
        },
      },
    } as never);

    const pageModule = await import("@/app/[locale]/welcome/page");
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
  });

  it("branch 2: renders already-retrieved panel when pat_plaintext absent", async () => {
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

    const pageModule = await import("@/app/[locale]/welcome/page");
    const WelcomePage = pageModule.default;

    const element = await WelcomePage({
      params: Promise.resolve({ locale: "en" }),
    });
    const { container } = render(element as React.ReactElement);

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

  it("branch 3: redirects to /sign-up when tenant_id absent", async () => {
    const clerk = await import("@clerk/nextjs/server");
    vi.mocked(clerk.auth).mockResolvedValue({
      userId: "user_3",
      sessionClaims: {
        publicMetadata: {},
      },
    } as never);

    const pageModule = await import("@/app/[locale]/welcome/page");
    const WelcomePage = pageModule.default;

    await expect(
      WelcomePage({ params: Promise.resolve({ locale: "en" }) }),
    ).rejects.toThrow("REDIRECT:/en/sign-up");
  });
});
