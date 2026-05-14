import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Drawer } from "./Drawer";

describe("Drawer", () => {
  it("renders dialog with accessible name", () => {
    renderWithProviders(
      <Drawer open onOpenChange={() => {}} title="Filters">
        body
      </Drawer>
    );
    expect(screen.getByRole("dialog", { name: "Filters" })).toBeInTheDocument();
  });

  it("ESC closes the drawer", async () => {
    const onOpenChange = vi.fn();
    renderWithProviders(
      <Drawer open onOpenChange={onOpenChange} title="Filters">
        body
      </Drawer>
    );
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("has no a11y violations", async () => {
    const { baseElement } = renderWithProviders(
      <Drawer open onOpenChange={() => {}} title="Filters">
        body
      </Drawer>
    );
    expect(await axe(baseElement)).toHaveNoViolations();
  });
});
