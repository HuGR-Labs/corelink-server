import { describe, it, expect } from "vitest";
import { renderWithProviders } from "@/test-utils/render";
import { StatusDot } from "./StatusDot";

describe("StatusDot", () => {
  it("renders a dot with the base class", () => {
    const { container } = renderWithProviders(<StatusDot />);
    expect(container.querySelector(".lin-dot")).not.toBeNull();
  });

  it("applies the tone modifier class", () => {
    const { container } = renderWithProviders(<StatusDot tone="success" />);
    expect(container.querySelector(".lin-dot--success")).not.toBeNull();
  });
});
