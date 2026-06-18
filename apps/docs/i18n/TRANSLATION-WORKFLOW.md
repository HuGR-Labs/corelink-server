# CoreLink translation workflow

Formal localization process for CoreLink docs (`apps/docs`) and the admin UI
(`apps/admin-ui`). Owners: Docs team (`@docs-tl`) + i18n vendor (TBD).

## 1. Canonical locales

| Locale   | Market / rationale                              | Tier   | SLA per EN change |
| -------- | ----------------------------------------------- | ------ | ----------------- |
| `en-US`  | Source of truth (native; authored by HuGR)      | source | n/a               |
| `pt-BR`  | LGPD / Brazil (R-S18-12)                        | T1     | ≤ 14 d            |
| `es-419` | LATAM (R-S18-12)                                | T1     | ≤ 14 d            |
| `de`     | DACH / EU enterprise (R-prep i18n-de — GA buyers) | T1   | ≤ 14 d            |

Source-of-truth: spec `S-18 §5.6 R-S18-12` (Tier-1 locales) + this workflow's
R-prep i18n-de extension. `en-US` is the authoritative version: when EN
changes, the other locales follow within the SLA above.

## 2. Pipeline

```
              author EN .mdx                              translator vendor
                    │                                            │
                    ▼                                            ▼
        ┌──────────────────────┐    XLIFF 2.1 (per locale)   ┌────────┐
        │  apps/docs/docs/**   │  ────────────────────────▶  │ vendor │
        │  apps/admin-ui/src/  │                             │  pool  │
        │       i18n/**        │  ◀──── translated XLIFF ─── │        │
        └──────────────────────┘                             └────────┘
                    │
                    ▼
              import-xliff                                 stale check
                    │                                            ▲
                    ▼                                            │
              mt-stub-seed (if any gaps)                         │
                    │                                            │
                    ▼                                            │
              i18n-coverage gate (CI) ──── i18n-stale CI ────────┘
                    │
                    ▼
              merge to main → deploy
```

### 2.1 String extraction

Source strings live in two surfaces:

- `apps/docs/docs/**/*.{md,mdx}` — long-form documentation (Diátaxis: tutorial,
  how-to, reference, explanation, pricing, trust).
- `apps/admin-ui/src/**` — UI labels (`src/i18n/locales/<locale>.json`,
  `src/i18n/messages-<locale>.json`) and content markdown
  (`src/content/<base>.<locale>.md`).

For docs, extraction is implicit: every `.mdx` page under `docs/` requires a
counterpart under `i18n/<locale>/docusaurus-plugin-content-docs/current/<rel>`.
Coverage report: `pnpm tsx scripts/i18n-coverage-report.ts`.

For admin-ui UI strings, extraction is implicit: every key in
`src/i18n/locales/en.json` (or `messages-en.json`) must exist in the per-
locale sibling. The next-intl strict mode flags missing keys at build time.

### 2.2 XLIFF export

```bash
cd apps/docs
pnpm i18n:export                          # all locales, tarball-per-locale
pnpm tsx scripts/export-xliff.ts --locale de   # single locale
```

XLIFF 2.1 bundles include source/target segments + TMX cross-reference. The
exporter snapshots EN at the export time so the translator works against a
stable source.

### 2.3 Translator / vendor

Three accepted vendor channels:

1. **In-house translator** (preferred for legal-sensitive surfaces: DPA,
   ToS, privacy notice). Requires CoreLink-cleared linguist with NDA.
2. **Vendor pool** (per `apps/docs/i18n/VENDOR-RFP.md`). Cost model: ~$0.10/word
   for technical docs, ~$0.15/word for legal.
3. **MT-stub** (`pnpm tsx scripts/mt-stub-seed.ts`) — fallback when (1) and
   (2) cannot meet the 14-day SLA. MT-stub pages carry a visible `MT:` banner
   and the `<!-- i18n:MT ... -->` marker; the canonical EN remains the source
   of truth until a human pass removes the marker.

#### Tier-1 locale acceptance gate

A locale graduates from MT-stub → native-translated when:

- `i18n-coverage.ts --threshold 0.95` passes (every page has either
  `translated` or `mt-stub` status; no `missing`, no `todo`).
- `translation-quality-check.ts --strict` passes (no banned terms, no MT
  telltales like `du`/`dir` in `de`, no Castilian forms in `es-419`).
- Native-speaker reviewer signs off in the PR.

### 2.4 XLIFF import

```bash
cd apps/docs
pnpm i18n:import --in dist/xliff-de        # apply translated XLIFF tarball
pnpm tsx scripts/i18n-coverage-report.ts   # regenerate COVERAGE.md
```

