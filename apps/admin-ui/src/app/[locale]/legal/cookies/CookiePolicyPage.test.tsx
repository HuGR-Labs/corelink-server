import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen, waitFor } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { CookiePolicyPage } from "./CookiePolicyPage";

function mockFetch(ok = true) {
  return vi.fn(async () => ({
    ok,
    status: ok ? 200 : 500,
    json: async () => ({ accepted_at: "2026-05-14T12:00:00Z" }),
  })) as unknown as typeof fetch;
}

describe("CookiePolicyPage", () => {
  it("renders three categories with functional always-on", () => {
    renderWithProviders(<CookiePolicyPage locale="en" fetchImpl={mockFetch()} />);
    const switches = screen.getAllByRole("switch");
    expect(switches).toHaveLength(3);
    // Functional is disabled (always-on).
    expect(switches[0]).toHaveAttribute("aria-checked", "true");
    expect(switches[0]).toBeDisabled();
    expect(switches[1]).toHaveAttribute("aria-checked", "false");
    expect(switches[2]).toHaveAttribute("aria-checked", "false");
  });

  it("emits POST /v1/consent on save with chosen state", async () => {
    const fetchImpl = mockFetch();
    renderWithProviders(<CookiePolicyPage locale="en" fetchImpl={fetchImpl} />);
    await userEvent.click(screen.getByRole("switch", { name: "Analytics" }));
    await userEvent.click(screen.getByRole("button", { name: "Save consent" }));

    await waitFor(() => expect(fetchImpl).toHaveBeenCalledOnce());
    const call = (fetchImpl as unknown as ReturnType<typeof vi.fn>).mock.calls[0];
    if (!call) throw new Error("unreachable: fetchImpl was not called");
    const [, init] = call;
    expect(init.method).toBe("POST");
    const body = JSON.parse(init.body as string);
    expect(body).toEqual({ functional: true, analytics: true, marketing: false });
  });

  it("shows error message when POST fails", async () => {
    const fetchImpl = mockFetch(false);
    renderWithProviders(<CookiePolicyPage locale="en" fetchImpl={fetchImpl} />);
    await userEvent.click(screen.getByRole("button", { name: "Save consent" }));
    expect(await screen.findByText(/could not save your consent/i)).toBeInTheDocument();
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(
      <CookiePolicyPage locale="en" fetchImpl={mockFetch()} />
    );
    expect(await axe(container)).toHaveNoViolations();
  });
});
