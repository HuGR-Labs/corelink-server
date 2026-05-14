/**
 * Cross-functional review gate test.
 *
 * Per S-18 spec contract §10 anti-scope and WI-S18-004 task #17, every MDX
 * (or MD) file under the compliance, security, and pricing trees MUST:
 *
 *   1. Carry `cross_functional_review: TBD` in its YAML frontmatter.
 *   2. Declare a non-empty `pending_signoff:` list.
 *   3. Set `draft: true` so downstream rendering can flag the page.
 *   4. Render the DRAFT — pending Legal + Finance + Security review banner
 *      (either as the `<DraftBanner />` JSX element or as the literal warning
 *      blockquote, matched by {@link DRAFT_BANNER_REGEX}).
 *
 * This test is the canonical CI gate for the S-18 anti-scope rule. Removing
 * any of these checks requires a waiver per the spec contract §19.
 */

import { describe, expect, test } from "vitest";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  CROSS_FUNCTIONAL_TBD,
  DRAFT_BANNER_REGEX,
  GATED_TREES,
  listMarkdownFiles,
  parseFrontmatter,
  readUtf8,
  toRelativePosix,
} from "../src/cross-functional-review";

const HERE = dirname(fileURLToPath(import.meta.url));
const DOCS_ROOT = resolve(HERE, "..");

interface GatedFile {
  readonly tree: string;
  readonly abs: string;
  readonly rel: string;
  readonly content: string;
}

function collectGatedFiles(): readonly GatedFile[] {
  const acc: GatedFile[] = [];
  for (const tree of GATED_TREES) {
    const abs = resolve(DOCS_ROOT, tree);
    const files = listMarkdownFiles(abs);
    for (const file of files) {
      acc.push({
        tree,
        abs: file,
        rel: toRelativePosix(DOCS_ROOT, file),
        content: readUtf8(file),
      });
    }
  }
  return acc;
}

const GATED_FILES = collectGatedFiles();

describe("cross-functional review gate", () => {
  test("each gated tree contains at least one MDX file", () => {
    for (const tree of GATED_TREES) {
      const filesInTree = GATED_FILES.filter((f) => f.tree === tree);
      expect(
        filesInTree.length,
        `expected at least one .mdx file under ${tree}`,
      ).toBeGreaterThan(0);
    }
  });

  test.each(GATED_FILES.map((f) => [f.rel, f]))(
    "%s has cross_functional_review: TBD frontmatter",
    (_rel, file) => {
      const parsed = parseFrontmatter(file.content);
      expect(parsed, `missing YAML frontmatter block in ${file.rel}`).not.toBeNull();
      const fields = parsed!.fields;
      expect(
        fields.get("cross_functional_review"),
        `${file.rel} must declare cross_functional_review: TBD`,
      ).toBe(CROSS_FUNCTIONAL_TBD);
    },
  );

  test.each(GATED_FILES.map((f) => [f.rel, f]))(
    "%s has draft: true frontmatter",
    (_rel, file) => {
      const parsed = parseFrontmatter(file.content);
      expect(parsed).not.toBeNull();
      expect(
        parsed!.fields.get("draft"),
        `${file.rel} must declare draft: true`,
      ).toBe("true");
    },
  );

  test.each(GATED_FILES.map((f) => [f.rel, f]))(
    "%s declares a non-empty pending_signoff list",
    (_rel, file) => {
      const parsed = parseFrontmatter(file.content);
      expect(parsed).not.toBeNull();
      const raw = parsed!.fields.get("pending_signoff");
      expect(raw, `${file.rel} must declare pending_signoff:`).toBeDefined();
      const parsedList = JSON.parse(raw!) as readonly string[];
      expect(
        parsedList.length,
        `${file.rel} pending_signoff list must not be empty`,
      ).toBeGreaterThan(0);
    },
  );

  test.each(GATED_FILES.map((f) => [f.rel, f]))(
    "%s renders the DRAFT banner",
    (_rel, file) => {
      expect(
        DRAFT_BANNER_REGEX.test(file.content),
        `${file.rel} must render the DRAFT banner (<DraftBanner /> or literal blockquote)`,
      ).toBe(true);
    },
  );

  test("pricing pages contain no specific dollar amounts (only $X placeholders)", () => {
    // Match any $ followed by a digit — that would indicate a specific price.
    const SPECIFIC_PRICE = /\$\d/;
    const pricingFiles = GATED_FILES.filter((f) => f.tree === "docs/pricing");
    expect(pricingFiles.length).toBeGreaterThan(0);
    for (const file of pricingFiles) {
      expect(
        SPECIFIC_PRICE.test(file.content),
        `${file.rel} contains a specific dollar amount; only $X placeholders are allowed before Finance sign-off`,
      ).toBe(false);
    }
  });
});
