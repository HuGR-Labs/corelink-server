import { describe, it, expect } from "vitest";
import { renderWithProviders } from "@/test-utils/render";
import { Skeleton } from "./Skeleton";

describe("Skeleton", () => {
  it("renders a single row by default", () => {
    const { container } = renderWithProviders(<Skeleton />);
    expect(container.querySelectorAll(".lin-skel")).toHaveLength(1);
  });

  it("renders the requested number of rows", () => {
    const { container } = renderWithProviders(<Skeleton rows={3} />);
    expect(container.querySelectorAll(".lin-skel")).toHaveLength(3);
  });
});
