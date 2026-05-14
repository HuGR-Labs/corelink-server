import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderWithProviders, screen } from "@/test-utils/render";
import { axe } from "@/test-utils/axe";
import { ErrorBoundary } from "./ErrorBoundary";

function Boom(): React.ReactElement {
  throw new Error("boom");
}

describe("ErrorBoundary", () => {
  beforeEach(() => {
    vi.spyOn(console, "error").mockImplementation(() => {});
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders friendly fallback with report link", () => {
    renderWithProviders(
      <ErrorBoundary>
        <Boom />
      </ErrorBoundary>
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.getByText("Something went wrong.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Report issue" })).toBeInTheDocument();
  });

  it("renders children when no error", () => {
    renderWithProviders(
      <ErrorBoundary>
        <p>ok</p>
      </ErrorBoundary>
    );
    expect(screen.getByText("ok")).toBeInTheDocument();
  });

  it("fallback has no a11y violations", async () => {
    const { container } = renderWithProviders(
      <ErrorBoundary>
        <Boom />
      </ErrorBoundary>
    );
    expect(await axe(container)).toHaveNoViolations();
  });
});
