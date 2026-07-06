import { describe, it, expect } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Select } from "./Select";

describe("Select", () => {
  it("renders options with the base class", () => {
    renderWithProviders(
      <Select aria-label="Tier" defaultValue="free">
        <option value="free">Free</option>
        <option value="pro">Pro</option>
      </Select>,
    );
    expect(screen.getByLabelText("Tier")).toHaveClass("lin-select");
  });

  it("changes the selected value", async () => {
    renderWithProviders(
      <Select aria-label="Tier" defaultValue="free">
        <option value="free">Free</option>
        <option value="pro">Pro</option>
      </Select>,
    );
    const select = screen.getByLabelText("Tier");
    await userEvent.selectOptions(select, "pro");
    expect(select).toHaveValue("pro");
  });
});
