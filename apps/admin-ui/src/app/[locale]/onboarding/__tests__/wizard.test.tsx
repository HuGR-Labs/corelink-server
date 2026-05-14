import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Wizard, stepIndex } from "@/components/onboarding/Wizard";
import { ALL_STEPS } from "@/lib/onboarding-state";

describe("Wizard step indicator", () => {
  it("marks all earlier steps as done and the current as current", () => {
    render(
      <Wizard
        current="region-plan"
        labels={{
          stepFormat: (c, t) => `${c}/${t}`,
        }}
      >
        <p>body</p>
      </Wizard>,
    );

    const list = screen.getByRole("list", { name: /onboarding steps/i });
    const items = list.querySelectorAll("li");
    expect(items).toHaveLength(ALL_STEPS.length);

    const states = Array.from(items).map((li) => li.getAttribute("data-state"));
    const currentIdx = stepIndex("region-plan");
    states.forEach((state, i) => {
      if (i < currentIdx) expect(state).toBe("done");
      else if (i === currentIdx) expect(state).toBe("current");
      else expect(state).toBe("pending");
    });

    expect(screen.getByTestId("wizard-progress").textContent).toBe(
      `${currentIdx + 1}/${ALL_STEPS.length}`,
    );
  });

  it("transitions current marker when prop changes", () => {
    const { rerender } = render(<Wizard current="tenant">x</Wizard>);
    expect(
      screen.getByRole("list").querySelector('[data-state="current"]')!
        .getAttribute("data-step"),
    ).toBe("tenant");

    rerender(<Wizard current="pat">x</Wizard>);
    expect(
      screen.getByRole("list").querySelector('[data-state="current"]')!
        .getAttribute("data-step"),
    ).toBe("pat");
  });
});
