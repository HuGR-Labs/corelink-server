// Minimal i18n shim. Real implementation provided by WI-S16-001 (next-intl wiring).
// We only depend on a thin interface here so this WI compiles + tests in isolation.

export type Locale = "en-US" | "pt-BR" | "es-419";

export const SUPPORTED_LOCALES: Locale[] = ["en-US", "pt-BR", "es-419"];

export const DEFAULT_LOCALE: Locale = "en-US";

export function isLocale(value: string): value is Locale {
  return (SUPPORTED_LOCALES as readonly string[]).includes(value);
}

/**
 * Active rendered locale source — Lote 10.16 codex P0 canonical fix.
 * Prefers the `corelink_locale` cookie set by the Next.js middleware (which
 * reflects the user's actively rendered locale after any switcher action).
 * Falls back to `<html lang>` which is server-rendered. NEVER reads
 * `navigator.language` / `Accept-Language` — these can diverge from the
 * rendered locale and break proof-of-informed (CTRL-PRIV-CONSENT-005).
 */
export function getActiveLocale(): Locale {
  if (typeof document !== "undefined") {
    const cookie = readCookie("corelink_locale");
    if (cookie && isLocale(cookie)) return cookie;
    const htmlLang = document.documentElement?.lang;
    if (htmlLang && isLocale(htmlLang)) return htmlLang;
  }
  return DEFAULT_LOCALE;
}

function readCookie(name: string): string | null {
  if (typeof document === "undefined") return null;
  const cookies = document.cookie ? document.cookie.split("; ") : [];
  for (const pair of cookies) {
    const eq = pair.indexOf("=");
    if (eq === -1) continue;
    if (pair.slice(0, eq) === name) return decodeURIComponent(pair.slice(eq + 1));
  }
  return null;
}
