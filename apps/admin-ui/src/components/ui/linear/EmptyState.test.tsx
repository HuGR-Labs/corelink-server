import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { EmptyState } from "./EmptyState";

describe("EmptyState", () => {
  it("renders the title", () => {
    renderWithProviders(<EmptyState title="No tenants yet" />);
    expect(screen.getByText("No tenants yet")).toBeInTheDocument();
  });

  it("renders the body and cta when provided", () => {
    renderWithProviders(
      <EmptyState
        title="No tenants yet"
        body="Create your first tenant to get started"
        cta={<button>Create</button>}
      />,
    );
    expect(
      screen.getByText("Create your first tenant to get started"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create" })).toBeInTheDocument();
  });
});
