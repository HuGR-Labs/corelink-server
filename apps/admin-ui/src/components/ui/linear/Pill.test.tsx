import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Pill } from "./Pill";

describe("Pill", () => {
  it("renders its children with the pill class", () => {
    renderWithProviders(<Pill>v2</Pill>);
    const pill = screen.getByText("v2");
    expect(pill).toBeInTheDocument();
    expect(pill).toHaveClass("lin-pill");
  });
});
