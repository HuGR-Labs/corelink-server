import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { MarkdownView } from "./MarkdownView";

describe("MarkdownView", () => {
  it("renders headings and paragraphs", () => {
    renderWithProviders(<MarkdownView content={"# Hello\n\nWorld"} />);
    expect(screen.getByRole("heading", { level: 1, name: "Hello" })).toBeInTheDocument();
    expect(screen.getByText("World")).toBeInTheDocument();
  });

  it("escapes script tags (XSS guard)", () => {
    const malicious = "<script>alert('xss')</script>\n\n# safe heading";
    const { container } = renderWithProviders(<MarkdownView content={malicious} />);
    expect(container.querySelector("script")).toBeNull();
    // The literal text is rendered, not executed.
    expect(container.textContent).toContain("alert('xss')");
  });

  it("renders tables (GFM)", () => {
    const md = "| A | B |\n|---|---|\n| 1 | 2 |";
    renderWithProviders(<MarkdownView content={md} />);
    expect(screen.getByRole("table")).toBeInTheDocument();
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(<MarkdownView content={"# Hi"} />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
