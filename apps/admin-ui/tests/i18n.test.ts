import { describe, it, expect } from "vitest";
import en from "@/i18n/locales/en.json";
import pt from "@/i18n/locales/pt.json";
import es from "@/i18n/locales/es.json";
import { isLocale, LOCALES, DEFAULT_LOCALE } from "@/i18n/request";

const REQUIRED_KEYS = [
  "brand.name",
  "brand.tagline",
  "nav.dashboard",
  "nav.settings",
  "auth.signIn",
  "landing.heading",
  "landing.cta",
];

function get(obj: unknown, dotted: string): unknown {
  return dotted.split(".").reduce<unknown>((acc, key) => {
    if (acc && typeof acc === "object" && key in (acc as Record<string, unknown>)) {
      return (acc as Record<string, unknown>)[key];
    }
    return undefined;
  }, obj);
}

describe("i18n locale completeness", () => {
  it("LOCALES list matches available locale files", () => {
    expect(LOCALES).toEqual(["en", "pt", "es"]);
    expect(DEFAULT_LOCALE).toBe("en");
  });

  it("isLocale accepts known + rejects unknown", () => {
    expect(isLocale("en")).toBe(true);
    expect(isLocale("pt")).toBe(true);
    expect(isLocale("es")).toBe(true);
    expect(isLocale("fr")).toBe(false);
    expect(isLocale("")).toBe(false);
  });

  it("every required key resolves in every locale", () => {
    for (const [name, bundle] of [
      ["en", en],
      ["pt", pt],
      ["es", es],
    ] as const) {
      for (const key of REQUIRED_KEYS) {
        const value = get(bundle, key);
        expect(
          typeof value === "string" && value.length > 0,
          `${name}: missing or empty ${key}`,
        ).toBe(true);
      }
    }
  });

  it("locales never share identical CTA (proves real translation)", () => {
    const enCta = get(en, "landing.cta");
    const ptCta = get(pt, "landing.cta");
    const esCta = get(es, "landing.cta");
    expect(enCta).not.toBe(ptCta);
    expect(enCta).not.toBe(esCta);
  });
});
