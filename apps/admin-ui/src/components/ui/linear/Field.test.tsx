import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Field } from "./Field";
import { Input } from "./Input";

describe("Field", () => {
  it("renders the label and wraps its child control", () => {
    renderWithProviders(
      <Field label="Email" htmlFor="email">
        <Input id="email" />
      </Field>,
    );
    expect(screen.getByText("Email")).toBeInTheDocument();
    expect(screen.getByLabelText("Email")).toBeInTheDocument();
  });

  it("renders a help popover trigger when help is provided", () => {
    renderWithProviders(
      <Field label="API key" help="Used to authenticate requests">
        <Input />
      </Field>,
    );
    expect(screen.getByRole("button", { name: "API key" })).toBeInTheDocument();
  });
});
