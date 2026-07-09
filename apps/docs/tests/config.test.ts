import { describe, expect, it } from "vitest";
import config from "../docusaurus.config";

describe("docusaurus.config.ts — site identity", () => {
  it("has the canonical CoreLink site identity", () => {
    expect(config.title).toBe("CoreLink");
    expect(config.url).toBe("https://corelink-docs.humangr.com");
    expect(config.baseUrl).toBe("/");
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
