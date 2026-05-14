import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { RadioGroup } from "./RadioGroup";

const options = [
  { value: "a", label: "Alpha" },
  { value: "b", label: "Beta" },
];

describe("RadioGroup", () => {
  it("renders accessible group with options", () => {
    renderWithProviders(<RadioGroup label="Pick" options={options} defaultValue="a" />);
    expect(screen.getByRole("radiogroup", { name: "Pick" })).toBeInTheDocument();
    expect(screen.getAllByRole("radio")).toHaveLength(2);
  });

  it("changes selection via click", async () => {
    const onChange = vi.fn();
    renderWithProviders(
      <RadioGroup label="Pick" options={options} onValueChange={onChange} />
    );
    await userEvent.click(screen.getByRole("radio", { name: "Beta" }));
    expect(onChange).toHaveBeenCalledWith("b");
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(
      <RadioGroup label="Pick" options={options} />
    );
    expect(await axe(container)).toHaveNoViolations();
  });
});
