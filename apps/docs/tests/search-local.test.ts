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

interface LocalSearchPluginOptions {
  hashed?: boolean | "query" | "filename";
  docsRouteBasePath?: string | string[];
  blogRouteBasePath?: string | string[];
  language?: string | string[];
}

describe("search — @easyops-cn/docusaurus-search-local (replaces the dead Algolia stub)", () => {
  // Regression guard: the Algolia DocSearch stub (`ALGOLIA_APP_ID ?? "STUB_APP_ID"`)
  // shipped `STUB_APP_ID` in production HTML because no ALGOLIA_* secrets were
  // ever provisioned. Fixed by dropping Algolia entirely in favor of a free,
  // fully offline local index — assert it stays gone.
  it("does NOT declare a themeConfig.algolia block", () => {
    const theme = config.themeConfig as ThemeConfigShape;
    expect(theme.algolia).toBeUndefined();
  });

  it("registers the local-search plugin with a hashed, offline-built index", () => {
    const plugins = config.plugins ?? [];
    const localSearchEntry = plugins.find(
      (p) => Array.isArray(p) && p[0] === "@easyops-cn/docusaurus-search-local",
    );
    expect(localSearchEntry).toBeDefined();

    const [, options] = localSearchEntry as [string, LocalSearchPluginOptions];
    // `hashed: true` — the index only rebuilds when doc/blog content actually
    // changes, so search stays evergreen without pinning stale cache.
    expect(options.hashed).toBe(true);
    // Docs preset uses `routeBasePath: "/"` (docs-only-style mount at the
    // baseUrl root); the local-search index must match or it indexes nothing.
    expect(options.docsRouteBasePath).toBe("/");
    expect(options.blogRouteBasePath).toBe("/blog");
    // One lunr language per configured i18n locale (en-US, pt-BR, es-419, de).
    expect(options.language).toEqual(["en", "pt", "es", "de"]);
  });

  it("does not reference any Algolia dependency, key, or index name in config", () => {
    const serialized = JSON.stringify(config);
    expect(serialized.toLowerCase()).not.toContain("algolia");
    expect(serialized).not.toContain("STUB_APP_ID");
  });
});
