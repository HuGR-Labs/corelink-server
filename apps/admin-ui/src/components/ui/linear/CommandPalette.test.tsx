import { describe, it, expect, vi } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { CommandPalette } from "./CommandPalette";

function mkItems(onSelect = vi.fn()) {
  return [
    { id: "settings", label: "Open settings", onSelect },
    { id: "billing", label: "Open billing", onSelect },
  ];
}

describe("CommandPalette", () => {
  it("is closed initially (renders nothing)", () => {
    const { container } = renderWithProviders(<CommandPalette items={mkItems()} />);
    expect(container.firstChild).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("opens on Cmd/Ctrl+K", async () => {
    renderWithProviders(<CommandPalette items={mkItems()} />);
    await userEvent.keyboard("{Meta>}k{/Meta}");
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
  });

  it("filters options by query and selects on Enter", async () => {
    const onSelect = vi.fn();
    renderWithProviders(<CommandPalette items={mkItems(onSelect)} />);
    await userEvent.keyboard("{Meta>}k{/Meta}");

    const input = screen.getByRole("textbox", { name: "Command palette input" });
    await userEvent.type(input, "billing");

    expect(screen.getByText("Open billing")).toBeInTheDocument();
    expect(screen.queryByText("Open settings")).toBeNull();

    await userEvent.type(input, "{Enter}");
    expect(onSelect).toHaveBeenCalledOnce();
  });
});
