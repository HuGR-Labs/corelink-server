import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// The footer copyright is defined ONCE, in docusaurus.config.ts, as a template
// literal that computes the year at build time. The per-locale
// docusaurus-theme-classic/footer.json files are overrides: a `copyright` key
// there wins over the config, and its value is rendered verbatim — Docusaurus
// passes no ICU values for that string, so a `{year}` placeholder in a
// translation file is emitted literally on every page of that locale, which is
// what shipped ("Copyright © {year} HuGR Labs.").
//
// Two failure modes are pinned here: re-introducing the override, and replacing
// the computed year with a hardcoded one (correct for at most twelve months).

const I18N_DIR = path.resolve(__dirname, "..", "i18n");
const CONFIG = path.resolve(__dirname, "..", "docusaurus.config.ts");

function localeDirs(): string[] {
  return fs
    .readdirSync(I18N_DIR, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => e.name);
}

describe("footer copyright", () => {
  it("computes the year in the config rather than hardcoding it", () => {
    const config = fs.readFileSync(CONFIG, "utf8");
    const copyrights = [
      ...config.matchAll(/copyright:\s*`([^`]*)`/g),
    ].map((m) => m[1]);
    expect(copyrights.length).toBeGreaterThan(0);
    for (const c of copyrights) {
      expect(c).toContain("${new Date().getFullYear()}");
      // A four-digit literal year would silently go stale on 1 January.
      expect(c).not.toMatch(/\b(19|20)\d{2}\b/);
    }
  });

  it("has no locale overriding copyright (the override cannot interpolate)", () => {
    for (const locale of localeDirs()) {
      const footer = path.join(
        I18N_DIR,
        locale,
        "docusaurus-theme-classic",
        "footer.json",
      );
      if (!fs.existsSync(footer)) continue;
      const keys = Object.keys(JSON.parse(fs.readFileSync(footer, "utf8")));
      expect(keys, `${locale} footer.json must not override copyright`).not.toContain(
        "copyright",
      );
    }
  });
});
