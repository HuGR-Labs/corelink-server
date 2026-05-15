/**
 * XLIFF 2.1 import script — R-4 / H-16 acceleration deliverable.
 *
 * Reads translator-returned `.xlf` bundle and reconstructs MDX with translated
 * `<target>` segments substituted for English `<source>` segments, while
 * preserving frontmatter, fenced code blocks, and MDX components verbatim
 * (these were marked `translate="no"` at export time).
 *
 * Usage:
 *   pnpm tsx scripts/import-xliff.ts --in <dir>            # default dist/xliff
 *   pnpm tsx scripts/import-xliff.ts --in <dir> --dry-run  # diff, no writes
 *   pnpm tsx scripts/import-xliff.ts --locale pt-BR        # single locale
 *
 * Validation:
 *   - Empty `<target>` on a billable unit → flagged as remaining TODO.
 *   - Any unit count drift from current EN source → warning (re-export).
 *   - Diff vs existing stub shows changed / unchanged segments.
 *
 * Writes back to:
 *   apps/docs/i18n/<locale>/docusaurus-plugin-content-docs/current/<rel>.{md,mdx}
 *
 * Spec contract: R-4 (i18n parity at GA) + S-18 §5.6 R-S18-12.
 */

import { promises as fs } from "node:fs";
import path from "node:path";
import process from "node:process";

// R-prep i18n-de — `de` joined as the fourth canonical locale.
const LOCALES = ["pt-BR", "es-419", "de"] as const;
type Locale = (typeof LOCALES)[number];

const REPO_ROOT = path.resolve(new URL(".", import.meta.url).pathname, "..");
const SOURCE_DIR = path.join(REPO_ROOT, "docs");
const I18N_BASE = path.join(REPO_ROOT, "i18n");
const DEFAULT_IN = path.join(REPO_ROOT, "dist", "xliff");

interface Args {
  inDir: string;
  dryRun: boolean;
  locales: Locale[];
}

function parseArgs(argv: string[]): Args {
  let inDir = DEFAULT_IN;
  let dryRun = false;
  let locales: Locale[] = [...LOCALES];
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--in" && argv[i + 1]) {
      inDir = path.resolve(argv[i + 1]);
      i += 1;
    } else if (arg === "--dry-run") {
      dryRun = true;
    } else if (arg === "--locale" && argv[i + 1]) {
      const v = argv[i + 1] as Locale;
      if (!LOCALES.includes(v)) {
        throw new Error(`unknown --locale ${v}`);
      }
      locales = [v];
      i += 1;
    }
  }
  return { inDir, dryRun, locales };
}

async function walk(dir: string, suffix: string, acc: string[] = []): Promise<string[]> {
  let entries: import("node:fs").Dirent[];
  try {
    entries = await fs.readdir(dir, { withFileTypes: true });
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === "ENOENT") return acc;
    throw err;
  }
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) await walk(full, suffix, acc);
    else if (entry.isFile() && entry.name.endsWith(suffix)) acc.push(full);
  }
  return acc;
}

function xmlUnescape(s: string): string {
  return s
    .replaceAll("&apos;", "'")
    .replaceAll("&quot;", '"')
    .replaceAll("&gt;", ">")
    .replaceAll("&lt;", "<")
    .replaceAll("&amp;", "&");
}

interface ParsedUnit {
  id: string;
  protectedUnit: boolean;
  kind: "frontmatter" | "codeblock" | "prose";
  source: string;
  target: string;
}

/**
 * Tiny XLIFF 2.1 parser — we control the producer (export-xliff.ts) so we
 * don't need a generic XML library. Each `<unit>` carries:
 *   - `id` attribute
 *   - optional `translate="no"` (protected)
 *   - one `<note category="kind">` with value frontmatter|codeblock|prose
 *   - one `<segment>` with `<source>` + `<target>`
 */
function parseXliff(xml: string): ParsedUnit[] {
  const units: ParsedUnit[] = [];
  const unitRe = /<unit\s+id="([^"]+)"([^>]*)>([\s\S]*?)<\/unit>/g;
  let m: RegExpExecArray | null;
  while ((m = unitRe.exec(xml)) !== null) {
    const id = m[1];
    const attrs = m[2];
    const inner = m[3];
    const protectedUnit = /\btranslate\s*=\s*"no"/.test(attrs);
    const kindMatch = inner.match(
      /<note\s+category="kind">([^<]+)<\/note>/,
    );
    const kindRaw = kindMatch ? kindMatch[1] : "prose";
    const kind = (kindRaw === "frontmatter" || kindRaw === "codeblock"
      ? kindRaw
      : "prose") as ParsedUnit["kind"];
    const sourceMatch = inner.match(
      /<source[^>]*>([\s\S]*?)<\/source>/,
    );
    const targetMatch = inner.match(
      /<target[^>]*>([\s\S]*?)<\/target>/,
    );
    const source = sourceMatch ? xmlUnescape(sourceMatch[1]) : "";
    const target = targetMatch ? xmlUnescape(targetMatch[1]) : "";
    units.push({ id, protectedUnit, kind, source, target });
  }
  return units;
}

