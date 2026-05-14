import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import Link from "next/link";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { Button } from "./Button";

describe("Button", () => {
  it("renders with accessible name and is focusable", async () => {
    renderWithProviders(<Button>Save</Button>);
    const btn = screen.getByRole("button", { name: "Save" });
    expect(btn).toBeInTheDocument();
    btn.focus();
    expect(btn).toHaveFocus();
  });

  it("announces disabled state via aria-disabled", () => {
    renderWithProviders(<Button disabled>Save</Button>);
    const btn = screen.getByRole("button", { name: "Save" });
    expect(btn).toHaveAttribute("aria-disabled", "true");
  });

  it("invokes onClick", async () => {
    const onClick = vi.fn();
    renderWithProviders(<Button onClick={onClick}>Go</Button>);
    await userEvent.click(screen.getByRole("button", { name: "Go" }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("supports asChild slot composition", () => {
    renderWithProviders(
      <Button asChild>
        <Link href="/x">link</Link>
      </Button>
    );
    expect(screen.getByRole("link", { name: "link" })).toHaveAttribute("href", "/x");
  });

  it("has no a11y violations", async () => {
    const { container } = renderWithProviders(<Button>Save</Button>);
    expect(await axe(container)).toHaveNoViolations();
  });
});
