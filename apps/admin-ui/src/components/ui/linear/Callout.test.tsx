import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Callout } from "./Callout";

describe("Callout", () => {
  it("renders its children", () => {
    renderWithProviders(<Callout>Heads up</Callout>);
    expect(screen.getByText("Heads up")).toBeInTheDocument();
  });

  it("applies the tone modifier class", () => {
    const { container } = renderWithProviders(<Callout tone="warn">Careful</Callout>);
    expect(container.querySelector(".lin-callout--warn")).not.toBeNull();
  });
});
