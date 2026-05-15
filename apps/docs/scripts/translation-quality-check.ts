/**
 * Translation quality gate — R-4 / H-16 acceleration deliverable.
 *
 * Runs **after** translator delivery (post-`import-xliff.ts`) to verify:
 *
 *   1. All 200 stubs have non-stub translations (no `<!-- i18n:TODO -->`).
 *   2. No banned terminology slipped in (translated brand/product names,
 *      banned Castilian constructions, banned Brazilian-bureaucratic forms —
 *      see `apps/docs/i18n/STYLE-GUIDE.md`).
 *   3. MDX structure preserved (tag-balance check on `<Tabs>`, `<Admonition>`,
 *      `<TabItem>`, custom CoreLink components).
 *   4. Code blocks untouched (compare against EN source — block count and
 *      verbatim equality).
 *   5. Link integrity: every `[text](url)` URL identical to source.
 *
 * Usage:
 *   pnpm tsx scripts/translation-quality-check.ts                # all locales
 *   pnpm tsx scripts/translation-quality-check.ts --locale pt-BR
 *   pnpm tsx scripts/translation-quality-check.ts --against=current  # baseline
 *   pnpm tsx scripts/translation-quality-check.ts --strict       # exit-nonzero
 *
 * Output: `apps/docs/dist/translation-quality-report.md`.
 *
 * Exits 0 when run without --strict (report only). Use --strict in CI.
 */

import { promises as fs } from "node:fs";
import path from "node:path";
import process from "node:process";

const LOCALES = ["pt-BR", "es-419"] as const;
type Locale = (typeof LOCALES)[number];

const REPO_ROOT = path.resolve(new URL(".", import.meta.url).pathname, "..");
const SOURCE_DIR = path.join(REPO_ROOT, "docs");
const I18N_BASE = path.join(REPO_ROOT, "i18n");
const DIST_DIR = path.join(REPO_ROOT, "dist");

/**
 * Banned terms that must remain in English. If they appear translated in
 * <target>, flag a terminology violation. Keep aligned with STYLE-GUIDE.md §3.
 */
const PROTECTED_TERMS = [
  "CoreLink",
  "HuGR",
  "HumanGR",
  "Forge",
  "Stripe",
  "Cloudflare",
  "GitHub",
  "Linear",
  "PAT",
  "BYOK",
  "CAS",
  "REAPI",
  "BLAKE3",
  "SHA-256",
  "Ed25519",
  "OAuth",
  "OIDC",
  "SAML",
  "JWT",
  "LGPD",
  "GDPR",
  "SOC 2",
  "ISO 27001",
  "DSR",
  "DPA",
  "mTLS",
  "TLS",
  "TPM",
  "HSM",
  "KMS",
  "Bazel",
  "Buck2",
  "WASM",
  "WASI",
  "Workers",
  "Wrangler",
  "Durable Objects",
];

/**
 * Locale-specific banned constructions (machine-translation telltales,
 * formal-archaic forms, Castilian-only forms in es-419, etc.).
 */
const BANNED_BY_LOCALE: Record<Locale, RegExp[]> = {
  "pt-BR": [
    /\bvossa senhoria\b/i,
    /\bvós\b/i,
    /\bvosso\b/i,
    /\bidéia\b/, // pre-2009 orthography
    /\bfreqüente\b/, // pre-2009 orthography
  ],
  "es-419": [
    /\bvosotros\b/i,
    /\bvuestro\b/i,
    /\bordenador\b/i, // Castilian — prefer computadora
    /\bmóvil\b(?!\s*device)/i, // Castilian for phone
    /\bno obstante\b/i,
  ],
};

interface Args {
  locales: Locale[];
  strict: boolean;
  against: string;
}

function parseArgs(argv: string[]): Args {
  let locales: Locale[] = [...LOCALES];
  let strict = false;
  let against = "current";
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--strict") strict = true;
    else if (arg === "--locale" && argv[i + 1]) {
      const v = argv[i + 1] as Locale;
      if (!LOCALES.includes(v)) throw new Error(`unknown --locale ${v}`);
      locales = [v];
      i += 1;
    } else if (arg.startsWith("--against=")) {
      against = arg.split("=", 2)[1] ?? "current";
    }
  }
  return { locales, strict, against };
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

interface Finding {
  rel: string;
  rule: string;
  severity: "error" | "warning" | "info";
  detail: string;
}

interface FileMetrics {
  /** Code-fence block count. */
  codeBlocks: number;
  /** Inline code spans. */
  inlineCode: number;
  /** Markdown links `[text](url)`. */
  links: { text: string; url: string }[];
  /** Opening MDX/HTML-style tag names. */
  openTags: string[];
  /** Closing tag names. */
  closeTags: string[];
  /** Self-closing tag names. */
  selfClosingTags: string[];
  /** Code-fence block bodies (joined). */
  codeBodies: string;
  /** Whether file contains the i18n:TODO stub marker. */
  hasTodo: boolean;
}

