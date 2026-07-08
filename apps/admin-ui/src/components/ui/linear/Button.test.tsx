import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Button } from "./Button";

describe("Button (linear)", () => {
  it("renders its children", () => {
    renderWithProviders(<Button>Save</Button>);
    expect(screen.getByRole("button", { name: "Save" })).toBeInTheDocument();
  });

  it("applies the variant class", () => {
    renderWithProviders(<Button variant="danger">Delete</Button>);
    expect(screen.getByRole("button", { name: "Delete" })).toHaveClass(
      "lin-btn",
      "lin-btn--danger",
    );
  });

  it("shows the spinner and disables while loading", () => {
    const { container } = renderWithProviders(<Button loading>Save</Button>);
    const btn = screen.getByRole("button", { name: "Save" });
    expect(btn).toBeDisabled();
    expect(container.querySelector(".lin-btn__spin")).not.toBeNull();
  });

  it("invokes onClick", async () => {
    const onClick = vi.fn();
    renderWithProviders(<Button onClick={onClick}>Go</Button>);
    await userEvent.click(screen.getByRole("button", { name: "Go" }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("renders an anchor with the kit classes when href is given", () => {
    renderWithProviders(
      <Button href="/api/install/github" variant="ghost" size="sm">
        Install
      </Button>,
    );
    const link = screen.getByRole("link", { name: "Install" });
    expect(link).toHaveAttribute("href", "/api/install/github");
    expect(link).toHaveClass("lin-btn", "lin-btn--ghost", "lin-btn--sm");
    // The anchor variant is a link, never a <button>.
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});
