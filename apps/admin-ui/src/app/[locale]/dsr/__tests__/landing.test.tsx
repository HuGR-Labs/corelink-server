import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import DsrLandingPage from "../page";
import { DSR_ACTIONS } from "@/lib/dsr-types";

// DsrLandingPage is an async server component (Next 15 `params` is a Promise).
// Await the component to resolve its element tree, then render it with RTL.
function renderLanding(locale: string) {
  return DsrLandingPage({ params: Promise.resolve({ locale }) });
}

describe("DSR landing page", () => {
  it("renders all 6 action buttons (Test 1)", async () => {
    render(await renderLanding("en"));
    for (const action of DSR_ACTIONS) {
      expect(
        screen.getByTestId(`dsr-action-button-${action}`),
      ).toBeInTheDocument();
    }
    expect(DSR_ACTIONS).toHaveLength(6);
  });

  it("falls back to en when an unsupported locale is passed", async () => {
    render(await renderLanding("xx"));
    // English label for "access" is "Access my data"
    expect(screen.getByText(/Access my data/i)).toBeInTheDocument();
  });

  it("renders rights in Portuguese when locale=pt", async () => {
    render(await renderLanding("pt"));
    expect(screen.getByText(/Acessar meus dados/)).toBeInTheDocument();
    expect(screen.getByText(/Retificar meus dados/)).toBeInTheDocument();
  });

  it("links to status route", async () => {
    render(await renderLanding("en"));
    const link = screen.getByTestId("dsr-status-link");
    expect(link).toHaveAttribute("href", "/en/dsr/status");
  });
});
