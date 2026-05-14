import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { EmptyState } from "./EmptyState";

describe("EmptyState", () => {
  it("renders heading, body, and CTA", () => {
    renderWithProviders(
      <EmptyState heading="No records" body="Try creating one." cta={<button>Create</button>} />
    );
    expect(screen.getByRole("heading", { name: "No records" })).toBeInTheDocument();
    expect(screen.getByText("Try creating one.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create" })).toBeInTheDocument();
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(<EmptyState heading="No records" />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
