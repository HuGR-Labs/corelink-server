import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { PrivacyPage } from "./PrivacyPage";

const md = `# Privacy Notice\n\n**Version:** 1.0.0\n**Last updated:** 2026-05-14\n\nBody text.`;

describe("PrivacyPage", () => {
  it("renders title, version, and content", () => {
    renderWithProviders(
      <PrivacyPage locale="en" content={md} version="1.0.0" lastUpdated="2026-05-14" />
    );
    expect(screen.getByRole("heading", { level: 1, name: "Privacy notice" })).toBeInTheDocument();
    // The version label appears at least once (banner + markdown).
    expect(screen.getAllByText(/Version/).length).toBeGreaterThan(0);
    expect(screen.getByText(/Body text\./)).toBeInTheDocument();
  });

  it("links to sub-processors page", () => {
    renderWithProviders(
      <PrivacyPage locale="en" content={md} version="1.0.0" lastUpdated="2026-05-14" />
    );
    const link = screen.getByRole("link", { name: /Sub-processors/ });
    expect(link).toHaveAttribute("href", "/en/privacy/sub-processors");
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(
      <PrivacyPage locale="en" content={md} version="1.0.0" lastUpdated="2026-05-14" />
    );
    expect(await axe(container)).toHaveNoViolations();
  });
});
