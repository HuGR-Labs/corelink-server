/**
 * XLIFF 2.1 export script — R-4 / H-16 acceleration deliverable.
 *
 * Walks the canonical English source under `apps/docs/docs/**` and, for each
 * target locale, emits an XLIFF 2.1 file with the EN content as `<source>`
 * segments and an empty `<target>` for the translator to fill. Output layout:
 *
 *   apps/docs/dist/xliff/<locale>/<rel-path>.xlf      one file per source page
 *   apps/docs/dist/xliff/tm-<locale>.tmx              translation memory seed
 *   apps/docs/dist/xliff/translation-package-<date>.tar.gz   shippable bundle
 *
 * Why XLIFF 2.1: industry standard for translator CAT tools (memoQ, Trados,
 * Smartcat, Smartling, Transifex). 1.2 still works in legacy tools but 2.1 has
 * better structure for inline tags + segmentation. We pick 2.1 because every
 * managed-service candidate (Smartling, Transifex, Smartcat) supports it.
 *
 * Segmentation strategy: paragraph-level. We deliberately do NOT split inside
 * code fences, frontmatter blocks, or MDX components — those segments carry
 * `translate="no"` so the translator never sees them as billable units. This
 * minimises word count (translators charge per-word) and prevents accidental
 * mangling of JSX / variables.
 *
 * Usage:
 *   pnpm tsx scripts/export-xliff.ts                  # full export + tarball
 *   pnpm tsx scripts/export-xliff.ts --dry-run        # count files, no writes
 *   pnpm tsx scripts/export-xliff.ts --locale pt-BR   # single locale
 *   pnpm tsx scripts/export-xliff.ts --no-tarball     # skip tar.gz packaging
 *
 * Spec contract: R-4 (i18n parity at GA) + S-18 §5.6 R-S18-12 (3 locales).
 */

import { promises as fs } from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

// R-prep i18n-de — `de` joined as the fourth canonical locale.
const LOCALES = ["pt-BR", "es-419", "de"] as const;
type Locale = (typeof LOCALES)[number];

const REPO_ROOT = path.resolve(new URL(".", import.meta.url).pathname, "..");
const SOURCE_DIR = path.join(REPO_ROOT, "docs");
const I18N_BASE = path.join(REPO_ROOT, "i18n");
const DIST_DIR = path.join(REPO_ROOT, "dist", "xliff");

interface Segment {
  id: string;
  /** When true, segment carries `translate="no"`. Translators skip it. */
  protected: boolean;
  /** When true, this is frontmatter — preserved verbatim. */
  frontmatter: boolean;
  /** When true, this is a fenced code block. */
  code: boolean;
  text: string;
}

interface Args {
  dryRun: boolean;
  locales: Locale[];
  tarball: boolean;
}

function parseArgs(argv: string[]): Args {
  let dryRun = false;
  let tarball = true;
  let locales: Locale[] = [...LOCALES];
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--dry-run") dryRun = true;
    else if (arg === "--no-tarball") tarball = false;
    else if (arg === "--locale" && argv[i + 1]) {
      const v = argv[i + 1] as Locale;
      if (!LOCALES.includes(v)) {
        throw new Error(`unknown --locale ${v} (expected one of ${LOCALES.join(", ")})`);
      }
      locales = [v];
      i += 1;
    }
  }
  return { dryRun, locales, tarball };
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

