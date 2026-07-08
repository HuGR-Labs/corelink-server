import { describe, it, expect } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { Stat } from "./Stat";

describe("Stat", () => {
  it("renders the label and value", () => {
    renderWithProviders(<Stat label="Requests" value="1,204" />);
    expect(screen.getByText("Requests")).toBeInTheDocument();
    expect(screen.getByText("1,204")).toBeInTheDocument();
  });

  it("renders the trend label when provided", () => {
    renderWithProviders(
      <Stat label="Requests" value="1,204" trend={{ dir: "up", label: "+12%" }} />,
    );
    expect(screen.getByText("+12%")).toBeInTheDocument();
  });
});
