import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { ConfirmDialog } from "./ConfirmDialog";

describe("ConfirmDialog", () => {
  it("renders nothing when open is false", () => {
    renderWithProviders(
      <ConfirmDialog
        open={false}
        onClose={() => {}}
        onConfirm={() => {}}
        title="Delete tenant"
      />,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("renders the dialog with title when open", () => {
    renderWithProviders(
      <ConfirmDialog
        open
        onClose={() => {}}
        onConfirm={() => {}}
        title="Delete tenant"
        body="This cannot be undone"
      />,
    );
    expect(screen.getByRole("dialog", { name: "Delete tenant" })).toBeInTheDocument();
    expect(screen.getByText("This cannot be undone")).toBeInTheDocument();
  });

  it("Cancel calls onClose", async () => {
    const onClose = vi.fn();
    renderWithProviders(
      <ConfirmDialog open onClose={onClose} onConfirm={() => {}} title="Delete" />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
  });

  it("Confirm calls onConfirm", async () => {
    const onConfirm = vi.fn();
    renderWithProviders(
      <ConfirmDialog
        open
        onClose={() => {}}
        onConfirm={onConfirm}
        title="Delete"
        confirmLabel="Yes, delete"
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Yes, delete" }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });
});
