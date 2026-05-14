import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Checkbox } from "./Checkbox";

describe("Checkbox", () => {
  it("toggles on click and emits change", async () => {
    const onChange = vi.fn();
    renderWithProviders(<Checkbox label="Accept" onCheckedChange={onChange} />);
    const cb = screen.getByRole("checkbox", { name: "Accept" });
    await userEvent.click(cb);
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("supports indeterminate state", () => {
    renderWithProviders(<Checkbox label="Select all" checked="indeterminate" />);
    const cb = screen.getByRole("checkbox", { name: "Select all" });
    expect(cb).toHaveAttribute("aria-checked", "mixed");
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(<Checkbox label="Accept" />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
