import type { Config } from "@docusaurus/types";
import type * as Preset from "@docusaurus/preset-classic";
import { themes as prismThemes } from "prism-react-renderer";
import { getStatuspageUrl } from "./src/statuspage-url";

/**
 * CoreLink public docs Docusaurus configuration.
 *
 * WI-S18-001 foundation deliverable. Deploys to Cloudflare Pages at
 * `docs.corelink.humangr.com` (custom domain via CNAME) with three locales
 * (en-US default + pt-BR + es-419 per sprint contract R-S18-12) and a
 * Diátaxis-organized sidebar (tutorial / how-to / reference / explanation).
 *
 * Algolia DocSearch is configured as a stub; production credentials are
 * injected at D-day via environment variables (`ALGOLIA_APP_ID`,
 * `ALGOLIA_SEARCH_API_KEY`, `ALGOLIA_INDEX_NAME`).
 */
const SITE_URL = "https://docs.corelink.humangr.com";
const ORG = "humangr-labs";
const REPO = "corelink-server";
const EDIT_BASE = `https://github.com/${ORG}/${REPO}/edit/main/apps/docs/`;

/**
 * Canonical default statuspage URL (DEBT-016 closure, R-prep wave-24).
 *
 * Operator-bound provisioning (see `specs/_runbooks/STATUSPAGE-INIT.md`):
 *
 *   Option A — CNAME (zero docs rebuild, preferred):
 *     Operator owns `status.corelink.humangr.com` DNS and CNAMEs it to the real
 *     Atlassian Statuspage instance (e.g. `corelink.statuspage.io`). All
 *     literal URLs in MDX trust pages resolve correctly with no rebuild.
 *
 *   Option B — env-var override (rebuild required):
 *     Operator sets `STATUSPAGE_URL=https://status.example.com` before
 *     `pnpm build`. Trust-page MDX consumes the URL via the
 *     `siteConfig.customFields.statuspageUrl` accessor (used by shared
 *     components / `getStatuspageUrl()` helper). Existing literal
 *     `https://status.corelink.humangr.com` references remain as the **default
 *     canonical host** — Option A is the preferred provisioning path.
 *
 * The default value is the canonical wave-19 commit value referenced from
 * 5 customer-facing trust pages × 4 locales (en/pt-BR/es-419/de) and
 * 20+ internal runbooks + spec docs.
 */
const STATUSPAGE_URL = getStatuspageUrl();

