/**
 * Single source of truth for the DPA click-through notice.
 *
 * Both DPA-first gates (team-invite AND paid-upgrade) load the notice through
 * this helper so the exact bytes the user sees, the SHA-256 hash bound into the
 * acceptance record, and the version string are all derived from ONE place —
 * the localized markdown in `src/content/dpa.*` (never fabricated, never
 * forked). Works in both RSC (server component) and the browser: it only uses
 * static string imports + Web Crypto (`crypto.subtle`), both edge/browser-safe.
 */

import { loadLocalizedMarkdown } from "@/content/load";
import type { Locale } from "@/i18n/messages";

/**
 * Canonical DPA version accepted by the backend tier-select / dpa-accept
 * routes. Sourced from `content/dpa.en.ts` (`**Version:** 1.0.0`). Locale is
 * irrelevant to the version — the same agreement is translated, not re-versioned
 * — so both gates send this exact value.
 */
export const CANONICAL_DPA_VERSION = "1.0.0";

export interface DpaNotice {
  locale: Locale;
  dpaText: string;
  dpaVersion: string;
  /** SHA-256 hex of `dpaText` — the exact bytes rendered to the user. */
  noticeTextHash: string;
}

/** SHA-256 hex digest via Web Crypto (available in RSC + the browser). */
export async function sha256Hex(text: string): Promise<string> {
  const buf = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(text),
  );
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/**
 * Load the localized DPA notice + its content hash. The hash is computed from
 * the SAME text this returns, so the acceptance record a caller sends is
 * guaranteed to anchor the bytes the user actually saw.
 */
export async function loadDpaNotice(locale: Locale): Promise<DpaNotice> {
  const dpaText = loadLocalizedMarkdown("dpa", locale);
  const noticeTextHash = await sha256Hex(dpaText);
  return {
    locale,
    dpaText,
    dpaVersion: CANONICAL_DPA_VERSION,
    noticeTextHash,
  };
}
