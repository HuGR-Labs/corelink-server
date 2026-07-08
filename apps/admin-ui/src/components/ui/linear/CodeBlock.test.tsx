import { describe, it, expect, vi, beforeEach } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { CodeBlock } from "./CodeBlock";

describe("CodeBlock", () => {
  beforeEach(() => {
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
  });

  it("renders the code", () => {
    renderWithProviders(<CodeBlock code="npm install corelink" />);
    expect(screen.getByText("npm install corelink")).toBeInTheDocument();
  });

  it("copies the code to the clipboard on click", async () => {
    renderWithProviders(<CodeBlock code="echo hi" />);
    await userEvent.click(screen.getByRole("button", { name: "Copy code" }));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("echo hi");
  });
});
