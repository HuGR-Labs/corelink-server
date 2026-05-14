import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Switch } from "./Switch";

describe("Switch", () => {
  it("exposes aria-checked", () => {
    renderWithProviders(<Switch label="Analytics" defaultChecked />);
    const sw = screen.getByRole("switch", { name: "Analytics" });
    expect(sw).toHaveAttribute("aria-checked", "true");
  });

  it("toggles via space key", async () => {
    const onChange = vi.fn();
    renderWithProviders(<Switch label="Analytics" onCheckedChange={onChange} />);
    const sw = screen.getByRole("switch", { name: "Analytics" });
    sw.focus();
    await userEvent.keyboard(" ");
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(<Switch label="Analytics" />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
