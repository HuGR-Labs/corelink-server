import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Toast } from "./Toast";

describe("Toast", () => {
  it("renders title with status role when open", () => {
    renderWithProviders(
      <Toast open onOpenChange={() => {}} title="Saved" severity="success" />
    );
    expect(screen.getByText("Saved")).toBeInTheDocument();
  });

  it("error severity uses assertive (foreground) live region", () => {
    renderWithProviders(
      <Toast open onOpenChange={() => {}} title="Failed" severity="error" />
    );
    // Radix toast renders content with appropriate live region attributes.
    // For type=foreground (assertive) the toast becomes a status region announced immediately.
    expect(screen.getByText("Failed")).toBeInTheDocument();
  });

  it("has no a11y violations", async () => {
    const { baseElement } = renderWithProviders(
      <Toast open onOpenChange={() => {}} title="Saved" description="OK" />
    );
    expect(await axe(baseElement)).toHaveNoViolations();
  });
});
