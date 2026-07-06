import { describe, it, expect } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Input } from "./Input";

describe("Input", () => {
  it("renders an input with the base class", () => {
    renderWithProviders(<Input aria-label="Name" />);
    expect(screen.getByLabelText("Name")).toHaveClass("lin-input");
  });

  it("accepts typed input", async () => {
    renderWithProviders(<Input aria-label="Name" />);
    const input = screen.getByLabelText("Name");
    await userEvent.type(input, "corelink");
    expect(input).toHaveValue("corelink");
  });
});
