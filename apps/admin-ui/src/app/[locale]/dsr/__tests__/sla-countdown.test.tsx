import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import {
  colorFor,
  computeCountdown,
} from "@/lib/sla-countdown";
import { SlaCountdown } from "@/components/dsr/SlaCountdown";

describe("SLA countdown color thresholds (Test 9)", () => {
  it("returns green when > 14 days", () => {
    expect(colorFor(20)).toBe("green");
    expect(colorFor(15)).toBe("green");
  });
  it("returns yellow when between 7 and 14 days", () => {
    expect(colorFor(14)).toBe("yellow");
    expect(colorFor(7)).toBe("yellow");
  });
  it("returns red when < 7 days", () => {
    expect(colorFor(6)).toBe("red");
    expect(colorFor(0)).toBe("red");
  });
  it("returns overdue for past deadlines", () => {
    const past = new Date(Date.now() - 86_400_000).toISOString();
    expect(computeCountdown(past).overdue).toBe(true);
    expect(computeCountdown(past).color).toBe("overdue");
  });

  it("renders correct data-color attribute on <SlaCountdown>", () => {
    const future20d = new Date(Date.now() + 20 * 86_400_000).toISOString();
    render(<SlaCountdown locale="en" deadline={future20d} refreshMs={0} />);
    expect(screen.getByTestId("dsr-sla-countdown")).toHaveAttribute(
      "data-color",
      "green",
    );
  });
});