function analyze(content: string): FileMetrics {
  const m: FileMetrics = {
    codeBlocks: 0,
    inlineCode: 0,
    links: [],
    openTags: [],
    closeTags: [],
    selfClosingTags: [],
    codeBodies: "",
    hasTodo: /<!--\s*i18n:TODO/i.test(content),
  };
  const lines = content.split("\n");
  let inCode = false;
  let fence = "";
  const codeAcc: string[] = [];
  for (const line of lines) {
    const trimmed = line.trim();
    if (!inCode && (trimmed.startsWith("```") || trimmed.startsWith("~~~"))) {
      inCode = true;
      fence = trimmed.slice(0, 3);
      m.codeBlocks += 1;
      continue;
    }
    if (inCode && trimmed.startsWith(fence)) {
      inCode = false;
      continue;
    }
    if (inCode) {
      codeAcc.push(line);
      continue;
    }
    // Inline code count.
    const inline = line.match(/`[^`\n]+`/g);
    if (inline) m.inlineCode += inline.length;
    // Links (skip image syntax to keep simple).
    const linkRe = /\[([^\]]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g;
    let lm: RegExpExecArray | null;
    while ((lm = linkRe.exec(line)) !== null) {
      m.links.push({ text: lm[1], url: lm[2] });
    }
    // Tags. Cheap regex; good enough for balance.
    const tagRe = /<\/?([A-Za-z][\w.-]*)\b[^>]*?(\/?)>/g;
    let tm: RegExpExecArray | null;
    while ((tm = tagRe.exec(line)) !== null) {
      const full = tm[0];
      const name = tm[1];
      const selfClose = tm[2] === "/";
      if (selfClose) m.selfClosingTags.push(name);
      else if (full.startsWith("</")) m.closeTags.push(name);
      else m.openTags.push(name);
    }
  }
  m.codeBodies = codeAcc.join("\n");
  return m;
}

async function checkLocaleFile(
  locale: Locale,
  rel: string,
  enPath: string,
  tgtPath: string,
): Promise<Finding[]> {
  const findings: Finding[] = [];
  let exists = true;
  let tgtContent = "";
  try {
    tgtContent = await fs.readFile(tgtPath, "utf8");
  } catch {
    exists = false;
  }
  if (!exists) {
    findings.push({
      rel,
      rule: "MISSING_FILE",
      severity: "error",
      detail: `target file missing for locale=${locale}`,
    });
    return findings;
  }
  const enContent = await fs.readFile(enPath, "utf8");
  const en = analyze(enContent);
  const tgt = analyze(tgtContent);

  // 1. Stub still present?
  if (tgt.hasTodo) {
    findings.push({
      rel,
      rule: "STUB_REMAINS",
      severity: "error",
      detail: "<!-- i18n:TODO --> marker still present — translation not delivered",
    });
  }

  // 2. Code block parity.
  if (en.codeBlocks !== tgt.codeBlocks) {
    findings.push({
      rel,
      rule: "CODEBLOCK_COUNT_DRIFT",
      severity: "error",
      detail: `EN has ${en.codeBlocks} code blocks, target has ${tgt.codeBlocks}`,
    });
  }
  if (en.codeBodies !== tgt.codeBodies) {
    findings.push({
      rel,
      rule: "CODEBLOCK_CONTENT_DRIFT",
      severity: "error",
      detail: "code block contents differ from EN source (must be verbatim)",
    });
  }

  // 3. Link integrity — same URL set in same order.
  const enUrls = en.links.map((l) => l.url);
  const tgtUrls = tgt.links.map((l) => l.url);
  if (enUrls.length !== tgtUrls.length) {
    findings.push({
      rel,
      rule: "LINK_COUNT_DRIFT",
      severity: "error",
      detail: `EN has ${enUrls.length} links, target has ${tgtUrls.length}`,
    });
  } else {
    for (let i = 0; i < enUrls.length; i += 1) {
      if (enUrls[i] !== tgtUrls[i]) {
        findings.push({
          rel,
          rule: "LINK_URL_CHANGED",
          severity: "error",
          detail: `link[${i}]: EN=${enUrls[i]} target=${tgtUrls[i]}`,
        });
      }
    }
  }

  // 4. Tag balance.
  const tgtOpen = sortedCount(tgt.openTags);
  const tgtClose = sortedCount(tgt.closeTags);
  for (const [name, count] of Object.entries(tgtOpen)) {
    const closeCount = tgtClose[name] ?? 0;
    if (count !== closeCount) {
      findings.push({
        rel,
        rule: "MDX_TAG_UNBALANCED",
        severity: "error",
        detail: `<${name}>: open=${count} close=${closeCount}`,
      });
    }
  }

  // 5. Protected-term integrity — every EN occurrence count must be ≥ target.
  // (Translators sometimes drop tokens; sometimes they translate brand names.)
  for (const term of PROTECTED_TERMS) {
    const enCount = countOccurrences(enContent, term);
    if (enCount === 0) continue;
    const tgtCount = countOccurrences(tgtContent, term);
    if (tgtCount < enCount) {
      findings.push({
        rel,
        rule: "PROTECTED_TERM_DROPPED",
        severity: "error",
        detail: `"${term}": EN=${enCount} target=${tgtCount} (must match)`,
      });
    }
  }

  // 6. Banned constructions per locale.
  for (const pattern of BANNED_BY_LOCALE[locale]) {
    if (pattern.test(tgtContent)) {
      findings.push({
        rel,
        rule: "BANNED_CONSTRUCTION",
        severity: "warning",
        detail: `matches ${pattern} — see STYLE-GUIDE.md §7`,
      });
    }
  }

  return findings;
}

function sortedCount(arr: string[]): Record<string, number> {
  const out: Record<string, number> = {};
  for (const s of arr) out[s] = (out[s] ?? 0) + 1;
  return out;
}

function countOccurrences(haystack: string, needle: string): number {
  if (needle.length === 0) return 0;
  let n = 0;
  let i = 0;
  while ((i = haystack.indexOf(needle, i)) !== -1) {
    n += 1;
    i += needle.length;
  }
  return n;
}

interface LocaleSummary {
  locale: Locale;
  totalFiles: number;
  filesWithTodo: number;
  filesTranslated: number;
  errors: number;
  warnings: number;
  findings: Finding[];
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  console.log(
    `[tqc] locales=${args.locales.join(",")} strict=${args.strict} against=${args.against}`,
  );
  const sources = await walk(SOURCE_DIR);
  const summaries: LocaleSummary[] = [];
  for (const locale of args.locales) {
    const localeRoot = path.join(
      I18N_BASE,
      locale,
      "docusaurus-plugin-content-docs",
      "current",
    );
    const findings: Finding[] = [];
    let filesWithTodo = 0;
    let filesTranslated = 0;
    for (const src of sources) {
      const rel = path.relative(SOURCE_DIR, src);
      const tgt = path.join(localeRoot, rel);
      const fileFindings = await checkLocaleFile(locale, rel, src, tgt);
      findings.push(...fileFindings);
      const stubFinding = fileFindings.find((f) => f.rule === "STUB_REMAINS");
      const missing = fileFindings.find((f) => f.rule === "MISSING_FILE");
      if (stubFinding) filesWithTodo += 1;
      else if (!missing) filesTranslated += 1;
    }
    const errors = findings.filter((f) => f.severity === "error").length;
    const warnings = findings.filter((f) => f.severity === "warning").length;
    summaries.push({
      locale,
      totalFiles: sources.length,
      filesWithTodo,
      filesTranslated,
      errors,
      warnings,
      findings,
    });
  }

  // Write report.
  await fs.mkdir(DIST_DIR, { recursive: true });
  const reportPath = path.join(DIST_DIR, "translation-quality-report.md");
  const date = new Date().toISOString();
  const lines: string[] = [
    "# Translation quality report",
    "",
    `Generated: ${date}`,
    `Source corpus: ${sources.length} EN files under \`apps/docs/docs/\`.`,
    `Baseline: \`${args.against}\`.`,
    "",
    "## Summary",
    "",
    "| Locale | Translated | Stubs remaining | Errors | Warnings |",
    "|---|---|---|---|---|",
  ];
  for (const s of summaries) {
    lines.push(
      `| ${s.locale} | ${s.filesTranslated}/${s.totalFiles} | ${s.filesWithTodo} | ${s.errors} | ${s.warnings} |`,
    );
  }
  lines.push("", "## Findings", "");
  for (const s of summaries) {
    lines.push(`### ${s.locale}`, "");
    if (s.findings.length === 0) {
      lines.push("_No findings — locale is clean._", "");
      continue;
    }
    const byRule = new Map<string, Finding[]>();
    for (const f of s.findings) {
      const arr = byRule.get(f.rule) ?? [];
      arr.push(f);
      byRule.set(f.rule, arr);
    }
    for (const [rule, fs2] of byRule) {
      lines.push(`#### ${rule} (${fs2.length})`, "");
      for (const f of fs2.slice(0, 25)) {
        lines.push(`- \`${f.rel}\` — ${f.detail}`);
      }
      if (fs2.length > 25) lines.push(`- ... and ${fs2.length - 25} more`);
      lines.push("");
    }
  }
  await fs.writeFile(reportPath, lines.join("\n"), "utf8");

  // Console summary.
  let totalErrors = 0;
  for (const s of summaries) {
    totalErrors += s.errors;
    console.log(
      `[tqc] locale=${s.locale} translated=${s.filesTranslated}/${s.totalFiles} ` +
        `stubs=${s.filesWithTodo} errors=${s.errors} warnings=${s.warnings}`,
    );
  }
  console.log(`[tqc] report: ${path.relative(REPO_ROOT, reportPath)}`);

  if (args.strict && totalErrors > 0) {
    console.error(`[tqc] FAIL — ${totalErrors} errors in strict mode.`);
    process.exit(1);
  }
}

main().catch((err) => {
  console.error("[tqc] fatal error:", err);
  process.exit(2);
});
