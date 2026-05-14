import { describe, it, expect } from "vitest";
import { formatDate, formatNumber, formatCurrency, bcp47 } from "./format";

describe("i18n format", () => {
  it("maps Locale to BCP-47 tag", () => {
    expect(bcp47("en")).toBe("en-US");
    expect(bcp47("pt")).toBe("pt-BR");
    expect(bcp47("es")).toBe("es-419");
  });

  it("formats numbers per locale (grouping)", () => {
    // pt-BR uses '.' as thousand-sep, en-US uses ','.
    expect(formatNumber(1234567.89, "pt")).toContain("1.234.567");
    expect(formatNumber(1234567.89, "en")).toContain("1,234,567");
  });

  it("formats currency per locale + ISO code", () => {
    const usd = formatCurrency(99.5, "USD", "en");
    expect(usd).toContain("99.50");
    const brl = formatCurrency(99.5, "BRL", "pt");
    expect(brl).toMatch(/R\$/);
  });

  it("formats dates per locale", () => {
    const out = formatDate("2026-05-14", "en", { year: "numeric", month: "long", day: "numeric" });
    expect(out).toMatch(/May/);
  });
});
