import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { AppShell } from "./AppShell";

const navItems = [
  { label: "Home", href: "/" },
  { label: "Admin", href: "/admin", requiredRoles: ["admin"] },
];

describe("AppShell", () => {
  it("renders skip-to-content link first", () => {
    renderWithProviders(
      <AppShell navItems={navItems}>
        <p>body</p>
      </AppShell>
    );
    const skip = screen.getByText("Skip to main content");
    expect(skip).toBeInTheDocument();
    // It targets #main-content
    expect(skip).toHaveAttribute("href", "#main-content");
  });

  it("hides RBAC-gated items when role missing", () => {
    renderWithProviders(
      <AppShell navItems={navItems}>
        <p>body</p>
      </AppShell>
    );
    expect(screen.queryByText("Admin")).not.toBeInTheDocument();
    expect(screen.getByText("Home")).toBeInTheDocument();
  });

  it("shows RBAC items when user has role", () => {
    renderWithProviders(
      <AppShell navItems={navItems} userRoles={["admin"]}>
        <p>body</p>
      </AppShell>
    );
    expect(screen.getByText("Admin")).toBeInTheDocument();
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(
      <AppShell navItems={navItems} footer={<span>© 2026</span>}>
        <h1>Welcome</h1>
      </AppShell>
    );
    expect(await axe(container)).toHaveNoViolations();
  });
});