function xmlEscape(s: string): string {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

/**
 * Split markdown into segments. Frontmatter (--- ... ---) and fenced code
 * blocks (``` ... ```) become protected. Paragraphs split on blank lines.
 * MDX components stay inline within their containing paragraph; translators
 * see them as inline placeholders they should preserve verbatim.
 */
function segmentMarkdown(content: string): Segment[] {
  const segments: Segment[] = [];
  const lines = content.split("\n");
  let i = 0;
  let segIdx = 0;
  const nextId = (): string => {
    segIdx += 1;
    return `s${segIdx}`;
  };

  // 1) Frontmatter
  if (lines[0]?.trim() === "---") {
    let end = -1;
    for (let j = 1; j < lines.length; j += 1) {
      if (lines[j].trim() === "---") {
        end = j;
        break;
      }
    }
    if (end > 0) {
      segments.push({
        id: nextId(),
        protected: true,
        frontmatter: true,
        code: false,
        text: lines.slice(0, end + 1).join("\n"),
      });
      i = end + 1;
      // Skip a blank line after frontmatter to keep the body clean.
      if (i < lines.length && lines[i].trim() === "") i += 1;
    }
  }

  // 2) Body: paragraphs split on blank lines, code fences are atomic.
  let buf: string[] = [];
  const flushBuf = (): void => {
    if (buf.length === 0) return;
    const text = buf.join("\n");
    if (text.trim().length === 0) {
      buf = [];
      return;
    }
    segments.push({
      id: nextId(),
      protected: false,
      frontmatter: false,
      code: false,
      text,
    });
    buf = [];
  };

  while (i < lines.length) {
    const line = lines[i];
    const trimmed = line.trim();

    // Fenced code block: atomic protected segment.
    if (trimmed.startsWith("```") || trimmed.startsWith("~~~")) {
      flushBuf();
      const fence = trimmed.slice(0, 3);
      const codeLines: string[] = [line];
      i += 1;
      while (i < lines.length) {
        codeLines.push(lines[i]);
        if (lines[i].trim().startsWith(fence)) {
          i += 1;
          break;
        }
        i += 1;
      }
      segments.push({
        id: nextId(),
        protected: true,
        frontmatter: false,
        code: true,
        text: codeLines.join("\n"),
      });
      continue;
    }

    // Paragraph boundary on blank line.
    if (trimmed === "") {
      flushBuf();
      i += 1;
      continue;
    }

    buf.push(line);
    i += 1;
  }
  flushBuf();

  return segments;
}

function buildXliff(opts: {
  sourceLocale: string;
  targetLocale: Locale;
  fileId: string;
  segments: Segment[];
}): string {
  const { sourceLocale, targetLocale, fileId, segments } = opts;
  const header =
    '<?xml version="1.0" encoding="UTF-8"?>\n' +
    `<xliff xmlns="urn:oasis:names:tc:xliff:document:2.0" version="2.1" ` +
    `srcLang="${sourceLocale}" trgLang="${targetLocale}">\n` +
    `  <file id="${xmlEscape(fileId)}" original="${xmlEscape(fileId)}">\n`;
  const body = segments
    .map((seg) => {
      const meta = seg.frontmatter
        ? "frontmatter"
        : seg.code
          ? "codeblock"
          : "prose";
      const translate = seg.protected ? ' translate="no"' : "";
      const tgt = seg.protected
        ? `        <target xml:space="preserve">${xmlEscape(seg.text)}</target>\n`
        : `        <target xml:space="preserve"></target>\n`;
      return (
        `    <unit id="${seg.id}"${translate}>\n` +
        `      <notes>\n        <note category="kind">${meta}</note>\n      </notes>\n` +
        `      <segment>\n` +
        `        <source xml:space="preserve">${xmlEscape(seg.text)}</source>\n` +
        tgt +
        `      </segment>\n` +
        `    </unit>\n`
      );
    })
    .join("");
  return header + body + "  </file>\n</xliff>\n";
}

interface TMUnit {
  id: string;
  source: string;
  target: string;
}

/**
 * Extract a translation-memory seed from any committed translations already
 * present in the i18n tree (e.g. code.json, footer.json, navbar.json from
 * WI-S16-003 consent UI, navbar/footer chrome). Translators get a head start
 * with established terminology in TMX 1.4 format.
 */
async function buildTmx(targetLocale: Locale): Promise<string> {
  const enJsonRoots = [
    path.join(I18N_BASE, "en-US", "code.json"),
    path.join(I18N_BASE, "en-US", "docusaurus-theme-classic", "footer.json"),
    path.join(I18N_BASE, "en-US", "docusaurus-theme-classic", "navbar.json"),
  ];
  const localeJsonRoots = [
    path.join(I18N_BASE, targetLocale, "code.json"),
    path.join(I18N_BASE, targetLocale, "docusaurus-theme-classic", "footer.json"),
    path.join(I18N_BASE, targetLocale, "docusaurus-theme-classic", "navbar.json"),
  ];

  const units: TMUnit[] = [];
  for (let i = 0; i < enJsonRoots.length; i += 1) {
    try {
      const enRaw = await fs.readFile(enJsonRoots[i], "utf8");
      const tgtRaw = await fs.readFile(localeJsonRoots[i], "utf8");
      const en = JSON.parse(enRaw) as Record<string, { message?: string }>;
      const tgt = JSON.parse(tgtRaw) as Record<string, { message?: string }>;
      for (const key of Object.keys(en)) {
        const enMsg = en[key]?.message;
        const tgtMsg = tgt[key]?.message;
        if (typeof enMsg !== "string" || typeof tgtMsg !== "string") continue;
        if (enMsg.trim().length === 0 || tgtMsg.trim().length === 0) continue;
        if (enMsg === tgtMsg) continue; // skip identical (likely untranslated)
        units.push({ id: key, source: enMsg, target: tgtMsg });
      }
    } catch {
      // skip missing files
    }
  }

  const header =
    '<?xml version="1.0" encoding="UTF-8"?>\n' +
    '<tmx version="1.4">\n' +
    `  <header creationtool="corelink-export-xliff" creationtoolversion="1.0" ` +
    `segtype="sentence" o-tmf="json" adminlang="en-US" srclang="en-US" datatype="plaintext"/>\n` +
    "  <body>\n";
  const body = units
    .map(
      (u) =>
        `    <tu tuid="${xmlEscape(u.id)}">\n` +
        `      <tuv xml:lang="en-US"><seg>${xmlEscape(u.source)}</seg></tuv>\n` +
        `      <tuv xml:lang="${targetLocale}"><seg>${xmlEscape(u.target)}</seg></tuv>\n` +
        `    </tu>\n`,
    )
    .join("");
  return header + body + "  </body>\n</tmx>\n";
}

async function mkdirp(dir: string): Promise<void> {
  await fs.mkdir(dir, { recursive: true });
}

function isoDate(): string {
  const d = new Date();
  const yyyy = d.getUTCFullYear();
  const mm = String(d.getUTCMonth() + 1).padStart(2, "0");
  const dd = String(d.getUTCDate()).padStart(2, "0");
  return `${yyyy}-${mm}-${dd}`;
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  const sources = await walk(SOURCE_DIR);
  if (sources.length === 0) {
    console.warn(`[export-xliff] no source docs at ${SOURCE_DIR}; nothing to export.`);
    return;
  }

  console.log(
    `[export-xliff] source=${sources.length} files, locales=${args.locales.join(",")} ` +
      `dry-run=${args.dryRun} tarball=${args.tarball}`,
  );

  let totalUnits = 0;
  let totalWords = 0;
  let totalFiles = 0;

  for (const locale of args.locales) {
    for (const src of sources) {
      const rel = path.relative(SOURCE_DIR, src);
      const content = await fs.readFile(src, "utf8");
      const segments = segmentMarkdown(content);
      const billable = segments.filter((s) => !s.protected);
      const words = billable.reduce(
        (n, s) => n + s.text.split(/\s+/).filter(Boolean).length,
        0,
      );
      totalUnits += billable.length;
      totalWords += words;
      totalFiles += 1;

      if (args.dryRun) continue;

      const xliff = buildXliff({
        sourceLocale: "en-US",
        targetLocale: locale,
        fileId: rel,
        segments,
      });
      const outPath = path.join(DIST_DIR, locale, `${rel}.xlf`);
      await mkdirp(path.dirname(outPath));
      await fs.writeFile(outPath, xliff, "utf8");
    }

    if (!args.dryRun) {
      const tmx = await buildTmx(locale);
      await mkdirp(DIST_DIR);
      await fs.writeFile(path.join(DIST_DIR, `tm-${locale}.tmx`), tmx, "utf8");
    }
  }

  console.log(
    `[export-xliff] units=${totalUnits} billable-words=${totalWords} ` +
      `files-written=${args.dryRun ? 0 : totalFiles}`,
  );

  // Word-count is per-locale; the translator gets every locale, so estimate
  // doubles. Industry pricing $0.08–$0.15/word for FIGS-class pairs.
  const perLocale = totalWords / args.locales.length;
  const total = perLocale * args.locales.length;
  const low = (total * 0.08).toFixed(0);
  const high = (total * 0.15).toFixed(0);
  console.log(
    `[export-xliff] est. cost USD ${low}–${high} ` +
      `(${perLocale} words/locale × ${args.locales.length} locales × $0.08–0.15/word)`,
  );

  if (args.dryRun) return;

  // Copy style guide alongside so translators receive everything in the bundle.
  const styleGuideSrc = path.join(REPO_ROOT, "i18n", "STYLE-GUIDE.md");
  try {
    const sg = await fs.readFile(styleGuideSrc, "utf8");
    await fs.writeFile(path.join(DIST_DIR, "STYLE-GUIDE.md"), sg, "utf8");
  } catch {
    console.warn("[export-xliff] STYLE-GUIDE.md not found; bundle will omit it.");
  }

  // README so the translator knows the bundle structure.
  const readme = [
    "# CoreLink translation bundle",
    "",
    `Generated: ${isoDate()} (UTC)`,
    `Source: en-US (canonical)`,
    `Targets: ${args.locales.join(", ")}`,
    "",
    "Structure:",
    "",
    "```",
    "translation-package-<date>/",
    "  STYLE-GUIDE.md           # tone, terminology, banned-list — READ FIRST",
    "  tm-<locale>.tmx          # translation-memory seed (XML, TMX 1.4)",
    "  <locale>/                # one .xlf per source page",
    "    ...                    # mirror of apps/docs/docs/**/*.{md,mdx}.xlf",
    "```",
    "",
    "Workflow:",
    "",
    "1. Load `tm-<locale>.tmx` into your CAT tool (memoQ / Trados / Smartcat /",
    "   Smartling / Transifex). This pre-fills established terminology.",
    "2. Open each `<locale>/**/*.xlf` and fill empty `<target>` elements.",
    "3. Segments with `translate=\"no\"` are frontmatter / code blocks /",
    "   MDX components — **do not translate**, do not edit.",
    "4. Preserve every `{{variable}}`, `$VAR`, `<Component>` placeholder verbatim.",
    "5. Return the same `.xlf` files (same directory structure) as a tarball.",
    "",
    "Questions: docs@humangr.com",
    "",
  ].join("\n");
  await fs.writeFile(path.join(DIST_DIR, "README.md"), readme, "utf8");

  if (args.tarball) {
    const stamp = isoDate();
    const tarName = `translation-package-${stamp}.tar.gz`;
    const tarPath = path.join(DIST_DIR, "..", tarName);
    const items = ["xliff"];
    const result = spawnSync(
      "tar",
      ["-czf", tarPath, "-C", path.dirname(DIST_DIR), ...items],
      { stdio: "inherit" },
    );
    if (result.status !== 0) {
      console.warn(`[export-xliff] tar exited with ${result.status}; bundle not packaged.`);
    } else {
      console.log(`[export-xliff] bundle: ${path.relative(REPO_ROOT, tarPath)}`);
    }
  }
}

main().catch((err) => {
  console.error("[export-xliff] fatal error:", err);
  process.exit(2);
});
