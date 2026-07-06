import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Card } from "./Card";

describe("Card", () => {
  it("renders its children", () => {
    renderWithProviders(<Card>Body content</Card>);
    expect(screen.getByText("Body content")).toBeInTheDocument();
  });

  it("renders the title heading when provided", () => {
    renderWithProviders(
      <Card title="Usage" meta="last 30 days">
        Body
      </Card>,
    );
    expect(screen.getByRole("heading", { name: "Usage" })).toBeInTheDocument();
    expect(screen.getByText("last 30 days")).toBeInTheDocument();
  });
});
