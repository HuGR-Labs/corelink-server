import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { PageHeader } from "./PageHeader";

describe("PageHeader", () => {
  it("renders h1 with title", () => {
    renderWithProviders(<PageHeader title="Settings" description="Manage your account." />);
    expect(screen.getByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
  });

  it("renders actions slot", () => {
    renderWithProviders(
      <PageHeader title="Settings" actions={<button>Save</button>} />
    );
    expect(screen.getByRole("button", { name: "Save" })).toBeInTheDocument();
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(<PageHeader title="Settings" />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
