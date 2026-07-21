import { describe, expect, it } from "vitest";
import config from "../docusaurus.config";

describe("docusaurus.config.ts — site identity", () => {
  it("has the canonical CoreLink site identity", () => {
    expect(config.title).toBe("CoreLink");
    // Canonical path-based mount: humangr.com/corelink/docs (not a subdomain).
    expect(config.url).toBe("https://humangr.com");
    expect(config.baseUrl).toBe("/corelink/docs/");
    expect(config.organizationName).toBe("HumanGuardrail");
    expect(config.projectName).toBe("corelink-server");
  });

  it("treats broken links and broken markdown links as fatal", () => {
    expect(config.onBrokenLinks).toBe("throw");
    expect(config.onBrokenMarkdownLinks).toBe("throw");
  });

  it("declares the og-image and favicon assets used by static/", () => {
    expect(config.favicon).toBe("img/favicon.svg");
    const themeConfig = config.themeConfig as Record<string, unknown>;
    expect(themeConfig.image).toBe("img/og-image.png");
  });
});
