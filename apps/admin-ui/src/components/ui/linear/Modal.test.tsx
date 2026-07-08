import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Modal } from "./Modal";

describe("Modal (linear)", () => {
  it("renders nothing when open is false", () => {
    renderWithProviders(
      <Modal open={false} onClose={() => {}} title="Confirm">
        Body
      </Modal>,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("renders the dialog with accessible name when open", () => {
    renderWithProviders(
      <Modal open onClose={() => {}} title="Confirm delete">
        Body
      </Modal>,
    );
    expect(screen.getByRole("dialog", { name: "Confirm delete" })).toBeInTheDocument();
    expect(screen.getByText("Body")).toBeInTheDocument();
  });

  it("Escape calls onClose", async () => {
    const onClose = vi.fn();
    renderWithProviders(
      <Modal open onClose={onClose} title="Confirm">
        Body
      </Modal>,
    );
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledOnce();
  });
});
