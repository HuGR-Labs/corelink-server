import { describe, it, expect, afterEach } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { ThemeToggle } from "./ThemeToggle";

describe("ThemeToggle", () => {
  afterEach(() => {
    document.documentElement.classList.remove("light");
    localStorage.clear();
  });

  it("renders a toggle button", () => {
    renderWithProviders(<ThemeToggle />);
    expect(screen.getByRole("button")).toBeInTheDocument();
  });

  it("toggles the 'light' class on the document element", async () => {
    renderWithProviders(<ThemeToggle />);
    expect(document.documentElement.classList.contains("light")).toBe(false);
    await userEvent.click(screen.getByRole("button"));
    expect(document.documentElement.classList.contains("light")).toBe(true);
  });
});
