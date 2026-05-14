import { describe, it, expect } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { SubProcessorsPage, toCsv } from "./SubProcessorsPage";

const items = [
  {
    id: "cloudflare",
    name: "Cloudflare, Inc.",
    role: "CDN",
    region: "Global",
    certifications: ["ISO 27001", "SOC 2"],
    last_audit: "2026-02-12",
  },
  {
    id: "clerk",
    name: "Clerk, Inc.",
    role: "IdP",
    region: "US",
    certifications: ["SOC 2"],
    last_audit: "2026-01-30",
  },
];

describe("SubProcessorsPage", () => {
  it("renders table with sub-processors", () => {
    renderWithProviders(<SubProcessorsPage locale="en" version="2026-05-14" items={items} />);
    expect(screen.getByRole("table", { name: "Sub-processors" })).toBeInTheDocument();
    expect(screen.getByText("Cloudflare, Inc.")).toBeInTheDocument();
  });

  it("table headers are keyboard-reachable via tab", async () => {
    renderWithProviders(<SubProcessorsPage locale="en" version="2026-05-14" items={items} />);
    const sortBtns = screen
      .getAllByRole("button")
      .filter((b) => b.textContent?.toLowerCase().includes("name") || b.textContent?.toLowerCase().includes("region"));
    expect(sortBtns.length).toBeGreaterThan(0);
    const firstBtn = sortBtns[0];
    if (!firstBtn) throw new Error("unreachable: no sort buttons matched");
    firstBtn.focus();
    expect(firstBtn).toHaveFocus();
    await userEvent.tab();
    // Some other focusable element should now have focus.
    expect(document.activeElement).not.toBe(firstBtn);
  });

  it("produces valid CSV", () => {
    const csv = toCsv(items);
    expect(csv.split("\n")).toHaveLength(3);
    expect(csv).toContain("Cloudflare, Inc.");
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(
      <SubProcessorsPage locale="en" version="2026-05-14" items={items} />
    );
    expect(await axe(container)).toHaveNoViolations();
  });
});
