import { describe, it, expect } from "vitest";
import { renderWithProviders } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Skeleton } from "./Skeleton";

describe("Skeleton", () => {
  it("is marked aria-busy and aria-hidden (decorative)", () => {
    const { container } = renderWithProviders(<Skeleton data-testid="sk" />);
    const el = container.querySelector("[data-testid=sk]") as HTMLElement;
    expect(el).toHaveAttribute("aria-busy", "true");
    expect(el).toHaveAttribute("aria-hidden", "true");
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(<Skeleton />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
