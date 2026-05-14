import { describe, it, expect } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Tabs } from "./Tabs";

const items = [
  { value: "one", label: "One", content: <div>one body</div> },
  { value: "two", label: "Two", content: <div>two body</div> },
];

describe("Tabs", () => {
  it("renders accessible tablist with orientation", () => {
    renderWithProviders(<Tabs label="Sections" items={items} />);
    const list = screen.getByRole("tablist", { name: "Sections" });
    expect(list).toHaveAttribute("aria-orientation", "horizontal");
    expect(screen.getAllByRole("tab")).toHaveLength(2);
  });

  it("arrow key switches focused tab", async () => {
    renderWithProviders(<Tabs label="Sections" items={items} />);
    const tabs = screen.getAllByRole("tab");
    tabs[0].focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(tabs[1]).toHaveFocus();
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(<Tabs label="Sections" items={items} />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
