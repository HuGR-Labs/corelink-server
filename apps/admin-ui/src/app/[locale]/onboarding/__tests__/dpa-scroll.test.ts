import { describe, it, expect } from "vitest";
import { hasScrolledToEnd, buildAcceptanceRecord } from "@/lib/dpa-scroll";

describe("hasScrolledToEnd", () => {
  it("returns false when at the top of a long doc", () => {
    expect(
      hasScrolledToEnd({ scrollTop: 0, scrollHeight: 2000, clientHeight: 600 }),
    ).toBe(false);
  });

  it("returns false near but not at the bottom", () => {
    expect(
      hasScrolledToEnd({
        scrollTop: 1200,
        scrollHeight: 2000,
        clientHeight: 600,
      }),
    ).toBe(false);
  });

  it("returns true when scrolled exactly to the bottom", () => {
    expect(
      hasScrolledToEnd({
        scrollTop: 1400,
        scrollHeight: 2000,
        clientHeight: 600,
      }),
    ).toBe(true);
  });

  it("returns true within 8px of the bottom (sub-pixel slack)", () => {
    expect(
      hasScrolledToEnd({
        scrollTop: 1395,
        scrollHeight: 2000,
        clientHeight: 600,
      }),
    ).toBe(true);
  });

  it("builds an acceptance record carrying version + locale + hash + ts", () => {
    const ts = new Date("2026-05-14T00:00:00Z");
    const rec = buildAcceptanceRecord("1.0.0", "pt", "abc123", ts);
    expect(rec).toEqual({
      version: "1.0.0",
      locale: "pt",
      noticeTextHash: "abc123",
      acceptedAt: "2026-05-14T00:00:00.000Z",
    });
  });
});
