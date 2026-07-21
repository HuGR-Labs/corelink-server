import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const STATIC_DIR = path.resolve(__dirname, "..", "static");

describe("static/ assets", () => {
  // No CNAME: the docs are served by a Worker (Static Assets) mounted at the
  // canonical path humangr.com/corelink/docs, not a Pages custom domain.

  it("ships a robots.txt that allows crawling and points at the sitemap", () => {
    const robots = fs.readFileSync(
      path.join(STATIC_DIR, "robots.txt"),
      "utf8",
    );
    expect(robots).toMatch(/User-agent:\s*\*/);
    expect(robots).toMatch(/Allow:\s*\//);
    expect(robots).toMatch(
      /Sitemap:\s*https:\/\/humangr\.com\/corelink\/docs\/sitemap\.xml/,
    );
  });

  it("ships favicon and logo SVGs referenced by docusaurus.config.ts", () => {
    expect(fs.existsSync(path.join(STATIC_DIR, "img", "favicon.svg"))).toBe(
      true,
    );
    expect(fs.existsSync(path.join(STATIC_DIR, "img", "logo.svg"))).toBe(
      true,
    );
  });
});
