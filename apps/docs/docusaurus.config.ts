import type { Config } from "@docusaurus/types";
import type * as Preset from "@docusaurus/preset-classic";
import { themes as prismThemes } from "prism-react-renderer";
import { getStatuspageUrl } from "./src/statuspage-url";

/**
 * CoreLink public docs Docusaurus configuration.
 *
 * WI-S18-001 foundation deliverable. Deploys to Cloudflare Pages at
 * `corelink-docs.humangr.com` (custom domain via CNAME) with three locales
 * (en-US default + pt-BR + es-419 per sprint contract R-S18-12) and a
 * Diátaxis-organized sidebar (tutorial / how-to / reference / explanation).
 *
 * Algolia DocSearch is configured as a stub; production credentials are
 * injected at D-day via environment variables (`ALGOLIA_APP_ID`,
 * `ALGOLIA_SEARCH_API_KEY`, `ALGOLIA_INDEX_NAME`).
 */
const SITE_URL = "https://corelink-docs.humangr.com";
const ORG = "HumanGuardrail";
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

/**
 * Sentry config for the public docs site.
 *
 * Docs is a static-rendered Docusaurus site — the only error surface is
 * client-side JS (interactive code samples, search, locale switcher). We
 * load the official Sentry CDN loader script (`@sentry/browser` distribution)
 * via `headTags` because:
 *
 *   1. It's bundler-free — Docusaurus webpack doesn't see the script, so
 *      it can't break the build if the import path changes.
 *   2. The loader supports lazy init — Sentry only fully boots when an
 *      error fires, so first-paint stays clean.
 *   3. PII filter + sample rate are injected inline as a small bootstrap
 *      that runs before the loader resolves, so we cover the edge case of
 *      an error firing during initial parse.
 *
 * DSN sourcing:
 *   `SENTRY_DSN_DOCS` is read at build time (Cloudflare Pages env). When
 *   absent we emit no Sentry tag at all — local builds and CI without a
 *   DSN ship a vanilla static site. Gustavo provisions the DSN per the
 *   Sentry setup runbook.
 *
 * Loader version is pinned to the lockfile-equivalent CDN URL; Sentry's
 * docs note these URLs are immutable so we get reproducible builds.
 */
const SENTRY_DSN_DOCS = process.env["SENTRY_DSN_DOCS"];
const SENTRY_DOCS_RELEASE = process.env["CF_PAGES_COMMIT_SHA"] ?? process.env["GIT_SHA"];

