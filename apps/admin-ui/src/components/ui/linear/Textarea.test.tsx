import { describe, it, expect } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Textarea } from "./Textarea";

describe("Textarea", () => {
  it("renders a textarea with the base class", () => {
    renderWithProviders(<Textarea aria-label="Notes" />);
    expect(screen.getByLabelText("Notes")).toHaveClass("lin-textarea");
  });

  it("accepts typed input", async () => {
    renderWithProviders(<Textarea aria-label="Notes" />);
    const ta = screen.getByLabelText("Notes");
    await userEvent.type(ta, "hello");
    expect(ta).toHaveValue("hello");
  });
});
