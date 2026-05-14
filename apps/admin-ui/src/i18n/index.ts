/**
 * Lightweight i18n facade for WI-S16-004.
 *
 * The admin-ui depends on `next-intl` at runtime, but the DSR surface uses
 * a thin synchronous loader so unit tests (which don't boot Next.js) can
 * exercise locale strings directly.
 */

import en from "./messages-en.json";
import pt from "./messages-pt.json";
import es from "./messages-es.json";

export const SUPPORTED_LOCALES = ["en", "pt", "es"] as const;
export type Locale = (typeof SUPPORTED_LOCALES)[number];

export const MESSAGES: Record<Locale, unknown> = { en, pt, es };

export function isLocale(value: string): value is Locale {
  return (SUPPORTED_LOCALES as readonly string[]).includes(value);
}

/**
 * Resolve a dotted key (e.g. "dsr.form.submit") to the localised string.
 * Falls back to en if the key is missing on the requested locale, then to
 * the raw key (so missing keys are visible in tests).
 */
export function tFor(locale: Locale, key: string): string {
  const fromLocale = lookup(MESSAGES[locale], key);
  if (fromLocale !== null) return fromLocale;
  const fromEnglish = lookup(MESSAGES.en, key);
  if (fromEnglish !== null) return fromEnglish;
  return key;
}

function lookup(tree: unknown, key: string): string | null {
  const parts = key.split(".");
  let node: unknown = tree;
  for (const part of parts) {
    if (node === null || typeof node !== "object") return null;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string" ? node : null;
}

/** Interpolate `{var}` placeholders into a localised template. */
export function interpolate(
  template: string,
  vars: Record<string, string | number>,
): string {
  return template.replace(/\{(\w+)\}/g, (_, name: string) => {
    const v = vars[name];
    return v === undefined ? `{${name}}` : String(v);
  });
}
