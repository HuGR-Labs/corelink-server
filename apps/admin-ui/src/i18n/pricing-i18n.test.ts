/**
 * i18n parity for the pricing page strings — in particular the new
 * runner-section keys. Every canonical locale (en/pt/es/de) MUST carry
 * the same `pricing.*` keys so no locale renders a raw dotted key.
 */

import { describe, it, expect } from "vitest";
import { t, type Locale } from "@/i18n/messages";

const LOCALES: readonly Locale[] = ["en", "pt", "es", "de"];

const PRICING_KEYS = [
  "pricing.title",
  "pricing.subtitle",
  "pricing.mostPopular",
  "pricing.footerNote",
  "pricing.cacheHeading",
  "pricing.runnerHeading",
  "pricing.runnerSubtitle",
  "pricing.cta.signUpFree",
  "pricing.cta.signUp",
  "pricing.cta.talkToUs",
  "pricing.cta.getRunners",
] as const;

describe("pricing i18n key parity", () => {
  it("every pricing key resolves (never the raw key) in every locale", () => {
    for (const locale of LOCALES) {
      for (const key of PRICING_KEYS) {
        const v = t(locale, key);
        expect(v, `${locale}:${key}`).not.toBe(key);
        expect(v.length, `${locale}:${key}`).toBeGreaterThan(0);
      }
    }
  });

  it("runner-section headers are present in all four locales", () => {
    for (const locale of LOCALES) {
      expect(t(locale, "pricing.runnerHeading").length).toBeGreaterThan(2);
      expect(t(locale, "pricing.runnerSubtitle").length).toBeGreaterThan(20);
      expect(t(locale, "pricing.cta.getRunners").length).toBeGreaterThan(2);
    }
  });
});
