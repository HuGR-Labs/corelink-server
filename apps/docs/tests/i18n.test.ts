import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import config from "../docusaurus.config";

const DOCS_ROOT = path.resolve(__dirname, "..");

describe("docusaurus.config.ts — i18n", () => {
  it("ships the four canonical locales en-US + pt-BR + es-419 + de", () => {
    // R-prep i18n-de expansion: German added for EU enterprise GA buyers
    // (DACH region) — see `apps/docs/docusaurus.config.ts` locales array
    // and `specs/_audits/2026-05-15-debt-register.md` row DEBT-007 / DEBT-015
    // wave-25 closure (`de` locale build green on Node 22.17.1).
    expect(config.i18n?.defaultLocale).toBe("en-US");
    expect(config.i18n?.locales).toEqual(["en-US", "pt-BR", "es-419", "de"]);
  });

  it("has on-disk translation directories for every declared locale", () => {
    const locales = config.i18n?.locales ?? [];
    for (const locale of locales) {
      if (!locale) continue;
      const localeDir = path.join(DOCS_ROOT, "i18n", locale);
      expect(
        fs.existsSync(localeDir),
        `expected i18n/${locale}/ to exist`,
      ).toBe(true);

      const codeJson = path.join(localeDir, "code.json");
      expect(
        fs.existsSync(codeJson),
        `expected i18n/${locale}/code.json to exist`,
      ).toBe(true);

      const navbarJson = path.join(
        localeDir,
        "docusaurus-theme-classic",
        "navbar.json",
      );
      expect(
        fs.existsSync(navbarJson),
        `expected navbar.json for ${locale}`,
      ).toBe(true);

      const footerJson = path.join(
        localeDir,
        "docusaurus-theme-classic",
        "footer.json",
      );
      expect(
        fs.existsSync(footerJson),
        `expected footer.json for ${locale}`,
      ).toBe(true);
    }
  });

  it("attaches an htmlLang to every locale (a11y / SEO)", () => {
    const cfg: Record<string, { htmlLang?: string } | undefined> =
      (config.i18n?.localeConfigs ?? {}) as Record<
        string,
        { htmlLang?: string } | undefined
      >;
    const locales = config.i18n?.locales ?? [];
    for (const locale of locales) {
      if (!locale) continue;
      const entry = cfg[locale];
      expect(entry, `localeConfigs[${locale}] missing`).toBeDefined();
      expect(entry?.htmlLang).toBeTruthy();
    }
  });
});
