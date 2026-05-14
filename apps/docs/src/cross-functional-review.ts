/**
 * Cross-functional review metadata helpers.
 *
 * Centralises the set of trees that fall under the S-18 spec contract §10
 * anti-scope cross-functional review gate, the required frontmatter fields,
 * and the regex that detects the DRAFT banner that every gated page must
 * render at the top of the file.
 *
 * Consumed by `tests/cross-functional.test.ts` (mandatory CI gate) and by
 * tooling that pretty-prints the review checklist in
 * `apps/docs/CONTENT-REVIEW.md`.
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, posix, relative, sep } from "node:path";

export const DOCS_APP_ROOT = posix.normalize("apps/docs");

/**
 * Trees under `apps/docs/docs/` that require cross-functional sign-off before
 * the `draft: true` frontmatter flag can be removed.
 */
export const GATED_TREES = [
  "docs/explanation/compliance",
  "docs/explanation/security",
  "docs/pricing",
] as const;

export type GatedTree = (typeof GATED_TREES)[number];

/**
 * Required frontmatter keys on every page under a gated tree.
 *
 * `cross_functional_review` MUST equal the literal string `TBD` until
 * sign-off lands; the test enforces this via {@link CROSS_FUNCTIONAL_TBD}.
 */
export const REQUIRED_FRONTMATTER_KEYS = [
  "cross_functional_review",
  "pending_signoff",
  "draft",
] as const;

export const CROSS_FUNCTIONAL_TBD = "TBD";

/**
 * Regex that detects the DRAFT banner. Either the standalone Markdown
 * blockquote or the `<DraftBanner />` JSX element satisfies the check.
 */
export const DRAFT_BANNER_REGEX =
  /(<DraftBanner\b|DRAFT\s*[—-]\s*pending Legal\s*\+\s*Finance\s*\+\s*Security review)/i;

/**
 * Recursively list every `.mdx` (or `.md`) file under a directory.
 *
 * Returns absolute paths. Silently returns an empty array if the directory
 * does not exist — callers may then choose to fail loud.
 */
export function listMarkdownFiles(rootDir: string): readonly string[] {
  const acc: string[] = [];
  walk(rootDir, acc);
  return acc;
}

function walk(dir: string, acc: string[]): void {
  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return;
  }
  for (const entry of entries) {
    const full = join(dir, entry);
    const st = statSync(full);
    if (st.isDirectory()) {
      walk(full, acc);
    } else if (entry.endsWith(".mdx") || entry.endsWith(".md")) {
      acc.push(full);
    }
  }
}

export interface ParsedFrontmatter {
  readonly raw: string;
  readonly fields: ReadonlyMap<string, string>;
  readonly body: string;
}

/**
 * Minimal YAML frontmatter parser tailored to the small set of scalar fields
 * we enforce. Not a full YAML parser — handles `key: value` and list items
 * (`- "value"`) on subsequent lines.
 */
export function parseFrontmatter(content: string): ParsedFrontmatter | null {
  if (!content.startsWith("---\n")) {
    return null;
  }
  const end = content.indexOf("\n---\n", 4);
  if (end === -1) {
    return null;
  }
  const raw = content.slice(4, end);
  const body = content.slice(end + 5);
  const fields = new Map<string, string>();
  const lines = raw.split("\n");
  let currentKey: string | null = null;
  let currentList: string[] | null = null;
  for (const line of lines) {
    if (line.length === 0) {
      continue;
    }
    if (line.startsWith("  - ") || line.startsWith("- ")) {
      if (currentList !== null) {
        const item = line.replace(/^\s*-\s*/, "").trim();
        currentList.push(stripQuotes(item));
      }
      continue;
    }
    const match = /^([A-Za-z0-9_]+):\s*(.*)$/.exec(line);
    if (match === null) {
      continue;
    }
    const [, key, valueRaw] = match;
    if (key === undefined) {
      continue;
    }
    if (currentList !== null && currentKey !== null) {
      fields.set(currentKey, JSON.stringify(currentList));
      currentList = null;
    }
    const value = (valueRaw ?? "").trim();
    if (value === "") {
      currentKey = key;
      currentList = [];
      continue;
    }
    fields.set(key, stripQuotes(value));
    currentKey = key;
    currentList = null;
  }
  if (currentList !== null && currentKey !== null) {
    fields.set(currentKey, JSON.stringify(currentList));
  }
  return { raw, fields, body };
}

function stripQuotes(value: string): string {
  if (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    return value.slice(1, -1);
  }
  return value;
}

export function toRelativePosix(root: string, abs: string): string {
  return relative(root, abs).split(sep).join("/");
}

export function readUtf8(path: string): string {
  return readFileSync(path, "utf8");
}
