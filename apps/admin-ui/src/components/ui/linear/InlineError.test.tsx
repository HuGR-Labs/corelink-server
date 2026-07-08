import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { InlineError } from "./InlineError";

describe("InlineError", () => {
  it("renders the error message in an alert", () => {
    renderWithProviders(<InlineError error={new Error("Boom")} />);
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Boom");
  });

  it("renders a Retry button that calls onRetry", async () => {
    const onRetry = vi.fn();
    renderWithProviders(<InlineError error="failed" onRetry={onRetry} />);
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledOnce();
  });
});
