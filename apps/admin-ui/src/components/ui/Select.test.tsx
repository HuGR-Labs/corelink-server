import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Select } from "./Select";

const options = [
  { value: "en", label: "English" },
  { value: "pt", label: "Português" },
  { value: "es", label: "Español" },
];

describe("Select", () => {
  it("renders trigger with accessible label and aria-expanded", () => {
    renderWithProviders(<Select label="Language" options={options} defaultValue="en" />);
    const trigger = screen.getByRole("combobox", { name: "Language" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  it("has no a11y violations on trigger", async () => {
    const { container } = renderWithProviders(<Select label="Language" options={options} />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
