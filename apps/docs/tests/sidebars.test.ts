import { describe, expect, it } from "vitest";
import sidebars from "../sidebars";

describe("sidebars.ts — Diátaxis 4-quadrant structure", () => {
  it("exposes a single defaultSidebar entry", () => {
    expect(Object.keys(sidebars)).toEqual(["defaultSidebar"]);
  });

  it("contains exactly the four Diátaxis categories (tutorial/how-to/reference/explanation)", () => {
    const sidebar = sidebars.defaultSidebar;
    if (!Array.isArray(sidebar)) {
      throw new Error("expected defaultSidebar to be an array");
    }

    const categoryLabels = sidebar
      .filter(
        (entry): boolean =>
          typeof entry === "object" &&
          entry !== null &&
          "type" in entry &&
          (entry as { type?: string }).type === "category",
      )
      .map((entry) => (entry as { label?: string }).label);

    expect(categoryLabels).toEqual([
      "Tutorial",
      "How-to",
      "Reference",
      "Explanation",
    ]);
  });

  it("includes a top-level Welcome doc entry", () => {
    const sidebar = sidebars.defaultSidebar;
    if (!Array.isArray(sidebar)) throw new Error("expected array");

    const welcome = sidebar.find(
      (entry) =>
        typeof entry === "object" &&
        entry !== null &&
        "type" in entry &&
        entry.type === "doc" &&
        "id" in entry &&
        entry.id === "index",
    );
    expect(welcome).toBeDefined();
  });
});
