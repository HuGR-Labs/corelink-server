import { describe, expect, it } from "vitest";
import config from "../docusaurus.config";

interface NavbarItem {
  to?: string;
  href?: string;
  label?: string;
  type?: string;
}

interface NavbarConfig {
  items: NavbarItem[];
}

interface ThemeConfigShape {
  navbar?: NavbarConfig;
  footer?: { links: Array<{ title?: string; items: NavbarItem[] }> };
}

const REQUIRED_NAV_LABELS = [
  "Tutorial",
  "How-to",
  "Reference",
  "Explanation",
  "API",
  "Pricing",
  "Security",
];

const REQUIRED_FOOTER_LABELS = ["Privacy", "Terms", "Sub-processors"];

describe("themeConfig.navbar — required labels", () => {
  it("includes Tutorial, How-to, Reference, Explanation, API, Pricing, Security", () => {
    const theme = config.themeConfig as ThemeConfigShape;
    const labels = (theme.navbar?.items ?? []).map((i) => i.label);
    for (const required of REQUIRED_NAV_LABELS) {
      expect(labels, `missing navbar label: ${required}`).toContain(required);
    }
  });

  it("offers a locale dropdown so users can switch language", () => {
    const theme = config.themeConfig as ThemeConfigShape;
    const types = (theme.navbar?.items ?? []).map((i) => i.type);
    expect(types).toContain("localeDropdown");
  });
});

describe("themeConfig.footer — Trust column", () => {
  it("includes Privacy, Terms, and Sub-processors links", () => {
    const theme = config.themeConfig as ThemeConfigShape;
    const allLabels = (theme.footer?.links ?? []).flatMap((column) =>
      column.items.map((i) => i.label),
    );
    for (const required of REQUIRED_FOOTER_LABELS) {
      expect(allLabels, `missing footer label: ${required}`).toContain(
        required,
      );
    }
  });
});
