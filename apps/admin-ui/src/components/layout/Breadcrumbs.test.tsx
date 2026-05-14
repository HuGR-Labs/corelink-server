import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Breadcrumbs } from "./Breadcrumbs";

const items = [
  { label: "Home", href: "/" },
  { label: "Settings", href: "/settings" },
  { label: "Profile" },
];

describe("Breadcrumbs", () => {
  it("renders nav with Breadcrumb aria-label and current page", () => {
    renderWithProviders(<Breadcrumbs items={items} />);
    expect(screen.getByRole("navigation", { name: "Breadcrumb" })).toBeInTheDocument();
    expect(screen.getByText("Profile")).toHaveAttribute("aria-current", "page");
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(<Breadcrumbs items={items} />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
