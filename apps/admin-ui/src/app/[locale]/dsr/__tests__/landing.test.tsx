import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import DsrLandingPage from "../page";
import { DSR_ACTIONS } from "@/lib/dsr-types";

describe("DSR landing page", () => {
  it("renders all 6 action buttons (Test 1)", () => {
    render(<DsrLandingPage params={{ locale: "en" }} />);
    for (const action of DSR_ACTIONS) {
      expect(
        screen.getByTestId(`dsr-action-button-${action}`),
      ).toBeInTheDocument();
    }
    expect(DSR_ACTIONS).toHaveLength(6);
  });

  it("falls back to en when an unsupported locale is passed", () => {
    render(<DsrLandingPage params={{ locale: "xx" }} />);
    // English label for "access" is "Access my data"
    expect(screen.getByText(/Access my data/i)).toBeInTheDocument();
  });

  it("renders rights in Portuguese when locale=pt", () => {
    render(<DsrLandingPage params={{ locale: "pt" }} />);
    expect(screen.getByText(/Acessar meus dados/)).toBeInTheDocument();
    expect(screen.getByText(/Retificar meus dados/)).toBeInTheDocument();
  });

  it("links to status route", () => {
    render(<DsrLandingPage params={{ locale: "en" }} />);
    const link = screen.getByTestId("dsr-status-link");
    expect(link).toHaveAttribute("href", "/en/dsr/status");
  });
});
