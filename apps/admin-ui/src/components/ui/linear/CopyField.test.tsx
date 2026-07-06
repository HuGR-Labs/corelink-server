import { describe, it, expect, vi, beforeEach } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { CopyField } from "./CopyField";

describe("CopyField", () => {
  beforeEach(() => {
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
  });

  it("renders the value", () => {
    renderWithProviders(<CopyField value="pat_live_abc123" />);
    expect(screen.getByText("pat_live_abc123")).toBeInTheDocument();
  });

  it("shows redactAs instead of the value when provided", () => {
    renderWithProviders(<CopyField value="secret-value" redactAs="••••••" />);
    expect(screen.getByText("••••••")).toBeInTheDocument();
    expect(screen.queryByText("secret-value")).toBeNull();
  });

  it("copies the real value even when redacted", async () => {
    renderWithProviders(<CopyField value="secret-value" redactAs="••••••" />);
    await userEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("secret-value");
  });
});
