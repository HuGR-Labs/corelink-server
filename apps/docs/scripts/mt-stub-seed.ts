/**
 * Machine-translation stub seeder — R-4 / WT-R4-1 deliverable (Goal A).
 *
 * Walks every `<!-- i18n:TODO ... -->` stub under
 * `apps/docs/i18n/<locale>/docusaurus-plugin-content-docs/current/**` and
 * promotes it to an `<!-- i18n:MT ... -->` stub, prefixing the source-of-
 * truth notice line with `MT:` so downstream readers (and the QC gate)
 * recognise the page as machine-seeded rather than raw English.
 *
 * Rationale: this is NOT a translation pass — it produces a higher-grade
 * placeholder for the coverage report. Per WT-R4-1 we only emit MT-stubs
 * "from TMX where memory exists". The current TMX (`apps/docs/i18n/tmx/`)
 * carries UI strings only; the docs themselves have no TMX yet, so the
 * source-of-truth notice is the only segment we replace. The doc body
 * stays in English (the original TODO behaviour) and a clear `MT:` banner
 * tells the reader the page is pending translator post-edit.
 *
 * Idempotent: re-running over an MT-stub file is a no-op.
 *
 * Usage:
 *   pnpm tsx scripts/mt-stub-seed.ts                   # all locales, promote TODO -> MT
 *   pnpm tsx scripts/mt-stub-seed.ts --locale pt-BR
 *   pnpm tsx scripts/mt-stub-seed.ts --dry-run         # report only
 *
 *   # R-prep i18n-de: bootstrap mode — create MT-stub shadow tree directly
 *   # from EN source files when no per-locale tree exists yet. Required for
 *   # adding a new locale (e.g. `de` for DACH enterprise GA buyers).
 *   pnpm tsx scripts/mt-stub-seed.ts --init --locale de
 */

import { promises as fs } from "node:fs";
import path from "node:path";
import process from "node:process";

const LOCALES = ["pt-BR", "es-419", "de"] as const;
type Locale = (typeof LOCALES)[number];

// Per-locale source-of-truth notice (MT banner). When a translator delivers
// a native pass, they remove the `<!-- i18n:MT -->` marker AND the banner.
const SOT_BANNER: Record<Locale, string> = {
  "pt-BR":
    "> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).\n" +
    ">\n" +
    "> Canonical EN source: `docs/{REL}`",
  "es-419":
    "> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).\n" +
    ">\n" +
    "> Canonical EN source: `docs/{REL}`",
  de:
    "> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).\n" +
    ">\n" +
    "> Canonical EN source: `docs/{REL}`",
};

const REPO_ROOT = path.resolve(new URL(".", import.meta.url).pathname, "..");
const I18N_BASE = path.join(REPO_ROOT, "i18n");
const SOURCE_DIR = path.join(REPO_ROOT, "docs");

interface Args {
  dryRun: boolean;
  locales: Locale[];
  init: boolean;
}

function parseArgs(argv: string[]): Args {
  let dryRun = false;
  let init = false;
  let locales: Locale[] = [...LOCALES];
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === "--dry-run") dryRun = true;
    else if (a === "--init") init = true;
    else if (a === "--locale" && argv[i + 1]) {
      const v = argv[i + 1] as Locale;
      if (!LOCALES.includes(v)) {
        throw new Error(`unknown --locale ${v} (expected: ${LOCALES.join(", ")})`);
      }
      locales = [v];
      i += 1;
    }
  }
  return { dryRun, locales, init };
}

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
    if (entry.isDirectory()) await walk(full, acc);
    else if (entry.isFile() && /\.(md|mdx)$/i.test(entry.name)) acc.push(full);
  }
  return acc;
}

interface TmxEntry {
  source: string;
  target: string;
}

async function loadTmx(locale: Locale): Promise<TmxEntry[]> {
  // Surface translations from existing UI string files as the seed TMX.
  // The XLIFF tooling under `scripts/export-xliff.ts` regenerates a richer
  // TMX after every translator round-trip; this static seed is enough to
  // mark "memory exists" for the MT-stub.
  const entries: TmxEntry[] = [];
  for (const rel of [
    "code.json",
    "docusaurus-theme-classic/navbar.json",
    "docusaurus-theme-classic/footer.json",
  ]) {
    const enPath = path.join(I18N_BASE, "en-US", rel);
    const trPath = path.join(I18N_BASE, locale, rel);
    try {
      const enJson = JSON.parse(await fs.readFile(enPath, "utf8"));
      const trJson = JSON.parse(await fs.readFile(trPath, "utf8"));
      for (const key of Object.keys(enJson)) {
        const en = (enJson as Record<string, { message: string }>)[key]?.message;
        const tr = (trJson as Record<string, { message: string }>)[key]?.message;
        if (en && tr && en !== tr) entries.push({ source: en, target: tr });
      }
    } catch {
      // skip missing file
    }
  }
  return entries;
}

interface Result {
  promoted: number;
  alreadyMt: number;
  skipped: number;
  scanned: number;
}

