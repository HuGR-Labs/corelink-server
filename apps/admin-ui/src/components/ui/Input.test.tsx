import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Input } from "./Input";

describe("Input", () => {
  it("associates label and hint via aria-describedby", () => {
    renderWithProviders(<Input label="Email" hint="We never share it." />);
    const input = screen.getByLabelText("Email");
    const describedBy = input.getAttribute("aria-describedby");
    expect(describedBy).toBeTruthy();
    const hint = document.getElementById(describedBy!);
    expect(hint).toHaveTextContent("We never share it.");
  });

  it("marks aria-invalid and exposes error via aria-describedby", () => {
    renderWithProviders(<Input label="Email" error="Invalid" />);
    const input = screen.getByLabelText("Email");
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("Invalid");
  });

  it("supports visually-hidden label", () => {
    renderWithProviders(<Input label="Search" hideLabel placeholder="Search" />);
    expect(screen.getByLabelText("Search")).toBeInTheDocument();
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(<Input label="Email" hint="hi" />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
