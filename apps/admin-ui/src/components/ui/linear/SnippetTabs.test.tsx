import { describe, it, expect, vi, beforeEach } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { SnippetTabs } from "./SnippetTabs";

const tabs = [
  { id: "npm", label: "npm", code: "npm install corelink" },
  { id: "pnpm", label: "pnpm", code: "pnpm add corelink" },
];

describe("SnippetTabs", () => {
  beforeEach(() => {
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
  });

  it("renders the first tab's code by default", () => {
    renderWithProviders(<SnippetTabs tabs={tabs} />);
    expect(screen.getByText("npm install corelink")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "npm" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("switches the active tab on click", async () => {
    renderWithProviders(<SnippetTabs tabs={tabs} />);
    await userEvent.click(screen.getByRole("tab", { name: "pnpm" }));
    expect(screen.getByText("pnpm add corelink")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "pnpm" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});
