import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Tooltip } from "./Tooltip";
import { Button } from "./Button";

describe("Tooltip", () => {
  it("renders trigger", () => {
    renderWithProviders(
      <Tooltip content="Hint">
        <Button>Help</Button>
      </Tooltip>
    );
    expect(screen.getByRole("button", { name: "Help" })).toBeInTheDocument();
  });

  it("shows tooltip when controlled open", async () => {
    renderWithProviders(
      <Tooltip content="Hint" open>
        <Button>Help</Button>
      </Tooltip>
    );
    // Radix renders tooltip content (may be in portal).
    const matches = await screen.findAllByText("Hint");
    expect(matches.length).toBeGreaterThan(0);
  });

  it("has no a11y violations", async () => {
    const { baseElement } = renderWithProviders(
      <Tooltip content="Hint">
        <Button>Help</Button>
      </Tooltip>
    );
    expect(await axe(baseElement)).toHaveNoViolations();
  });
});
