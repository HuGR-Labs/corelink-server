import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Gauge } from "./Gauge";

describe("Gauge", () => {
  it("renders the pct text", () => {
    renderWithProviders(<Gauge label="Storage" value={25} max={100} />);
    expect(screen.getByText("25%")).toBeInTheDocument();
  });

  it("sets the fill width to value/max", () => {
    const { container } = renderWithProviders(
      <Gauge label="Storage" value={30} max={100} />,
    );
    const fill = container.querySelector<HTMLElement>(".lin-gauge__fill");
    expect(fill?.style.width).toBe("30%");
  });

  it("applies the danger class past dangerAt", () => {
    const { container } = renderWithProviders(
      <Gauge label="Storage" value={95} max={100} dangerAt={0.9} />,
    );
    expect(container.querySelector(".lin-gauge__fill--danger")).not.toBeNull();
  });
});
