import type { SidebarsConfig } from "@docusaurus/plugin-content-docs";

/**
 * Diátaxis 4-quadrant sidebar structure.
 *
 * The four canonical Diátaxis modes — tutorial (learning-oriented),
 * how-to (task-oriented), reference (information-oriented), and
 * explanation (understanding-oriented) — are first-class top-level
 * categories. Subsequent work items (WI-S18-002..005) populate them.
 *
 * Per Quality Standard 14.s18.2, every documentation page MUST be filed
 * under exactly one of these four categories. PR review enforces this.
 */
const sidebars: SidebarsConfig = {
  defaultSidebar: [
    {
      type: "doc",
      id: "index",
      label: "Welcome",
    },
    {
      type: "category",
      label: "Tutorial",
      link: { type: "doc", id: "tutorial/index" },
      collapsed: false,
      items: [],
    },
    {
      type: "category",
      label: "How-to",
      link: { type: "doc", id: "how-to/index" },
      collapsed: true,
      items: [],
    },
    {
      type: "category",
      label: "Reference",
      link: { type: "doc", id: "reference/index" },
      collapsed: true,
      items: [],
    },
    {
      type: "category",
      label: "Explanation",
      link: { type: "doc", id: "explanation/architecture" },
      collapsed: true,
      items: ["explanation/architecture"],
    },
  ],
};

export default sidebars;
