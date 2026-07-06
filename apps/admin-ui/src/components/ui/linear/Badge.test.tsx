import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Badge } from "./Badge";

describe("Badge", () => {
  it("renders its children", () => {
    renderWithProviders(<Badge>Active</Badge>);
    expect(screen.getByText("Active")).toBeInTheDocument();
  });

  it("applies the tone modifier class", () => {
    renderWithProviders(<Badge tone="danger">Down</Badge>);
    expect(screen.getByText("Down")).toHaveClass("lin-badge", "lin-badge--danger");
  });

  it("renders a dot when dot is set", () => {
    const { container } = renderWithProviders(
      <Badge tone="success" dot>
        Up
      </Badge>,
    );
    expect(container.querySelector(".lin-dot")).not.toBeNull();
  });
});
