import { describe, expect, it } from "vitest";
import config from "../docusaurus.config";

interface ThemeConfigShape {
  algolia?: {
    appId?: string;
    apiKey?: string;
    indexName?: string;
    contextualSearch?: boolean;
  };
}

describe("themeConfig.algolia — DocSearch stub", () => {
  it("declares all four canonical Algolia fields", () => {
    const theme = config.themeConfig as ThemeConfigShape;
    expect(theme.algolia).toBeDefined();
    expect(theme.algolia?.appId).toBeTruthy();
    expect(theme.algolia?.apiKey).toBeTruthy();
    expect(theme.algolia?.indexName).toBe("corelink");
    expect(theme.algolia?.contextualSearch).toBe(true);
  });

  it("does NOT ship an admin key (apiKey must be the search-only public key)", () => {
    const theme = config.themeConfig as ThemeConfigShape;
    const apiKey = theme.algolia?.apiKey ?? "";
    // Admin keys are typically prefixed `admin_`. Public search keys are
    // safe to ship in the static bundle. This guard prevents accidental
    // leakage at D-day key swap.
    expect(apiKey.toLowerCase()).not.toContain("admin");
  });
});
