import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Menu, MenuItem } from "./Menu";

describe("Menu", () => {
  it("opens on trigger click and fires MenuItem onClick", async () => {
    const onClick = vi.fn();
    renderWithProviders(
      <Menu trigger={<button>Actions</button>}>
        <MenuItem onClick={onClick}>Rename</MenuItem>
      </Menu>,
    );

    expect(screen.queryByRole("menu")).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "Actions" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    expect(onClick).toHaveBeenCalledOnce();
  });
});
