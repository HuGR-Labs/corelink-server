import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import PricingPage from "@/app/[locale]/pricing/page";

async function renderPricingPage(locale = "en"): Promise<void> {
  const element = await PricingPage({
    params: Promise.resolve({ locale: locale as never }),
  });
  render(element);
}

describe("/[locale]/pricing page CTAs", () => {
  it("does not locale-prefix the locale-less Clerk sign-up route", async () => {
    await renderPricingPage("en");

    expect(screen.getByTestId("tier-cta-free")).toHaveAttribute("href", "/sign-up");
    expect(screen.getByTestId("tier-cta-pro")).toHaveAttribute("href", "/sign-up");
  });

  it("keeps upgrade CTAs under the active locale", async () => {
    await renderPricingPage("pt");

    expect(screen.getByTestId("tier-cta-runner_starter")).toHaveAttribute(
      "href",
      "/pt/upgrade?plan=runner_starter",
    );
  });
});
