import { describe, expect, it } from "vitest";
import sidebars from "../sidebars";

describe("sidebars.ts — Diátaxis 4-quadrant structure", () => {
  it("exposes a single defaultSidebar entry", () => {
    expect(Object.keys(sidebars)).toEqual(["defaultSidebar"]);
  });

  it("contains the four Diátaxis categories plus the Trust quadrant", () => {
    // The four canonical Diátaxis quadrants — tutorial / how-to / reference /
    // explanation — are present as top-level categories. The "Tutorial" label
    // was renamed to "Get Started" per R-8 GA launch checklist (single most-
    // clicked post-launch entry-point; see `apps/docs/sidebars.ts` line 24
    // comment and `.github/workflows/quickstart-validate.yml`).
    //
    // The "Trust" quadrant is an additive top-level category for the customer-
    // facing trust-portal pages (compliance / security / pricing) under
    // cross-functional review per S-18 spec contract §10 anti-scope. It is
    // not a Diátaxis quadrant; it is a separate gate (see PRR-S18 §9).
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
      "Get Started",
      "How-to",
      "Reference",
      "Explanation",
      "Trust",
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
