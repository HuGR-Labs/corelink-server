/**
 * Tests for PatRevealCard:
 *   - initial state: token is blurred, "Reveal" button present
 *   - reveal: click "Reveal" shows plaintext, "Hide" button appears
 *   - hide: click "Hide" re-blurs and shows "Reveal" button
 *   - copy: clipboard.writeText fires with the raw PAT in either state
 *   - copy feedback: "Copied!" shown after successful write
 *   - never persisted: no localStorage / sessionStorage writes
 *   - auto-hide: token reverts to blurred after 60 s (fake timers)
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import * as React from "react";
import { PatRevealCard } from "@/components/PatRevealCard";

const TEST_PAT = "corelink_pat_test_abc123xyz";

/**
 * Install a fresh clipboard mock before each test.
 * jsdom does not implement navigator.clipboard, so we define it once
 * at module scope and replace writeText with a fresh vi.fn() per test.
 */
if (typeof navigator !== "undefined") {
  // Ensure navigator.clipboard exists as a configurable object
  if (!navigator.clipboard) {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: vi.fn() },
      writable: true,
      configurable: true,
    });
  }
}

describe("PatRevealCard", () => {
  let writeTextMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    // Replace writeText with a fresh spy on each test
    writeTextMock = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator.clipboard, "writeText", {
      value: writeTextMock,
      writable: true,
      configurable: true,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("renders without crashing", () => {
    render(<PatRevealCard patPlaintext={TEST_PAT} />);
    expect(screen.getByTestId("pat-reveal-card")).toBeInTheDocument();
  });

  it("is blurred by default: shows 'Reveal' button", () => {
    render(<PatRevealCard patPlaintext={TEST_PAT} />);
    expect(screen.getByTestId("pat-reveal-toggle")).toHaveTextContent("Reveal");
  });

  it("shows only the last 4 chars when blurred", () => {
    render(<PatRevealCard patPlaintext={TEST_PAT} />);
    const display = screen.getByTestId("pat-reveal-token-display");
    expect(display.textContent).toContain(TEST_PAT.slice(-4));
    expect(display.textContent).not.toBe(TEST_PAT);
  });

  it("clicking Reveal shows plaintext and changes toggle to 'Hide'", async () => {
    const user = userEvent.setup();
    render(<PatRevealCard patPlaintext={TEST_PAT} />);

    await user.click(screen.getByTestId("pat-reveal-toggle"));

    const display = screen.getByTestId("pat-reveal-token-display");
    expect(display.textContent).toBe(TEST_PAT);
    expect(screen.getByTestId("pat-reveal-toggle")).toHaveTextContent("Hide");
  });

  it("clicking Hide re-blurs the token and resets toggle to 'Reveal'", async () => {
    const user = userEvent.setup();
    render(<PatRevealCard patPlaintext={TEST_PAT} />);

    // Reveal first
    await user.click(screen.getByTestId("pat-reveal-toggle"));
    expect(screen.getByTestId("pat-reveal-toggle")).toHaveTextContent("Hide");

    // Then hide
    await user.click(screen.getByTestId("pat-reveal-toggle"));
    expect(screen.getByTestId("pat-reveal-toggle")).toHaveTextContent("Reveal");
    const display = screen.getByTestId("pat-reveal-token-display");
    expect(display.textContent).not.toBe(TEST_PAT);
  });

  it("auto-hides after 60 s when revealed", async () => {
    vi.useFakeTimers();
    render(<PatRevealCard patPlaintext={TEST_PAT} />);

    // Reveal via direct DOM click (not userEvent, to avoid fake-timer interaction issues)
    await act(async () => {
      screen.getByTestId("pat-reveal-toggle").click();
      await Promise.resolve();
    });

    expect(screen.getByTestId("pat-reveal-toggle")).toHaveTextContent("Hide");

    // Advance time by 60 s — matches the AUTO_HIDE_MS constant
    await act(async () => {
      vi.advanceTimersByTime(60_000);
      await Promise.resolve();
    });

    expect(screen.getByTestId("pat-reveal-toggle")).toHaveTextContent("Reveal");
  });

  it("copy button fires clipboard.writeText with the raw PAT value", async () => {
    const user = userEvent.setup();
    render(<PatRevealCard patPlaintext={TEST_PAT} />);

    await user.click(screen.getByTestId("pat-reveal-copy"));

    expect(writeTextMock).toHaveBeenCalledOnce();
    expect(writeTextMock).toHaveBeenCalledWith(TEST_PAT);
  });

  it("copy button shows 'Copied!' feedback after successful write", async () => {
    const user = userEvent.setup();
    render(<PatRevealCard patPlaintext={TEST_PAT} />);

    await user.click(screen.getByTestId("pat-reveal-copy"));

    await waitFor(() => {
      expect(screen.getByTestId("pat-reveal-copy")).toHaveTextContent("Copied!");
    });
  });

  it("copy button works when token is blurred (not revealed)", async () => {
    const user = userEvent.setup();
    render(<PatRevealCard patPlaintext={TEST_PAT} />);

    // Confirm we are in blurred state
    expect(screen.getByTestId("pat-reveal-toggle")).toHaveTextContent("Reveal");

    await user.click(screen.getByTestId("pat-reveal-copy"));

    expect(writeTextMock).toHaveBeenCalledWith(TEST_PAT);
  });

  it("PAT is never written to localStorage or sessionStorage", () => {
    const localSetItem = vi.spyOn(window.localStorage.__proto__, "setItem");
    const sessionSetItem = vi.spyOn(window.sessionStorage.__proto__, "setItem");

    render(<PatRevealCard patPlaintext={TEST_PAT} />);

    expect(localSetItem).not.toHaveBeenCalled();
    expect(sessionSetItem).not.toHaveBeenCalled();

    localSetItem.mockRestore();
    sessionSetItem.mockRestore();
  });

  it("warning banner is always visible", () => {
    render(<PatRevealCard patPlaintext={TEST_PAT} />);
    expect(screen.getByTestId("pat-reveal-warning")).toBeInTheDocument();
  });

  it("countdown hint appears only when revealed", async () => {
    const user = userEvent.setup();
    render(<PatRevealCard patPlaintext={TEST_PAT} />);

    // Before reveal — hint should not be present
    expect(
      screen.queryByTestId("pat-reveal-countdown-hint"),
    ).not.toBeInTheDocument();

    await user.click(screen.getByTestId("pat-reveal-toggle"));

    // After reveal — hint should appear
    expect(
      screen.getByTestId("pat-reveal-countdown-hint"),
    ).toBeInTheDocument();
  });
});
