import { describe, it, expect } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { HelpPopover } from "./HelpPopover";

describe("HelpPopover", () => {
  it("renders the trigger button", () => {
    renderWithProviders(<HelpPopover label="Info">Extra detail</HelpPopover>);
    expect(screen.getByRole("button", { name: "Info" })).toBeInTheDocument();
  });

  it("reveals its content on hover", async () => {
    renderWithProviders(<HelpPopover label="Info">Extra detail</HelpPopover>);
    expect(screen.queryByText("Extra detail")).toBeNull();
    await userEvent.hover(screen.getByRole("button", { name: "Info" }));
    expect(screen.getByText("Extra detail")).toBeInTheDocument();
  });
});
