/**
 * i18n coverage *reporting* script — R-4 / WT-R4-1 deliverable (Goal A).
 *
 * Distinct from `i18n-coverage.ts` (which is a binary pass/fail CI gate
 * counting "file exists OR carries an i18n:TODO marker" as covered): this
 * report distinguishes three states per page and emits a Markdown table
 * suitable for stakeholders.
 *
 *   1. translated   — file exists, no `i18n:TODO` and no `i18n:MT` marker.
 *   2. mt-stub      — file exists, carries `<!-- i18n:MT` marker (machine-
 *                     translation seed; needs human post-edit before GA).
 *   3. todo         — file exists, carries `<!-- i18n:TODO` marker
 *                     (raw English placeholder; lowest quality).
 *   4. missing      — no counterpart file at all.
 *
 * Coverage % per locale = (translated + mt-stub) / total source files. Per
 * R-4 acceptance: ≥ 80 % mt-stub-or-better for both pt-BR and es-419.
 *
 * Namespaces are derived from the first path component under `docs/` so
 * the report fits in one table (tutorial, how-to, reference, explanation,
 * pricing, root). Per the spec, source-of-truth locales are pt-BR and
 * es-419 (NOT generic `pt`/`es`).
 *
 * Usage:
 *   pnpm tsx scripts/i18n-coverage-report.ts            # write COVERAGE.md
 *   pnpm tsx scripts/i18n-coverage-report.ts --stdout   # print only
 */

import { promises as fs } from "node:fs";
import path from "node:path";
import process from "node:process";

const LOCALES = ["pt-BR", "es-419"] as const;
type Locale = (typeof LOCALES)[number];

const REPO_ROOT = path.resolve(new URL(".", import.meta.url).pathname, "..");
const SOURCE_DIR = path.join(REPO_ROOT, "docs");
const I18N_BASE = path.join(REPO_ROOT, "i18n");
const OUTPUT = path.join(I18N_BASE, "COVERAGE.md");

type Status = "translated" | "mt-stub" | "todo" | "missing";