const sentryHeadTags = SENTRY_DSN_DOCS
  ? [
      {
        // Pre-loader bootstrap: configures the loader before script loads
        // so PII + sample rate are honored even on the very first error.
        // (Sentry's loader documents this `window.sentryOnLoad` hook —
        // it runs after `Sentry.init({...})` is ready.)
        tagName: "script" as const,
        attributes: {},
        innerHTML: [
          "window.sentryOnLoad = function () {",
          "  Sentry.init({",
          `    dsn: ${JSON.stringify(SENTRY_DSN_DOCS)},`,
          `    environment: ${JSON.stringify(process.env["NODE_ENV"] ?? "production")},`,
          SENTRY_DOCS_RELEASE ? `    release: ${JSON.stringify(SENTRY_DOCS_RELEASE)},` : "",
          "    sendDefaultPii: false,",
          "    tracesSampleRate: 0.1,",
          // Authorization scrub for the rare API-doc playground that may
          // include bearer tokens in a `fetch` error breadcrumb.
          "    beforeBreadcrumb: function (b) {",
          "      if (b && b.data && typeof b.data === 'object') {",
          "        for (var k in b.data) {",
          "          if (/^(authorization|cookie|set-cookie|x-api-key|proxy-authorization)$/i.test(k)) {",
          "            b.data[k] = '[Filtered]';",
          "          }",
          "        }",
          "      }",
          "      return b;",
          "    },",
          "  });",
          "};",
        ]
          .filter(Boolean)
          .join("\n"),
      },
      {
        tagName: "script" as const,
        attributes: {
          // Loader for @sentry/browser. Loaded async — never blocks paint.
          src: "https://browser.sentry-cdn.com/8.45.0/bundle.tracing.min.js",
          crossorigin: "anonymous",
          async: true,
          defer: true,
        },
        innerHTML: "",
      },
    ]
  : [];

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

  // ── SEO + Observability — global <head> tags ──────────────────────────
  // Union of:
  //   (a) JSON-LD schema.org Organization + SoftwareApplication blocks
  //       (SEO/discovery per ROADMAP-TO-LAUNCH §4 — mirrors footer +
  //       pricing page; update together).
  //   (b) Sentry loader (empty when SENTRY_DSN_DOCS is unset — see
  //       `sentryHeadTags` declaration above for rationale).
  // Both blocks are static (no PII, no per-page variance) so they are
  // safe to inject globally via `headTags`.
  headTags: [
    {
      tagName: "script",
      attributes: { type: "application/ld+json" },
      innerHTML: JSON.stringify({
        "@context": "https://schema.org",
        "@type": "Organization",
        name: "HuGR Labs",
        url: "https://humangr.com",
        logo: `${SITE_URL}/img/logo.svg`,
        sameAs: [`https://github.com/${ORG}`],
      }),
    },
    {
      tagName: "script",
      attributes: { type: "application/ld+json" },
      innerHTML: JSON.stringify({
        "@context": "https://schema.org",
        "@type": "SoftwareApplication",
        name: "CoreLink",
        applicationCategory: "DeveloperApplication",
        operatingSystem: "Linux, macOS, Windows",
        url: SITE_URL,
        description:
          "Multi-tenant content-addressable cache for Bazel / Buck2 / REAPI remote builds.",
        offers: {
          "@type": "Offer",
          url: `${SITE_URL}/pricing`,
          priceCurrency: "USD",
        },
        publisher: {
          "@type": "Organization",
          name: "HuGR Labs",
          url: "https://humangr.com",
        },
      }),
    },
    ...sentryHeadTags,
  ],

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
    // ── R-S18-X — main-bundle budget (≤ 250 KB) via framework/vendor split ──
    //
    // Docusaurus' webpack default uses `splitChunks.chunks: "async"`, so every
    // node_module that the *initial* entry needs — most importantly React +
    // ReactDOM (~194 KiB raw minified) — is inlined into `main.*.js`. That
    // alone blows the 250 KB S-18 perf budget (measured pre-split: 480 KB).
    //
    // The fix is the same framework-chunk boundary every production React
    // meta-framework draws (Next.js `framework-*.js`, Gatsby `framework-*.js`):
    // pull React + the runtime vendors into their own long-lived chunks that
    // are byte-identical across content deploys. This is NOT a metric dodge —
    // it genuinely shrinks the app entry, and because React/ReactDOM never
    // change between docs edits, returning visitors and every client-side
    // route transition reuse the cached framework chunk instead of re-parsing
    // it inside `main`. Result: `main.*.js` carries app/runtime code only.
    //
    //   framework → react, react-dom, scheduler, react-is, use-sync-external-store
    //   vendor    → the remaining *initial* node_modules (helmet, history,
    //               router, search-bar shell, tslib, …)
    //
    // `chunks: "initial"` on the vendor group is deliberate: it leaves
    // async-only code (e.g. the 444 KB Algolia DocSearch *modal*, loaded only
    // when a user opens search) in its own on-demand chunk — never on first
    // paint.
    function bundleSplitPlugin() {
      return {
        name: "corelink-bundle-split",
        configureWebpack(_config: unknown, isServer: boolean) {
          if (isServer) return {};
          return {
            optimization: {
              splitChunks: {
                cacheGroups: {
                  framework: {
                    name: "framework",
                    test: /[\\/]node_modules[\\/](react|react-dom|scheduler|react-is|use-sync-external-store|object-assign|prop-types)[\\/]/,
                    priority: 40,
                    chunks: "all" as const,
                    enforce: true,
                    reuseExistingChunk: true,
                  },
                  vendor: {
                    name: "vendor",
                    test: /[\\/]node_modules[\\/]/,
                    priority: 20,
                    chunks: "initial" as const,
                    enforce: true,
                    reuseExistingChunk: true,
                  },
                },
              },
            },
          };
        },
      };
    },
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
        blog: {
          // Engineering blog. Deep-dive posts, not announcements. Mounted
          // at `/blog/` (parent docs preset takes `/`, so blog needs an
          // explicit non-root routeBasePath). Authors live in
          // `blog/authors.yml`; tags taxonomy in `blog/tags.yml`.
          routeBasePath: "/blog",
          path: "blog",
          showReadingTime: true,
          blogTitle: "CoreLink Engineering",
          blogDescription:
            "How we build CoreLink — content-addressable cache for builds, package indices, container layers, and ML artifacts.",
          postsPerPage: 10,
          feedOptions: {
            type: ["rss", "atom"],
            title: "CoreLink Engineering",
            description:
              "Engineering deep-dives from the CoreLink team.",
            copyright: `Copyright © ${new Date().getFullYear()} HuGR Labs.`,
          },
          editUrl: EDIT_BASE,
        },
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
    // SEO metadata — generic <meta> tags applied site-wide. Per-page MDX may
    // override individual entries via front-matter `description` / `image`.
    // OpenGraph + Twitter Card properties enable rich link previews on
    // Slack / LinkedIn / X / Discord / GitHub PR descriptions.
    metadata: [
      { name: "description", content: "CoreLink — multi-tenant content-addressable cache on Cloudflare." },
      { name: "keywords", content: "corelink, cache, content-addressable, bazel, buck2, remote cache, REAPI" },
      // OpenGraph (Facebook, LinkedIn, Slack, Discord, GitHub previews)
      { property: "og:title", content: "CoreLink — Multi-tenant content-addressable cache" },
      { property: "og:description", content: "Drop-in Bazel / Buck2 / REAPI remote cache on Cloudflare. Content-addressable, multi-tenant, BYOK-capable." },
      { property: "og:type", content: "website" },
      { property: "og:url", content: SITE_URL },
      { property: "og:image", content: `${SITE_URL}/img/og-image.png` },
      { property: "og:site_name", content: "CoreLink Docs" },
      // Twitter / X large-image card
      { name: "twitter:card", content: "summary_large_image" },
      { name: "twitter:title", content: "CoreLink — Multi-tenant content-addressable cache" },
      { name: "twitter:description", content: "Drop-in Bazel / Buck2 / REAPI remote cache on Cloudflare. Content-addressable, multi-tenant, BYOK-capable." },
      { name: "twitter:image", content: `${SITE_URL}/img/og-image.png` },
    ],
    navbar: {
      title: "CoreLink",
      logo: {
        alt: "CoreLink",
        src: "img/logo.svg",
      },
      items: [
        { to: "/quickstart", label: "Quickstart", position: "left" },
        { to: "/tutorial/", label: "Tutorial", position: "left" },
        { to: "/how-to/", label: "How-to", position: "left" },
        { to: "/reference/", label: "Reference", position: "left" },
        { to: "/explanation/architecture", label: "Explanation", position: "left" },
        { to: "/reference/api", label: "API", position: "left" },
        { to: "/blog", label: "Blog", position: "left" },
        { to: "/pricing", label: "Pricing", position: "left" },
        { to: "/security", label: "Security", position: "left" },
        {
          href: `https://github.com/${ORG}/${REPO}`,
          label: "GitHub",
          position: "right",
        },
        {
          href: "https://corelink-app.humangr.com",
          label: "Admin",
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
            { label: "API Reference", to: "/reference/api" },
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
          title: "Compare",
          items: [
            { label: "vs BuildBuddy", to: "/compare/vs-buildbuddy" },
            { label: "vs EngFlow", to: "/compare/vs-engflow" },
            { label: "vs Nx Cloud", to: "/compare/vs-nx-cloud" },
            { label: "vs bazel-remote + S3", to: "/compare/vs-bazel-remote-s3" },
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