const config: Config = {
  title: "CoreLink",
  tagline: "Multi-tenant content-addressable cache on Cloudflare",
  favicon: "img/favicon.svg",
  url: SITE_URL,
  baseUrl: "/",
  organizationName: ORG,
  projectName: REPO,
  trailingSlash: false,
  onBrokenLinks: "throw",
  onBrokenMarkdownLinks: "throw",
  noIndex: false,

  // DEBT-016 — exposed to MDX/components via `useDocusaurusContext()`
  // (`siteConfig.customFields.statuspageUrl`). Default kept canonical
  // (`https://status.corelink.humangr.com`); operator override via `STATUSPAGE_URL`
  // env var at build time (see `specs/_runbooks/STATUSPAGE-INIT.md`).
  customFields: {
    statuspageUrl: STATUSPAGE_URL,
  },

  i18n: {
    defaultLocale: "en-US",
    locales: ["en-US", "pt-BR", "es-419", "de"],
    localeConfigs: {
      "en-US": { label: "English", direction: "ltr", htmlLang: "en-US" },
      "pt-BR": { label: "Português (Brasil)", direction: "ltr", htmlLang: "pt-BR" },
      "es-419": { label: "Español (Latinoamérica)", direction: "ltr", htmlLang: "es-419" },
      // R-prep i18n-de — German added for EU enterprise GA buyers (DACH region).
      // Native-speaker review pending; current shadow is MT-stub-seeded per
      // TRANSLATION-WORKFLOW.md SLA (≤ 14 d of EN change).
      de: { label: "Deutsch", direction: "ltr", htmlLang: "de" },
    },
  },

  // ── Phase 0.G — Plausible analytics ───────────────────────────────────
  // Lightweight, cookieless web analytics for the marketing-site funnel
  // (landing → /pricing → /sign-up) per metrics audit §3 + §8.1.
  // `defer` so the script never blocks first paint, and the noscript Image
  // fallback ensures the visit still counts when JS is blocked.
  //
  // Consent gating: Plausible is cookieless (no PII, no fingerprint), which
  // makes it lawful as "necessary measurement" under both ePrivacy and LGPD
  // without an opt-in dialog. We still document the choice in the privacy
  // page; the admin-ui dashboard uses the existing cookie-consent gate on a
  // separate Plausible domain (`corelink-admin.humangr.com`).
  scripts: [
    {
      src: "https://plausible.io/js/script.js",
      defer: true,
      "data-domain": "corelink-docs.humangr.com",
    },
  ],

  plugins: [
    // Phase 0 §A `LEGAL-FOOTER-WIRE` — alias legacy compliance paths to the
    // public `/legal/*` surface and absorb common visitor typos. Footer links
    // (themeConfig.footer) keep their canonical `/legal/{privacy,terms,sub-processors}`
    // targets; redirects below cover deep-link continuity from older external
    // references and the docs-internal compliance/privacy explainers.
    [
      "@docusaurus/plugin-client-redirects",
      {
        // Targets must be live routes. Internal compliance explainers
        // (`docs/explanation/compliance/dpa.mdx`, `privacy/gdpr.mdx`) are
        // `draft: true` and excluded from production builds; until they
        // lift draft status post-Legal+DPO review, `/legal/dpa` lands on
        // the public privacy page (which links onward to the DPA explainer
        // when published). The DPA-direct redirect is added here so it
        // auto-upgrades the moment the explainer page goes live without
        // touching this config again — just lift the draft flag.
        redirects: [
          { from: "/legal/dpa", to: "/legal/privacy" },
          { from: "/privacy", to: "/legal/privacy" },
          { from: "/terms", to: "/legal/terms" },
          { from: "/sub-processors", to: "/legal/sub-processors" },
        ],
      },
    ],
  ],

  presets: [
    [
      "classic",
      {
        docs: {
          sidebarPath: "./sidebars.ts",
          routeBasePath: "/",
          editUrl: EDIT_BASE,
          showLastUpdateAuthor: true,
          showLastUpdateTime: true,
          versions: {
            current: { label: "Latest", path: "" },
          },
        },
        blog: false,
        theme: {
          customCss: "./src/css/custom.css",
        },
        sitemap: {
          changefreq: "weekly",
          priority: 0.5,
          filename: "sitemap.xml",
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    image: "img/og-image.png",
    colorMode: {
      defaultMode: "light",
      disableSwitch: false,
      respectPrefersColorScheme: true,
    },
    metadata: [
      { name: "description", content: "CoreLink — multi-tenant content-addressable cache on Cloudflare." },
      { name: "keywords", content: "corelink, cache, content-addressable, bazel, buck2, remote cache, REAPI" },
    ],
    navbar: {
      title: "CoreLink",
      logo: {
        alt: "CoreLink",
        src: "img/logo.svg",
      },
      items: [
        { to: "/tutorial/", label: "Tutorial", position: "left" },
        { to: "/how-to/", label: "How-to", position: "left" },
        { to: "/reference/", label: "Reference", position: "left" },
        { to: "/explanation/architecture", label: "Explanation", position: "left" },
        { to: "/reference/api", label: "API", position: "left" },
        { to: "/pricing", label: "Pricing", position: "left" },
        { to: "/security", label: "Security", position: "left" },
        {
          href: `https://github.com/${ORG}/${REPO}`,
          label: "GitHub",
          position: "right",
        },
        { type: "localeDropdown", position: "right" },
        { type: "search", position: "right" },
      ],
    },
    footer: {
      style: "dark",
      links: [
        {
          title: "Product",
          items: [
            { label: "Tutorial", to: "/tutorial/" },
            { label: "How-to", to: "/how-to/" },
            { label: "Reference", to: "/reference/" },
            { label: "Pricing", to: "/pricing" },
          ],
        },
        {
          title: "Trust",
          items: [
            { label: "Security", to: "/security" },
            { label: "Privacy", to: "/legal/privacy" },
            { label: "Terms", to: "/legal/terms" },
            { label: "Sub-processors", to: "/legal/sub-processors" },
          ],
        },
        {
          title: "Community",
          items: [
            { label: "GitHub", href: `https://github.com/${ORG}/${REPO}` },
            { label: "Edit this site", href: EDIT_BASE },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} HuGR Labs. Built with Docusaurus.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ["bash", "diff", "json", "rust", "toml", "yaml", "python", "go"],
    },
    algolia: {
      // Production keys injected at D-day. These stubs allow the build to
      // succeed locally and in CI without secrets.
      appId: process.env.ALGOLIA_APP_ID ?? "STUB_APP_ID",
      apiKey: process.env.ALGOLIA_SEARCH_API_KEY ?? "stub_search_only_api_key_replace_at_dday",
      indexName: process.env.ALGOLIA_INDEX_NAME ?? "corelink",
      contextualSearch: true,
      searchPagePath: "search",
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
