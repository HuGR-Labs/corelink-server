/**
 * next-intl request config (WI-S16-001).
 *
 * Loads messages from src/i18n/locales/<locale>.json. Falls back to 'en'
 * when the Accept-Language locale is missing. Missing translation = build
 * fail is enforced by the CI typecheck step + next-intl strict mode.
 */

import { getRequestConfig } from "next-intl/server";
import type { AbstractIntlMessages } from "next-intl";

export const LOCALES = ["en", "pt", "es"] as const;
export const DEFAULT_LOCALE: (typeof LOCALES)[number] = "en";
export type Locale = (typeof LOCALES)[number];

export function isLocale(value: string): value is Locale {
  return (LOCALES as readonly string[]).includes(value);
}

export default getRequestConfig(async ({ requestLocale }) => {
  const requested = await requestLocale;
  const resolved: Locale =
    requested && isLocale(requested) ? requested : DEFAULT_LOCALE;
  const mod = (await import(`./locales/${resolved}.json`)) as {
    default: AbstractIntlMessages;
  };
  return { locale: resolved, messages: mod.default };
});
