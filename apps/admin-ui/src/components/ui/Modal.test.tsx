import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Modal } from "./Modal";

describe("Modal", () => {
  it("renders dialog with accessible name when open", () => {
    renderWithProviders(
      <Modal open onOpenChange={() => {}} title="Confirm delete">
        Body
      </Modal>
    );
    const dialog = screen.getByRole("dialog", { name: "Confirm delete" });
    expect(dialog).toBeInTheDocument();
  });

  it("ESC closes the modal", async () => {
    const onOpenChange = vi.fn();
    renderWithProviders(
      <Modal open onOpenChange={onOpenChange} title="Confirm delete">
        Body
      </Modal>
    );
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("has no a11y violations", async () => {
    const { container, baseElement } = renderWithProviders(
      <Modal open onOpenChange={() => {}} title="Confirm delete" description="desc">
        <div>body</div>
      </Modal>
    );
    // axe over the portal as well
    expect(await axe(baseElement)).toHaveNoViolations();
    expect(container).toBeDefined();
  });
});
