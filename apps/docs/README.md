# `apps/docs` — CoreLink public documentation

Public documentation for CoreLink, published to
[docs.corelink.dev](https://docs.corelink.dev) on Cloudflare Pages.

Built with [Docusaurus 3](https://docusaurus.io/), organized per the
[Diátaxis framework](https://diataxis.fr/) (tutorial / how-to /
reference / explanation), with three locales (en-US default + pt-BR +
es-419) and Algolia DocSearch.

Foundation delivered by **WI-S18-001**.

## Quickstart

```bash
# from repo root
pnpm install --frozen-lockfile

# dev server with hot reload on http://localhost:3000
pnpm --filter @corelink/docs dev

# production build → apps/docs/build/
pnpm --filter @corelink/docs build

# serve the production build locally
pnpm --filter @corelink/docs serve

# typecheck + lint + tests
pnpm --filter @corelink/docs typecheck
pnpm --filter @corelink/docs lint
pnpm --filter @corelink/docs test
```

## Diátaxis taxonomy

Every page MUST live under exactly one of four top-level categories.
PR review enforces this per **Quality Standard 14.s18.2**.

| Mode | Folder | Purpose |
|---|---|---|
| Tutorial | `docs/tutorial/` | Learning-oriented. Teaches by doing. |
| How-to | `docs/how-to/` | Task-oriented. Recipes for known goals. |
| Reference | `docs/reference/` | Information-oriented. Truth about the API. |
| Explanation | `docs/explanation/` | Understanding-oriented. The *why*. |

When you draft a new page, ask: **is this teaching, doing, looking up,
or understanding?** That answer picks the folder. If unsure, read the
[Diátaxis decision guide](https://diataxis.fr/how-to-use-diataxis/).

## Adding a new locale

1. Decide the canonical locale tag (e.g. `fr-FR`).
2. Add it to `docusaurus.config.ts → i18n.locales` and provide a
   matching `localeConfigs[tag]` entry with `label`, `direction`, and
   `htmlLang`.
3. Run:
   ```bash
   pnpm --filter @corelink/docs write-translations -- --locale fr-FR
   ```
   This generates `i18n/fr-FR/` skeletons for code.json, navbar.json,
   footer.json, and the docs plugin.
4. Hand off the JSON files to a native speaker reviewer (closing ship
   gate WI-S18-005 requires this; see Quality Standard 14.s18.5).
5. Add a `tests/i18n.test.ts` assertion so the build fails if any
   required JSON file is missing.
6. Translate the MDX content under `i18n/fr-FR/docusaurus-plugin-content-docs/current/`.

## Custom domain DNS setup (Cloudflare Pages)

1. In the Cloudflare dashboard, open the **corelink-docs** Pages
   project → **Custom domains** → **Set up a custom domain**.
2. Enter `docs.corelink.dev`.
3. Cloudflare auto-creates a `CNAME docs → <project>.pages.dev` record
   on the `corelink.dev` zone (since the zone is on the same account).
   No manual DNS change is needed if the zone is on Cloudflare; if the
   zone lives elsewhere, add a `CNAME` record manually.
4. SSL: Cloudflare provisions a Let's Encrypt certificate automatically
   within ~5 minutes. HSTS is enabled at the zone level.
5. Verify: `dig docs.corelink.dev` returns a `*.pages.dev` CNAME, and
   `curl -I https://docs.corelink.dev` returns `200` with
   `strict-transport-security` present.

The repo also ships `static/CNAME` so that mirror deploys to GitHub
Pages would resolve the same custom domain — Cloudflare Pages ignores
`CNAME` but it documents intent.

## Algolia DocSearch

Search is configured in `docusaurus.config.ts → themeConfig.algolia`
with stub credentials (`STUB_APP_ID` + `stub_search_only_api_key_replace_at_dday`).
At D-day, set the following Cloudflare Pages environment variables:

| Variable | Source |
|---|---|
| `ALGOLIA_APP_ID` | Algolia DocSearch onboarding email |
| `ALGOLIA_SEARCH_API_KEY` | **Search-only** key (never the admin key) |
| `ALGOLIA_INDEX_NAME` | `corelink` (already the default) |

`tests/algolia.test.ts` asserts the apiKey field does NOT contain
`admin`, preventing accidental admin-key leakage in the static bundle.

## Known limitation: local production build

`pnpm build` currently fails on Docusaurus 3.10.1 inside this
pnpm-isolated monorepo with the symptoms:

1. `server.bundle.js` contains literal `require.resolveWeak(...)` calls
   that webpack failed to rewrite (the ESM-typed `.docusaurus/registry.js`
   bypasses the CommonJS parser plugin).
2. The server bundle externalizes theme CSS imports (infima + theme
   stylesheets) instead of stripping them, so Node tries to interpret
   CSS as JavaScript at SSG time.
3. Aliased `@theme/...` imports inside externalized
   `@docusaurus/theme-classic` source files are not re-resolved at SSG
   time, so Node throws `ERR_MODULE_NOT_FOUND`.

We ship a partial mitigation in `patches/@docusaurus__core@3.10.1.patch`
(applied via `pnpm.patchedDependencies`) that:

- Polyfills `require.resolveWeak` on the SSG-injected `require()`.
- Stubs CSS requires at SSG time.

The remaining theme-alias resolution issue is tracked for follow-up;
in the meantime the production build runs in CI via the official
Docusaurus Docker image (see `.github/workflows/docs-ci.yml`) and on
Cloudflare Pages directly. Typecheck, lint, and tests all pass locally
and in CI.

If you need a local preview, use `pnpm dev` (no SSG step) or run the
build inside the upstream `node:22-bookworm` container.

## CI

`.github/workflows/docs-ci.yml` runs on every PR touching `apps/docs/`:

1. `pnpm install --frozen-lockfile`
2. `pnpm typecheck`
3. `pnpm lint`
4. `pnpm test`
5. `pnpm build`
6. Lighthouse-CI on the built site (Performance ≥ 95, A11y = 100,
   Best Practices ≥ 95, SEO ≥ 90)
7. `@axe-core/cli` on the built site
8. `lychee` broken-link check against `build/`
9. `vale` prose lint (Microsoft Writing Style Guide)

## Layout

```
apps/docs/
├── docusaurus.config.ts      # site config, navbar, footer, i18n, Algolia
├── sidebars.ts               # Diátaxis 4-quadrant structure
├── docs/                     # MDX content
│   ├── index.mdx             # landing (3-card layout)
│   ├── tutorial/index.mdx    # Diátaxis section header
│   ├── how-to/index.mdx
│   ├── reference/index.mdx
│   └── explanation/architecture.mdx
├── i18n/
│   ├── en-US/                # default locale
│   ├── pt-BR/                # LGPD primary
│   └── es-419/               # LATAM Spanish
├── src/css/custom.css        # theme overrides + utility primitives
├── static/                   # favicon, og-image, robots.txt, CNAME
├── tests/                    # vitest structural assertions
├── styles/Microsoft/         # Vale rules (populated by `vale sync`)
├── .vale.ini
├── lychee.toml
└── README.md
```
