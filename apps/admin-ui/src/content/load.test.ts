import { describe, it, expect } from "vitest";
import { loadLocalizedMarkdown, loadSubProcessors, extractFrontMatterFromBody } from "./load";

describe("content/load", () => {
  it("loads localized privacy notice (en/pt/es)", () => {
    for (const l of ["en", "pt", "es"] as const) {
      const md = loadLocalizedMarkdown("privacy-notice", l);
      expect(md.length).toBeGreaterThan(100);
    }
  });

  it("falls back to en for unknown locale-like keys", () => {
    // @ts-expect-error intentional bad locale.
    const md = loadLocalizedMarkdown("privacy-notice", "xx");
    expect(md).toContain("Privacy Notice");
  });

  it("loads sub-processors JSON with required fields", () => {
    const data = loadSubProcessors();
    expect(data.version).toBeTruthy();
    expect(data.items.length).toBeGreaterThanOrEqual(3);
    for (const item of data.items) {
      expect(item.id).toBeTruthy();
      expect(item.certifications.length).toBeGreaterThan(0);
    }
  });

  it("extracts version + last-updated", () => {
    const md = "# Title\n\n**Version:** 1.2.3\n**Last updated:** 2026-05-14\n";
    const meta = extractFrontMatterFromBody(md);
    expect(meta.version).toBe("1.2.3");
    expect(meta.lastUpdated).toBe("2026-05-14");
  });
});
