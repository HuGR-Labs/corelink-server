import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Checklist } from "./Checklist";

describe("Checklist", () => {
  it("renders each item label", () => {
    renderWithProviders(
      <Checklist
        items={[
          { id: "a", done: true, label: "Create account" },
          { id: "b", done: false, label: "Add payment" },
        ]}
      />,
    );
    expect(screen.getByText("Create account")).toBeInTheDocument();
    expect(screen.getByText("Add payment")).toBeInTheDocument();
  });

  it("marks done items via data-done", () => {
    const { container } = renderWithProviders(
      <Checklist items={[{ id: "a", done: true, label: "Done step" }]} />,
    );
    expect(container.querySelector('[data-done="true"]')).not.toBeNull();
  });
});
