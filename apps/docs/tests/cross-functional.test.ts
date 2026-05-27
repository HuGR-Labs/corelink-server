/**
 * Cross-functional review gate test.
 *
 * Per S-18 spec contract §10 anti-scope and WI-S18-004 task #17, every MDX
 * (or MD) file under the compliance, security, and pricing trees MUST:
 *
 *   1. Carry `cross_functional_review: TBD` OR `cross_functional_review: pending`
 *      in its YAML frontmatter. Both literals signal "not yet signed off" and
 *      are accepted per the wave-22 DEBT-015 build-closure trade-off (see
 *      {@link CROSS_FUNCTIONAL_PENDING_VALUES} in `cross-functional-review.ts`).
 *   2. Declare a non-empty `pending_signoff:` list.
 *   3. Render the DRAFT — pending Legal + Finance + Security review banner
 *      (either as the `<DraftBanner />` JSX element or as the literal warning
 *      blockquote, matched by {@link DRAFT_BANNER_REGEX}). This is the
 *      user-visible "not yet signed off" signal — required even when
 *      `draft: true` is omitted to keep the page in the production build.
 *
 * Historical note: the original WI-S18-004 contract also required
 * `draft: true`, but wave-22 DEBT-015 build closure removed it from
 * `audit-chain.mdx` / `byok.mdx` / `lgpd-brazil.mdx` because Docusaurus
 * excludes draft pages from the build, breaking 90+ MDX cross-links. The
 * `<DraftBanner />` JSX element fully subsumes the user-visible signal that
 * `draft: true` was meant to carry, so the gate test no longer asserts the
 * frontmatter flag. See `specs/_audits/sealed/2026-05-16-prexisting-test-failures-triage.md`
 * for the full reconciliation.
 *
 * This test is the canonical CI gate for the S-18 anti-scope rule. Removing
 * any of these checks requires a waiver per the spec contract §19.
 */

import { describe, expect, test } from "vitest";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  CROSS_FUNCTIONAL_PENDING_VALUES,
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
    "%s has cross_functional_review: TBD or pending frontmatter",
    (_rel, file) => {
      const parsed = parseFrontmatter(file.content);
      expect(parsed, `missing YAML frontmatter block in ${file.rel}`).not.toBeNull();
      const fields = parsed!.fields;
      const reviewValue = fields.get("cross_functional_review");
      expect(
        reviewValue !== undefined &&
          CROSS_FUNCTIONAL_PENDING_VALUES.includes(reviewValue),
        `${file.rel} must declare cross_functional_review: one of ${JSON.stringify(
          CROSS_FUNCTIONAL_PENDING_VALUES,
        )} (got ${JSON.stringify(reviewValue)})`,
      ).toBe(true);
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
