import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { LegalDocPage } from "./LegalDocPage";

describe("LegalDocPage", () => {
  it("renders title, version, and accepted-on", () => {
    renderWithProviders(
      <LegalDocPage
        locale="en"
        title="Data Processing Agreement"
        content={"# DPA"}
        version="1.0.0"
        acceptedAt="2026-05-01"
      />
    );
    expect(screen.getByRole("heading", { level: 1, name: "Data Processing Agreement" })).toBeInTheDocument();
    expect(screen.getByText(/Accepted on/)).toBeInTheDocument();
    expect(screen.getByText(/Version/)).toBeInTheDocument();
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(
      <LegalDocPage locale="en" title="DPA" content={"# DPA"} version="1.0.0" />
    );
    expect(await axe(container)).toHaveNoViolations();
  });
});
