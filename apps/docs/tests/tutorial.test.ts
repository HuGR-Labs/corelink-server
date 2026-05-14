/**
 * WI-S18-002 — Tutorial page structural test.
 *
 * Verifies the 6 getting-started tutorials all have:
 *   - Valid frontmatter (id, title, sidebar_label)
 *   - A single H1 heading
 *   - No real PAT leak (only the placeholder corelink_dev_t_xxx.xxx.xxx is allowed)
 */
import matter from "gray-matter";
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const TUTORIAL_DIR = resolve(__dirname, "..", "docs", "tutorial");
const EXPECTED = [
  "01-installation.mdx",
  "02-first-pat.mdx",
  "03-bazel-quickstart.mdx",
  "04-buck2-quickstart.mdx",
  "05-native-ffi-quickstart.mdx",
  "06-verify-cache-hit.mdx",
];

describe("tutorial pages", () => {
  it("contains all 6 expected pages", () => {
    const present = readdirSync(TUTORIAL_DIR);
    for (const f of EXPECTED) {
      expect(present, `missing ${f}`).toContain(f);
    }
  });

  for (const file of EXPECTED) {
    describe(file, () => {
      const raw = readFileSync(join(TUTORIAL_DIR, file), "utf8");
      const { data, content } = matter(raw);

      it("has frontmatter id, title, sidebar_label", () => {
        expect(data.id, "id").toBeTruthy();
        expect(data.title, "title").toBeTruthy();
        expect(data.sidebar_label, "sidebar_label").toBeTruthy();
      });

      it("has exactly one H1 heading (outside fenced code blocks)", () => {
        // Strip ``` fenced blocks so shell comments like `# 1. step` do not
        // count as headings.
        let inFence = false;
        const lines = content.split("\n").filter((l) => {
          if (/^```/.test(l)) {
            inFence = !inFence;
            return false;
          }
          return !inFence;
        });
        const h1s = lines.filter((l) => /^# [^#]/.test(l));
        expect(h1s.length, `unique H1 expected, got ${h1s.length}`).toBe(1);
      });

      it("does not leak a real PAT (CTRL-CRED-001)", () => {
        // Allowed placeholder; anything else with corelink_<env>_ prefix that
        // is NOT the placeholder is a violation.
        const matches = content.match(/corelink_(dev|staging|prod)_[a-z0-9_]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g) ?? [];
        for (const m of matches) {
          expect(m, `unexpected PAT-shaped string: ${m}`).toBe("corelink_dev_t_xxx.xxx.xxx");
        }
      });
    });
  }
});
