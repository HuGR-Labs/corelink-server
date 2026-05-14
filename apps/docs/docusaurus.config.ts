import { themes as prismThemes } from "prism-react-renderer";
import type { Config } from "@docusaurus/types";

const config: Config = {
  title: "CoreLink",
  tagline: "Shared content-addressable cache for builds, packages, and ML.",
  favicon: "img/favicon.ico",
  url: "https://docs.corelink.dev",
  baseUrl: "/",
  organizationName: "humangr-labs",
  projectName: "corelink-server",
  onBrokenLinks: "warn",
  onBrokenMarkdownLinks: "warn",
  i18n: {
    defaultLocale: "en",
    locales: ["en"],
  },
  presets: [
    [
      "classic",
      {
        docs: {
          sidebarPath: "./sidebars.ts",
          routeBasePath: "/",
          // Underscore-prefixed dirs `_generated` and `_examples` are
          // intentional (mirror the spec naming). Override Docusaurus' default
          // exclusion of `_*` entries so the auto-generated REAPI reference
          // and the hand-written examples are part of the build.
          exclude: ["**/_*.{js,jsx,ts,tsx}", "**/*.test.{js,jsx,ts,tsx}", "**/__tests__/**"],
        },
        blog: false,
        theme: {
          customCss: "./src/css/custom.css",
        },
      },
    ],
  ],
  themeConfig: {
    navbar: {
      title: "CoreLink",
      items: [
        { to: "/tutorial/installation", label: "Tutorial", position: "left" },
        { to: "/reference/reapi", label: "REAPI Reference", position: "left" },
        { href: "https://github.com/humangr-labs/corelink-server", label: "GitHub", position: "right" },
      ],
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: [],
    },
  },
};

export default config;
