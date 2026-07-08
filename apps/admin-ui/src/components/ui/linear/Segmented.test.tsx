import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Segmented } from "./Segmented";

const options = [
  { value: "month", label: "Monthly" },
  { value: "year", label: "Yearly" },
];

describe("Segmented", () => {
  it("marks the current value as selected", () => {
    renderWithProviders(
      <Segmented options={options} value="month" onChange={() => {}} />,
    );
    expect(screen.getByRole("tab", { name: "Monthly" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tab", { name: "Yearly" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
  });

  it("calls onChange with the clicked option value", async () => {
    const onChange = vi.fn();
    renderWithProviders(
      <Segmented options={options} value="month" onChange={onChange} />,
    );
    await userEvent.click(screen.getByRole("tab", { name: "Yearly" }));
    expect(onChange).toHaveBeenCalledWith("year");
  });
});
