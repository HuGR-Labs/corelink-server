import fs from "node:fs";
import path from "node:path";
import type { Locale } from "@/i18n/LocaleContext";

const ROOT = path.join(process.cwd(), "src", "content");

/**
 * Synchronously load a localized markdown content file from `src/content`.
 * Falls back to English if the requested locale is missing.
 *
 * @param base file basename without `.<locale>.md`, e.g. `"privacy-notice"`
 * @param locale
 */
export function loadLocalizedMarkdown(base: string, locale: Locale): string {
  const candidates = [`${base}.${locale}.md`, `${base}.en.md`];
  for (const name of candidates) {
    const full = path.join(ROOT, name);
    if (fs.existsSync(full)) {
      return fs.readFileSync(full, "utf8");
    }
  }
  return `# ${base}\n\nContent unavailable.`;
}

export interface SubProcessor {
  id: string;
  name: string;
  role: string;
  region: string;
  certifications: string[];
  last_audit: string;
}

export interface SubProcessorList {
  version: string;
  items: SubProcessor[];
}

export function loadSubProcessors(): SubProcessorList {
  const full = path.join(ROOT, "sub-processors.json");
  const raw = fs.readFileSync(full, "utf8");
  return JSON.parse(raw) as SubProcessorList;
}

/**
 * Extract the `**Version:** x.y.z` and `**Last updated:**` headers from the
 * leading lines of a markdown file. Returns `null` fields when missing.
 */
export function extractFrontMatterFromBody(
  md: string
): { version: string | null; lastUpdated: string | null } {
  const versionMatch = md.match(/\*\*(?:Version|Versão|Versión):\*\*\s*([^\n]+)/i);
  const updatedMatch = md.match(
    /\*\*(?:Last updated|Última atualização|Última actualización|Effective date|Vigência|Vigencia):\*\*\s*([^\n]+)/i
  );
  return {
    version: versionMatch?.[1]?.trim() ?? null,
    lastUpdated: updatedMatch?.[1]?.trim() ?? null,
  };
}
