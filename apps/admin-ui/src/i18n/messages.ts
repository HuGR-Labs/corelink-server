/**
 * Onboarding i18n message bundle loader.
 *
 * We import JSON statically so missing keys fail at build time, and so the
 * lookup function is synchronous (used inside RSC + client components).
 */

import en from "./locales/en.json";
import pt from "./locales/pt.json";
import es from "./locales/es.json";

export type Locale = "en" | "pt" | "es";

const BUNDLES: Record<Locale, Record<string, unknown>> = {
  en: en as Record<string, unknown>,
  pt: pt as Record<string, unknown>,
  es: es as Record<string, unknown>,
};

export function getMessages(locale: Locale): Record<string, unknown> {
  return BUNDLES[locale] ?? BUNDLES.en;
}

/** Lookup a dotted key in a locale bundle. Returns key itself if missing. */
export function t(locale: Locale, key: string): string {
  const parts = key.split(".");
  let cur: unknown = BUNDLES[locale];
  for (const p of parts) {
    if (cur && typeof cur === "object" && p in (cur as Record<string, unknown>)) {
      cur = (cur as Record<string, unknown>)[p];
    } else {
      return key;
    }
  }
  return typeof cur === "string" ? cur : key;
}