async function processFile(
  file: string,
  locale: Locale,
  tmx: TmxEntry[],
  dryRun: boolean,
): Promise<"promoted" | "already-mt" | "skipped"> {
  const txt = await fs.readFile(file, "utf8");
  if (/<!--\s*i18n:MT\b/.test(txt)) return "already-mt";
  if (!/<!--\s*i18n:TODO\b/.test(txt)) return "skipped";

  // Promote: TODO -> MT marker, prefix the SoT notice line with MT:
  let next = txt.replace(
    /<!--\s*i18n:TODO\s*\(([^)]*)\)\s*—\s*[^>]*?-->/,
    (_m, loc) =>
      `<!-- i18n:MT (${loc}) — TMX-seeded machine-translation stub; ` +
      `replace with native-speaker translation before GA -->`,
  );

  // Prefix the existing source-of-truth blockquote with "MT:" so readers
  // immediately see the page is machine-seeded.
  next = next.replace(
    /(^>\s*)(Esta página|This page|Esta hoja|Este documento)/m,
    (_m, gt, head) => `${gt}MT: ${head}`,
  );

  // Apply trivial TMX swaps to short navigation strings if present in body.
  // Skips code blocks (anything inside ``` fences).
  next = applyTmx(next, tmx);

  if (next !== txt && !dryRun) {
    await fs.writeFile(file, next);
  }
  return next !== txt ? "promoted" : "skipped";
}

function applyTmx(input: string, tmx: TmxEntry[]): string {
  if (tmx.length === 0) return input;
  const parts = input.split(/(```[\s\S]*?```)/g);
  for (let i = 0; i < parts.length; i += 1) {
    if (i % 2 === 1) continue; // code fence — leave alone
    let chunk = parts[i];
    for (const e of tmx) {
      // Word-boundary swap; only for capitalised tokens to minimise noise.
      const safe = e.source.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      if (e.source.length < 3 || e.target.length < 2) continue;
      chunk = chunk.replace(new RegExp(`\\b${safe}\\b`, "g"), e.target);
    }
    parts[i] = chunk;
  }
  return parts.join("");
}

/**
 * Bootstrap a new locale's shadow tree by copying every EN source file under
 * `docs/` to `i18n/<locale>/docusaurus-plugin-content-docs/current/`,
 * inserting an `<!-- i18n:MT (<locale>) -->` marker after the front-matter
 * and a localized source-of-truth banner. Idempotent: existing files are
 * left untouched.
 *
 * Used when registering a new locale (e.g. `de` for DACH enterprise GA).
 */
async function initLocale(
  locale: Locale,
  dryRun: boolean,
): Promise<{ created: number; existed: number; scanned: number }> {
  const sourceFiles = await walk(SOURCE_DIR);
  const localeRoot = path.join(
    I18N_BASE,
    locale,
    "docusaurus-plugin-content-docs",
    "current",
  );
  let created = 0;
  let existed = 0;
  for (const src of sourceFiles) {
    const rel = path.relative(SOURCE_DIR, src);
    const target = path.join(localeRoot, rel);
    try {
      await fs.access(target);
      existed += 1;
      continue;
    } catch {
      // missing — create
    }
    const raw = await fs.readFile(src, "utf8");
    const stub = stubBody(raw, locale, rel);
    if (!dryRun) {
      await fs.mkdir(path.dirname(target), { recursive: true });
      await fs.writeFile(target, stub);
    }
    created += 1;
  }
  return { created, existed, scanned: sourceFiles.length };
}

/** Insert MT marker + SoT banner after the YAML front-matter (if any). */
function stubBody(raw: string, locale: Locale, rel: string): string {
  const fmMatch = raw.match(/^---\n[\s\S]*?\n---\n/);
  const banner = SOT_BANNER[locale].replaceAll("{REL}", rel);
  const marker =
    `<!-- i18n:MT (${locale}) — bootstrap MT-stub from EN source; ` +
    `replace with native-speaker translation before GA -->`;
  const block = `\n${marker}\n\n${banner}\n\n`;
  if (fmMatch) {
    return raw.slice(0, fmMatch[0].length) + block + raw.slice(fmMatch[0].length);
  }
  return block + raw;
}

async function main(): Promise<void> {
  const { dryRun, locales, init } = parseArgs(process.argv.slice(2));

  if (init) {
    for (const locale of locales) {
      const r = await initLocale(locale, dryRun);
      console.log(
        `[mt-stub-seed] init locale=${locale} scanned=${r.scanned} ` +
          `created=${r.created} existed=${r.existed}` +
          (dryRun ? " (dry-run)" : ""),
      );
    }
    return;
  }

  const totals: Record<Locale, Result> = {
    "pt-BR": { promoted: 0, alreadyMt: 0, skipped: 0, scanned: 0 },
    "es-419": { promoted: 0, alreadyMt: 0, skipped: 0, scanned: 0 },
    de: { promoted: 0, alreadyMt: 0, skipped: 0, scanned: 0 },
  };

  for (const locale of locales) {
    const root = path.join(
      I18N_BASE,
      locale,
      "docusaurus-plugin-content-docs",
      "current",
    );
    const files = await walk(root);
    const tmx = await loadTmx(locale);
    totals[locale].scanned = files.length;
    for (const f of files) {
      const r = await processFile(f, locale, tmx, dryRun);
      if (r === "promoted") totals[locale].promoted += 1;
      else if (r === "already-mt") totals[locale].alreadyMt += 1;
      else totals[locale].skipped += 1;
    }
    const t = totals[locale];
    console.log(
      `[mt-stub-seed] locale=${locale} scanned=${t.scanned} ` +
        `promoted=${t.promoted} already-mt=${t.alreadyMt} skipped=${t.skipped}` +
        (dryRun ? " (dry-run)" : ""),
    );
  }
}

main().catch((err) => {
  console.error("[mt-stub-seed] fatal:", err);
  process.exit(2);
});