After import the per-locale tree is re-populated and the `<!-- i18n:MT -->`
marker is removed. The TMX (`apps/docs/i18n/tmx/`) is updated automatically
during the next export so subsequent rounds benefit from translation memory.

### 2.5 TMX (translation memory)

Translation memory (TMX 1.4) accrues over time at
`apps/docs/i18n/tmx/<locale>.tmx`. Sources of TMX entries:

1. **UI strings** (`code.json`, `navbar.json`, `footer.json`) — seeded by the
   `mt-stub-seed.ts` loader from the matched en→target JSON pairs.
2. **XLIFF round-trip** — every `import-xliff` run appends accepted segments
   to the TMX file via `export-xliff.ts`'s TMX writer.
3. **Manual additions** — terminology team can hand-craft TMX entries for
   legal/regulatory terms (e.g. "data controller" → "Verantwortlicher" in
   `de`).

TMX is consumed by:

- `mt-stub-seed.ts` — substitutes known EN→target segments inline when
  promoting TODO → MT.
- `export-xliff.ts` — embeds TMX cross-reference into the XLIFF bundle so
  vendors see prior translations.

### 2.6 Review & publish

1. PR opens → CI runs `i18n-coverage` (≥ 80 %), `translation-quality-check`,
   typecheck, build.
2. Native-speaker reviewer comments on the PR (required for legal/regulatory
   pages; advisory for technical docs).
3. Merge to `main` → Docusaurus build + deploy to `corelink-docs.humangr.com` with
   per-locale subpaths (`/pt-BR/...`, `/es-419/...`, `/de/...`).

## 3. SLA & stale-content policy

| Trigger                                  | Action                              | Owner       |
| ---------------------------------------- | ----------------------------------- | ----------- |
| EN page changes                          | MT-stub within 24 h, native ≤ 14 d  | docs team   |
| Locale falls < 80 % coverage             | Block sprint close                  | docs-tl     |
| Locale stale > 14 d after EN change      | Weekly CI opens an issue (Tue 10 UTC) | docs-tl   |
| Native-speaker review pending > 30 d     | Escalate to PM; consider vendor pool | PM         |

Stale-check implementation: `apps/docs/scripts/i18n-stale-check.sh`
(invoked by `.github/workflows/i18n-stale.yml`).

## 4. Adding a new locale (post-GA playbook)

This workflow was first exercised end-to-end for `de` (DACH / R-prep i18n-de).
To add a new locale `xx`:

1. `apps/docs/docusaurus.config.ts` — extend `locales` + `localeConfigs`.
2. `apps/docs/scripts/{i18n-coverage,i18n-coverage-report,export-xliff,import-xliff,translation-quality-check,mt-stub-seed}.ts`
   — add `xx` to `LOCALES` (+ `BANNED_BY_LOCALE` for the QC rules).
3. `apps/admin-ui/src/i18n/{LocaleContext,index,messages,request}.ts`,
   `apps/admin-ui/src/lib/{i18n,validators}.ts`,
   `apps/admin-ui/src/components/i18n/LocaleSwitcher.tsx` — extend Locale union
   + `Record<Locale, ...>` constants.
4. Create UI seed files:
   - `apps/docs/i18n/xx/code.json`
   - `apps/docs/i18n/xx/docusaurus-theme-classic/{navbar,footer}.json`
   - `apps/admin-ui/src/i18n/locales/xx.json`
   - `apps/admin-ui/src/i18n/messages-xx.json`
   - `apps/admin-ui/src/content/{privacy-notice,dpa,tos}.xx.md`
5. Bootstrap docs MT-stub tree:
   ```
   pnpm tsx apps/docs/scripts/mt-stub-seed.ts --init --locale xx
   ```
6. Regenerate `apps/docs/i18n/COVERAGE.md`:
   ```
   pnpm tsx apps/docs/scripts/i18n-coverage-report.ts
   ```
7. Update `apps/admin-ui/src/app/[locale]/dsr/page.tsx` `generateStaticParams`.
8. Open PR. CI gates run; native-speaker review (or MT-stub acceptance for
   the bootstrap PR) signs it off.

## 5. References

- `apps/docs/i18n/COVERAGE.md` — current coverage report
- `apps/docs/i18n/STYLE-GUIDE.md` — terminology + tone per locale
- `apps/docs/i18n/VENDOR-RFP.md` — vendor selection playbook
- ROADMAP §4 R-4 — i18n parity at GA
- Spec contract `S-18 §5.6 R-S18-12` — 3 Tier-1 locales canonical
- R-prep i18n-de — `de` joined as fourth Tier-1 locale (DACH/EU enterprise GA)