interface PageRecord {
  rel: string;
  namespace: string;
  status: Status;
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

function namespaceOf(rel: string): string {
  const head = rel.split(path.sep)[0];
  if (!head || head.endsWith(".mdx") || head.endsWith(".md")) return "root";
  return head;
}

async function classify(targetPath: string): Promise<Status> {
  let content: string;
  try {
    content = await fs.readFile(targetPath, "utf8");
  } catch {
    return "missing";
  }
  if (/<!--\s*i18n:MT\b/i.test(content)) return "mt-stub";
  if (/<!--\s*i18n:TODO\b/i.test(content)) return "todo";
  return "translated";
}

async function reportLocale(
  locale: Locale,
  sourceFiles: string[],
): Promise<PageRecord[]> {
  const localeRoot = path.join(
    I18N_BASE,
    locale,
    "docusaurus-plugin-content-docs",
    "current",
  );
  const out: PageRecord[] = [];
  for (const src of sourceFiles) {
    const rel = path.relative(SOURCE_DIR, src);
    const target = path.join(localeRoot, rel);
    const status = await classify(target);
    out.push({ rel, namespace: namespaceOf(rel), status });
  }
  return out;
}

function tableByNamespace(records: PageRecord[]): string {
  const namespaces = Array.from(new Set(records.map((r) => r.namespace))).sort();
  const rows: string[] = [];
  rows.push(
    "| Namespace | Total | Translated | MT-stub | TODO | Missing | Coverage (translated+MT) |",
  );
  rows.push(
    "| --- | ---: | ---: | ---: | ---: | ---: | ---: |",
  );
  for (const ns of namespaces) {
    const slice = records.filter((r) => r.namespace === ns);
    const total = slice.length;
    const translated = slice.filter((r) => r.status === "translated").length;
    const mt = slice.filter((r) => r.status === "mt-stub").length;
    const todo = slice.filter((r) => r.status === "todo").length;
    const missing = slice.filter((r) => r.status === "missing").length;
    const pct = total === 0 ? 0 : ((translated + mt) / total) * 100;
    rows.push(
      `| ${ns} | ${total} | ${translated} | ${mt} | ${todo} | ${missing} | ${pct.toFixed(1)}% |`,
    );
  }
  const total = records.length;
  const translated = records.filter((r) => r.status === "translated").length;
  const mt = records.filter((r) => r.status === "mt-stub").length;
  const todo = records.filter((r) => r.status === "todo").length;
  const missing = records.filter((r) => r.status === "missing").length;
  const pct = total === 0 ? 0 : ((translated + mt) / total) * 100;
  rows.push(
    `| **TOTAL** | **${total}** | **${translated}** | **${mt}** | **${todo}** | **${missing}** | **${pct.toFixed(1)}%** |`,
  );
  return rows.join("\n");
}

async function main(): Promise<void> {
  const stdoutOnly = process.argv.includes("--stdout");
  const sourceFiles = (await walk(SOURCE_DIR)).sort();
  const today = new Date().toISOString().slice(0, 10);

  const sections: string[] = [];
  sections.push(`# CoreLink docs i18n coverage`);
  sections.push("");
  sections.push(
    "Generated by `pnpm tsx scripts/i18n-coverage-report.ts`. Source: " +
      "`apps/docs/docs/**/*.{md,mdx}` vs the per-locale shadow trees under " +
      "`apps/docs/i18n/<locale>/docusaurus-plugin-content-docs/current/**`.",
  );
  sections.push("");
  sections.push(`- Generated: ${today}`);
  sections.push(`- Source locale: en-US (canonical, source of truth)`);
  sections.push(`- Target locales: pt-BR (LGPD market), es-419 (LATAM market)`);
  sections.push(
    "- Per spec contract S-18 §5.6 R-S18-12, three locales are canonical. " +
      "Per WT-R4-1 acceptance, ≥ 80 % mt-stub-or-better coverage is required " +
      "before native-speaker review can be scheduled.",
  );
  sections.push("");
  sections.push("## Legend");
  sections.push("");
  sections.push(
    "- **translated**: native-speaker translation, no `i18n:` marker.\n" +
      "- **MT-stub**: machine-translation seed (carries `<!-- i18n:MT` marker, " +
      "prefixed with `MT:` in the source-of-truth notice). Needs human post-edit " +
      "before GA.\n" +
      "- **TODO**: raw English placeholder (carries `<!-- i18n:TODO` marker). " +
      "Lowest quality — translator has not started.\n" +
      "- **missing**: no counterpart file. Build-fail per `i18n-coverage.ts`.",
  );

  const uiState: Record<string, "translated" | "mt-stub" | "todo" | "missing"> = {};
  for (const locale of LOCALES) {
    const records = await reportLocale(locale, sourceFiles);
    sections.push("");
    sections.push(`## ${locale}`);
    sections.push("");
    sections.push(tableByNamespace(records));

    // UI strings (code.json + navbar.json + footer.json) — separate, since
    // they live outside the docusaurus-plugin-content-docs shadow tree.
    const uiFiles = [
      `i18n/${locale}/code.json`,
      `i18n/${locale}/docusaurus-theme-classic/navbar.json`,
      `i18n/${locale}/docusaurus-theme-classic/footer.json`,
    ];
    const uiStatus: string[] = [];
    for (const rel of uiFiles) {
      const full = path.join(REPO_ROOT, rel);
      try {
        const txt = await fs.readFile(full, "utf8");
        const en = await fs.readFile(
          path.join(REPO_ROOT, rel.replace(`/${locale}/`, "/en-US/")),
          "utf8",
        );
        const same = txt.trim() === en.trim();
        uiStatus.push(`- \`${rel}\`: ${same ? "TODO (identical to EN)" : "translated"}`);
      } catch {
        uiStatus.push(`- \`${rel}\`: missing`);
      }
    }
    sections.push("");
    sections.push("### UI strings (theme + code.json)");
    sections.push("");
    sections.push(uiStatus.join("\n"));
    void uiState;
  }

  sections.push("");
  sections.push("## How to refresh");
  sections.push("");
  sections.push(
    "```bash\n" +
      "# 1. seed MT-stubs from translation memory (where TMX entries exist)\n" +
      "pnpm tsx scripts/mt-stub-seed.ts\n\n" +
      "# 2. regenerate this report\n" +
      "pnpm tsx scripts/i18n-coverage-report.ts\n\n" +
      "# 3. export XLIFF 2.1 for the translator vendor\n" +
      "pnpm i18n:export\n" +
      "```",
  );
  sections.push("");
  sections.push("## Acceptance");
  sections.push("");
  sections.push(
    "Per WT-R4-1 acceptance, coverage = `(translated + mt-stub) / total ≥ 80 %` " +
      "for both pt-BR and es-419. CI gate `i18n-coverage.ts` enforces the same " +
      "threshold at PR time (any new content without a stub fails the build).",
  );

  const md = sections.join("\n") + "\n";
  if (stdoutOnly) {
    process.stdout.write(md);
  } else {
    await fs.writeFile(OUTPUT, md);
    console.log(`[i18n-coverage-report] wrote ${path.relative(REPO_ROOT, OUTPUT)}`);
  }
}

main().catch((err) => {
  console.error("[i18n-coverage-report] fatal:", err);
  process.exit(2);
});
