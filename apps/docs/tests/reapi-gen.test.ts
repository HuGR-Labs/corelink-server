/**
 * WI-S18-002 — REAPI auto-gen smoke test.
 *
 * Re-runs the generator against the live proto set and verifies every
 * service expected by the spec contract produces a non-empty MDX page.
 */
import { execSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const DOCS_ROOT = resolve(__dirname, "..");
const GEN_DIR = join(DOCS_ROOT, "docs", "reference", "reapi", "_generated");

describe("REAPI auto-generator", () => {
  it("regenerates without drift (pnpm docs:gen:check exits 0)", () => {
    expect(() => {
      execSync("pnpm docs:gen:check", { cwd: DOCS_ROOT, stdio: "pipe" });
    }).not.toThrow();
  });

  it("emits at least one MDX page per vendored service", () => {
    const expected = ["ByteStream", "Capabilities", "ContentAddressableStorage", "Health"];
    for (const svc of expected) {
      const file = join(GEN_DIR, `${svc}.mdx`);
      expect(existsSync(file), `missing generated page for ${svc}`).toBe(true);
      const content = readFileSync(file, "utf8");
      expect(content.length, `${svc}.mdx is empty`).toBeGreaterThan(200);
      expect(content).toContain(`# ${svc}`);
      expect(content).toContain("## Methods");
    }
  });

  it("each generated page declares Docusaurus frontmatter", () => {
    const pages = readdirSync(GEN_DIR).filter((f) => f.endsWith(".mdx"));
    expect(pages.length).toBeGreaterThan(0);
    for (const page of pages) {
      const txt = readFileSync(join(GEN_DIR, page), "utf8");
      expect(txt.startsWith("---\n"), `${page} missing frontmatter`).toBe(true);
      expect(txt).toMatch(/^id: /m);
      expect(txt).toMatch(/^title: /m);
    }
  });

  it("each generated page is non-empty", () => {
    const pages = readdirSync(GEN_DIR).filter((f) => f.endsWith(".mdx"));
    for (const page of pages) {
      const st = statSync(join(GEN_DIR, page));
      expect(st.size, `${page} is empty`).toBeGreaterThan(100);
    }
  });
});
