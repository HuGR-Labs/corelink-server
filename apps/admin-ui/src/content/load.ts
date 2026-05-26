/**
 * Edge-compat content loader — no node:fs / node:path / node:crypto.
 *
 * All markdown and JSON content is statically imported at build time so the
 * module tree is fully resolvable in Cloudflare Pages Edge Runtime.
 *
 * Public API is identical to the previous Node.js implementation so all
 * callers (privacy/page.tsx, legal/terms/page.tsx, legal/dpa/page.tsx,
 * privacy/sub-processors/page.tsx) require zero changes.
 */

import type { Locale } from "@/i18n/messages";

// Re-export types so existing callers that import from "@/content/load" keep
// working without any modification.
export type { SubProcessor, SubProcessorList } from "./sub-processors";
import { subProcessors } from "./sub-processors";
import type { SubProcessorList } from "./sub-processors";

// ── Static markdown content map ─────────────────────────────────────────────
// Each .ts module exports `content: string` generated from the corresponding
// .md source file. The map key is `"<base>.<locale>"`.

import { content as dpaDe } from "./dpa.de";
import { content as dpaEn } from "./dpa.en";
import { content as dpaEs } from "./dpa.es";
import { content as dpaPt } from "./dpa.pt";
import { content as privacyNoticeDe } from "./privacy-notice.de";
import { content as privacyNoticeEn } from "./privacy-notice.en";
import { content as privacyNoticeEs } from "./privacy-notice.es";
import { content as privacyNoticePt } from "./privacy-notice.pt";
import { content as tosDe } from "./tos.de";
import { content as tosEn } from "./tos.en";
import { content as tosEs } from "./tos.es";
import { content as tosPt } from "./tos.pt";

const CONTENT_MAP: Record<string, string> = {
  "dpa.de": dpaDe,
  "dpa.en": dpaEn,
  "dpa.es": dpaEs,
  "dpa.pt": dpaPt,
  "privacy-notice.de": privacyNoticeDe,
  "privacy-notice.en": privacyNoticeEn,
  "privacy-notice.es": privacyNoticeEs,
  "privacy-notice.pt": privacyNoticePt,
  "tos.de": tosDe,
  "tos.en": tosEn,
  "tos.es": tosEs,
  "tos.pt": tosPt,
};

/**
 * Load a localized markdown content string from the static map.
 * Falls back to English if the requested locale is missing.
 *
 * @param base file basename without `.<locale>.md`, e.g. `"privacy-notice"`
 * @param locale
 */
export function loadLocalizedMarkdown(base: string, locale: Locale): string {
  const localized = CONTENT_MAP[`${base}.${locale}`];
  if (localized !== undefined) return localized;
  const fallback = CONTENT_MAP[`${base}.en`];
  if (fallback !== undefined) return fallback;
  return `# ${base}\n\nContent unavailable.`;
}

export function loadSubProcessors(): SubProcessorList {
  return subProcessors;
}

/**
 * Extract the `**Version:** x.y.z` and `**Last updated:**` headers from the
 * leading lines of a markdown file. Returns `null` fields when missing.
 */
export function extractFrontMatterFromBody(
  md: string,
): { version: string | null; lastUpdated: string | null } {
  const versionMatch = md.match(
    /\*\*(?:Version|Versão|Versión):\*\*\s*([^\n]+)/i,
  );
  const updatedMatch = md.match(
    /\*\*(?:Last updated|Última atualização|Última actualización|Effective date|Vigência|Vigencia):\*\*\s*([^\n]+)/i,
  );
  return {
    version: versionMatch?.[1]?.trim() ?? null,
    lastUpdated: updatedMatch?.[1]?.trim() ?? null,
  };
}
