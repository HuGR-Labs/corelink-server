/**
 * i18n coverage script — WI-S18-005 deliverable 1.
 *
 * Verifies every English source page has a counterpart in `pt-BR` and `es-419`
 * (or an explicit `<!-- i18n:TODO -->` marker). Fails CI if coverage < 80 % per
 * locale.
 *
 * Spec contract S-18 §5.6 R-S18-12 — 3 locales canonical (en-US default +
 * pt-BR LGPD + es-419 LATAM); missing translation = build fail
 * (Quality Standard 14.s18.5).
 *
 * Layout:
 *   apps/docs/docs/**\/*.md                       (English source)
 *   apps/docs/i18n/<locale>/docusaurus-plugin-content-docs/current/**\/*.md
 *
 * Usage:
 *   pnpm i18n-coverage                # all locales, default threshold 80 %
 *   pnpm i18n-coverage --threshold 90 # strict run for sprint close
 *
 * Exits non-zero if any locale falls below the threshold.
 */

import { promises as fs } from "node:fs";
import path from "node:path";
import process from "node:process";

// R-prep i18n-de: `de` joined as the fourth canonical locale (DACH enterprise
// GA). Coverage gate applies uniformly per-locale.
const LOCALES = ["pt-BR", "es-419", "de"] as const;
const DEFAULT_THRESHOLD = 0.8;

const REPO_ROOT = path.resolve(new URL(".", import.meta.url).pathname, "..");
const SOURCE_DIR = path.join(REPO_ROOT, "docs");
const I18N_BASE = path.join(REPO_ROOT, "i18n");

async function walk(dir: string, acc: string[] = []): Promise<string[]> {
  let entries: import("node:fs").Dirent[];
  try {
    entries = await fs.readdir(dir, { withFileTypes: true });
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === "ENOENT") return acc;
    throw err;
  }
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      await walk(full, acc);
    } else if (entry.isFile() && /\.(md|mdx)$/i.test(entry.name)) {
      acc.push(full);
    }
  }
  return acc;
}

async function hasTodoMarker(file: string): Promise<boolean> {
  try {
    const content = await fs.readFile(file, "utf8");
    return /<!--\s*i18n:TODO/i.test(content);
  } catch {
    return false;
  }
}

interface LocaleReport {
  locale: string;
  total: number;
  translated: number;
  todo: number;
  missing: string[];
  coverage: number;
}

async function reportLocale(
  locale: string,
  sourceFiles: string[],
): Promise<LocaleReport> {
  const localeRoot = path.join(
    I18N_BASE,
    locale,
    "docusaurus-plugin-content-docs",
    "current",
  );
  const missing: string[] = [];
  let translated = 0;
  let todo = 0;
  for (const src of sourceFiles) {
    const rel = path.relative(SOURCE_DIR, src);
    const target = path.join(localeRoot, rel);
    try {
      await fs.access(target);
      if (await hasTodoMarker(target)) {
        todo += 1;
      } else {
        translated += 1;
      }
    } catch {
      missing.push(rel);
    }
  }
  const total = sourceFiles.length;
  const counted = translated + todo;
  const coverage = total === 0 ? 1 : counted / total;
  return { locale, total, translated, todo, missing, coverage };
}

function parseArgs(argv: string[]): { threshold: number } {
  let threshold = DEFAULT_THRESHOLD;
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--threshold" && argv[i + 1]) {
      threshold = Number.parseFloat(argv[i + 1]);
      i += 1;
    }
  }
  if (!Number.isFinite(threshold) || threshold <= 0 || threshold > 1) {
    throw new Error(`invalid --threshold ${threshold} (expected 0 < x ≤ 1)`);
  }
  return { threshold };
}

async function main(): Promise<void> {
  const { threshold } = parseArgs(process.argv.slice(2));
  const sourceFiles = await walk(SOURCE_DIR);
  if (sourceFiles.length === 0) {
    console.warn(
      `[i18n-coverage] no source docs found at ${SOURCE_DIR}; ` +
        "WI-S18-001 has not landed yet — exiting 0 (vacuous pass).",
    );
    return;
  }

  const reports = await Promise.all(
    LOCALES.map((l) => reportLocale(l, sourceFiles)),
  );

  let failed = false;
  for (const r of reports) {
    const pct = (r.coverage * 100).toFixed(1);
    const status = r.coverage >= threshold ? "PASS" : "FAIL";
    console.log(
      `[${status}] locale=${r.locale} coverage=${pct}% ` +
        `(translated=${r.translated} todo=${r.todo} missing=${r.missing.length} ` +
        `total=${r.total} threshold=${(threshold * 100).toFixed(0)}%)`,
    );
    if (r.missing.length > 0) {
      const preview = r.missing.slice(0, 10).join("\n  - ");
      console.log(
        `  missing files (first 10 of ${r.missing.length}):\n  - ${preview}`,
      );
    }
    if (r.coverage < threshold) failed = true;
  }

  if (failed) {
    console.error(
      `\n[i18n-coverage] FAIL — at least one locale below threshold ` +
        `${(threshold * 100).toFixed(0)}%. Per spec contract S-18 §5.6 R-S18-12, ` +
        "missing translation = build fail (Quality Standard 14.s18.5).",
    );
    process.exitCode = 1;
  } else {
    console.log(
      `\n[i18n-coverage] OK — all locales ≥ ${(threshold * 100).toFixed(0)}%.`,
    );
  }
}

main().catch((err) => {
  console.error("[i18n-coverage] fatal error:", err);
  process.exit(2);
});