/**
 * Rebuild MDX from parsed units. Same join logic as the exporter's split:
 *   - frontmatter block (one unit) followed by blank line
 *   - subsequent units separated by blank lines
 *   - code blocks are their own atomic units
 */
function reconstruct(units: ParsedUnit[]): { content: string; todos: number } {
  const out: string[] = [];
  let todos = 0;
  for (let i = 0; i < units.length; i += 1) {
    const u = units[i];
    let text: string;
    if (u.protectedUnit) {
      // For protected units the translator should not have touched <target>.
      // Prefer source as canonical truth.
      text = u.source;
    } else {
      const trimmed = u.target.trim();
      if (trimmed.length === 0) {
        todos += 1;
        // Fall back to source so the doc still builds; flag count for caller.
        text = u.source;
      } else {
        text = u.target;
      }
    }
    out.push(text);
    if (i < units.length - 1) out.push("");
  }
  let content = out.join("\n");
  if (!content.endsWith("\n")) content += "\n";
  return { content, todos };
}

interface ImportReport {
  locale: Locale;
  rel: string;
  units: number;
  todos: number;
  changed: boolean;
  /** 0..1 — units whose <target> matches existing stub text. */
  unchangedRatio: number;
}

async function exists(p: string): Promise<boolean> {
  try {
    await fs.access(p);
    return true;
  } catch {
    return false;
  }
}

async function processOne(
  xlfPath: string,
  locale: Locale,
  args: Args,
): Promise<ImportReport> {
  const xml = await fs.readFile(xlfPath, "utf8");
  const units = parseXliff(xml);
  const { content, todos } = reconstruct(units);

  // Recover relative path: <locale>/<rel>.xlf inside the bundle.
  const inLocaleDir = path.join(args.inDir, locale);
  const relWithExt = path.relative(inLocaleDir, xlfPath); // foo/bar.mdx.xlf
  const rel = relWithExt.replace(/\.xlf$/i, ""); // foo/bar.mdx
  const targetPath = path.join(
    I18N_BASE,
    locale,
    "docusaurus-plugin-content-docs",
    "current",
    rel,
  );

  let unchangedRatio = 0;
  let changed = true;
  if (await exists(targetPath)) {
    const existing = await fs.readFile(targetPath, "utf8");
    changed = existing !== content;
    // Cheap heuristic for unchanged ratio: count units whose <target> equals
    // <source> (i.e. translator left as-is or kept English).
    const sameSrcCount = units.filter(
      (u) => !u.protectedUnit && u.target.trim() === u.source.trim(),
    ).length;
    const billable = units.filter((u) => !u.protectedUnit).length || 1;
    unchangedRatio = sameSrcCount / billable;
  }

  if (!args.dryRun) {
    await fs.mkdir(path.dirname(targetPath), { recursive: true });
    await fs.writeFile(targetPath, content, "utf8");
  }

  return {
    locale,
    rel,
    units: units.length,
    todos,
    changed,
    unchangedRatio,
  };
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  console.log(
    `[import-xliff] in=${args.inDir} locales=${args.locales.join(",")} ` +
      `dry-run=${args.dryRun}`,
  );

  const sourceFiles = await walk(SOURCE_DIR, ".mdx").then((mdx) =>
    walk(SOURCE_DIR, ".md").then((md) => [...mdx, ...md]),
  );
  const expectedCount = sourceFiles.length;

  let grandTodos = 0;
  let grandFiles = 0;
  let grandChanged = 0;
  for (const locale of args.locales) {
    const dir = path.join(args.inDir, locale);
    const xlfs = await walk(dir, ".xlf");
    if (xlfs.length === 0) {
      console.warn(
        `[import-xliff] locale=${locale} found no .xlf files under ${dir} — skipping.`,
      );
      continue;
    }
    if (xlfs.length !== expectedCount) {
      console.warn(
        `[import-xliff] locale=${locale} count drift: bundle has ${xlfs.length} files, ` +
          `EN source has ${expectedCount}. Re-export recommended after import.`,
      );
    }

    let localeTodos = 0;
    let localeChanged = 0;
    for (const xlf of xlfs) {
      const r = await processOne(xlf, locale, args);
      localeTodos += r.todos;
      if (r.changed) localeChanged += 1;
      grandFiles += 1;
    }
    grandTodos += localeTodos;
    grandChanged += localeChanged;
    console.log(
      `[import-xliff] locale=${locale} files=${xlfs.length} ` +
        `changed=${localeChanged} remaining-todos=${localeTodos}`,
    );
  }

  console.log(
    `[import-xliff] total files=${grandFiles} changed=${grandChanged} ` +
      `remaining-todos=${grandTodos}`,
  );

  if (grandTodos > 0) {
    console.warn(
      `[import-xliff] ${grandTodos} segments still untranslated — ` +
        "ship-blocker before R-4 gate.",
    );
  }
}

main().catch((err) => {
  console.error("[import-xliff] fatal error:", err);
  process.exit(2);
});
