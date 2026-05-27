import { describe, it, expect } from "vitest";
import { t } from "@/i18n/messages";

describe("i18n lookups", () => {
  it("returns localized strings per locale", () => {
    expect(t("en", "onboarding.tenant.title")).toMatch(/create your tenant/i);
    expect(t("pt", "onboarding.tenant.title")).toMatch(/crie seu tenant/i);
    expect(t("es", "onboarding.tenant.title")).toMatch(/crea tu tenant/i);
    // R-prep i18n-de — `de` joined as the fourth canonical locale.
    expect(t("de", "onboarding.tenant.title")).toMatch(/tenant anlegen/i);
  });

  it("returns localized strings for every onboarding step", () => {
    // Phase 0.C: `onboarding.billing.*` keys removed when the
    // in-wizard billing step was deleted (PLG defer-billing). The
    // post-signup upgrade flow uses Stripe-hosted Checkout which
    // localizes itself based on the Accept-Language header — we no
    // longer maintain a parallel set of billing strings here.
    const keys = [
      "onboarding.tenant.title",
      "onboarding.dpa.title",
      "onboarding.region_plan.title",
      "onboarding.pat.title",
      "onboarding.done.title",
    ];
    for (const k of keys) {
      for (const locale of ["en", "pt", "es", "de"] as const) {
        const v = t(locale, k);
        expect(v).not.toBe(k); // would mean key missing
        expect(v.length).toBeGreaterThan(0);
      }
    }
  });

  it("returns the key itself when missing (no throw)", () => {
    expect(t("en", "nonexistent.path")).toBe("nonexistent.path");
  });

  it("PAT modal warning string is present in all four locales", () => {
    const en = t("en", "onboarding.pat.modal_warning");
    const pt = t("pt", "onboarding.pat.modal_warning");
    const es = t("es", "onboarding.pat.modal_warning");
    const de = t("de", "onboarding.pat.modal_warning");
    for (const s of [en, pt, es, de]) {
      expect(s.length).toBeGreaterThan(20);
    }
  });
});
