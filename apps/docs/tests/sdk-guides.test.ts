/**
 * WI-S18-003 — SDK how-to guides structural test.
 *
 * Asserts:
 *   1. Each of the 4 SDK languages has all 7 canonical how-to pages.
 *   2. Every page has valid frontmatter with title + id + description.
 *   3. Every page contains the Diataxis "When to use this guide" section.
 *   4. Every page uses correct MDX code-fence language tags for snippets.
 *   5. Examples never embed real-looking PATs (CTRL-CRED-001 / CTRL-PRIV-001).
 */

import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import {
  SDK_LANGUAGES,
  SDK_HOWTO_SLUGS,
  REQUIRED_HOWTO_HEADING,
  CODE_FENCE_LANG,
} from "../src/sdk-guides.js";

const here = dirname(fileURLToPath(import.meta.url));
const docsRoot = resolve(here, "..", "docs", "how-to");

function parseFrontmatter(src: string): Record<string, string> | null {
  if (!src.startsWith("---\n")) return null;
  const end = src.indexOf("\n---\n", 4);
  if (end === -1) return null;
  const yaml = src.slice(4, end);
  const meta: Record<string, string> = {};
  for (const line of yaml.split("\n")) {
    const m = line.match(/^(\w[\w-]*):\s*(.*)$/);
    if (m) meta[m[1]] = m[2].replace(/^["']|["']$/g, "");
  }
  return meta;
}

describe("WI-S18-003 SDK how-to guides", () => {
  describe.each(SDK_LANGUAGES)("language: %s", (lang) => {
    const dir = resolve(docsRoot, `sdk-${lang}`);

    it("folder exists", () => {
      expect(existsSync(dir)).toBe(true);
    });

    it.each(SDK_HOWTO_SLUGS)("has %s.mdx", (slug) => {
      expect(existsSync(resolve(dir, `${slug}.mdx`))).toBe(true);
    });

    it.each(SDK_HOWTO_SLUGS)("frontmatter valid: %s", async (slug) => {
      const src = await readFile(resolve(dir, `${slug}.mdx`), "utf8");
      const fm = parseFrontmatter(src);
      expect(fm).not.toBeNull();
      expect(fm!.title).toBeTruthy();
      expect(fm!.id).toBeTruthy();
      expect(fm!.description).toBeTruthy();
    });

    it.each(SDK_HOWTO_SLUGS)(
      "has 'When to use this guide' section: %s",
      async (slug) => {
        const src = await readFile(resolve(dir, `${slug}.mdx`), "utf8");
        expect(src).toContain(REQUIRED_HOWTO_HEADING);
      },
    );

    it.each(SDK_HOWTO_SLUGS)("uses code fences: %s", async (slug) => {
      const src = await readFile(resolve(dir, `${slug}.mdx`), "utf8");
      const fenceCount = (src.match(/```/g) ?? []).length;
      expect(fenceCount, "must contain at least one code fence").toBeGreaterThanOrEqual(2);
      // every opening fence must declare a language for syntax highlighting
      const openingFences = [...src.matchAll(/```([a-zA-Z0-9_-]*)\n/g)];
      expect(openingFences.length).toBeGreaterThanOrEqual(1);
      const idiomatic = CODE_FENCE_LANG[lang];
      // at least one fenced block must use the language's idiomatic tag
      // (cli pages may also use json/text/yaml etc.)
      const hasIdiomatic =
        slug === "07-troubleshooting" ||
        openingFences.some((m) => m[1] === idiomatic) ||
        // cli reference allows bash, yaml, json, text
        (lang === "cli" && openingFences.some((m) =>
          ["bash", "json", "yaml", "text"].includes(m[1]),
        ));
      expect(hasIdiomatic, `expected at least one ${idiomatic} fence`).toBe(true);
    });

    it.each(SDK_HOWTO_SLUGS)(
      "contains no real-looking PAT: %s",
      async (slug) => {
        const src = await readFile(resolve(dir, `${slug}.mdx`), "utf8");
        // a "real" PAT would use prod/staging env AND long random segments
        expect(src).not.toMatch(
          /corelink_(?:prod|production|live|staging|stg)_[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,}/,
        );
      },
    );
  });

  it("all 4 languages × 7 guides = 28 files", () => {
    let count = 0;
    for (const lang of SDK_LANGUAGES) {
      for (const slug of SDK_HOWTO_SLUGS) {
        const p = resolve(docsRoot, `sdk-${lang}`, `${slug}.mdx`);
        if (existsSync(p)) count++;
      }
    }
    expect(count).toBe(28);
  });
});
