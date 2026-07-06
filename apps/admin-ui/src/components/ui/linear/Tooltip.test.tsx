import { describe, it, expect } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Tooltip } from "./Tooltip";

describe("Tooltip", () => {
  it("renders its child trigger content", () => {
    renderWithProviders(
      <Tooltip label="Copy to clipboard">
        <span>trigger</span>
      </Tooltip>,
    );
    expect(screen.getByText("trigger")).toBeInTheDocument();
  });

  it("reveals the label on hover", async () => {
    renderWithProviders(
      <Tooltip label="Copy to clipboard">
        <span>trigger</span>
      </Tooltip>,
    );
    expect(screen.queryByRole("tooltip")).toBeNull();
    await userEvent.hover(screen.getByText("trigger"));
    expect(screen.getByRole("tooltip")).toHaveTextContent("Copy to clipboard");
  });
});
