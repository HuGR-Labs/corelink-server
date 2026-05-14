import type { Locale } from "./LocaleContext";

const BCP47: Record<Locale, string> = {
  en: "en-US",
  pt: "pt-BR",
  es: "es-419",
};

/**
 * Format a Date or ISO string per locale conventions.
 * Defaults to `medium` date style + short time.
 */
export function formatDate(
  value: Date | string | number,
  locale: Locale,
  options: Intl.DateTimeFormatOptions = { dateStyle: "medium" }
): string {
  const d = typeof value === "string" || typeof value === "number" ? new Date(value) : value;
  return new Intl.DateTimeFormat(BCP47[locale], options).format(d);
}

/**
 * Format a number per locale (uses grouping, fractional digits, etc.).
 */
export function formatNumber(
  value: number,
  locale: Locale,
  options: Intl.NumberFormatOptions = {}
): string {
  return new Intl.NumberFormat(BCP47[locale], options).format(value);
}

/**
 * Format a currency amount per locale.
 * @param amount numeric amount
 * @param currency ISO-4217 code (e.g. "USD", "BRL", "EUR")
 */
export function formatCurrency(amount: number, currency: string, locale: Locale): string {
  return new Intl.NumberFormat(BCP47[locale], {
    style: "currency",
    currency,
  }).format(amount);
}

export function bcp47(locale: Locale): string {
  return BCP47[locale];
}
