import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Spinner } from "./Spinner";

describe("Spinner", () => {
  it("has role=status and sr-only label", () => {
    renderWithProviders(<Spinner />);
    const status = screen.getByRole("status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status).toHaveTextContent("Loading…");
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(<Spinner />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
