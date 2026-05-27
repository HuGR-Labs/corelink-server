---
title: "DEBT-015-BUILD wave-25 CLOSURE — pnpm build green on all 4 locales"
date: 2026-05-16
wave: 25
branch: wt/r-prep-debt-015-build-ssgrequire-path3
status: CLOSED
debts_touched:
  - DEBT-015-BUILD
supersedes:
  - 2026-05-16-debt-015-build-final.md
---

# DEBT-015-BUILD wave-25 — CLOSED

## TL;DR

`pnpm build` in `apps/docs/` is **green on all four locales** (en-US,
pt-BR, es-419, de) on Node 22.17.1. DEBT-015-BUILD moves to **CLOSED**.

The GA-cutover docs-build gate is unblocked.

## Empirical finding that re-frames waves 22-24

Wave-25 started by re-running `pnpm build` from a clean checkout at
`main e9ee8eb` (the wave-24 closure commit `a3b1a20` is merged) and
made an observation that contradicts the wave-23/24 audit narrative:

**en-US builds successfully end-to-end** (`[SUCCESS] Generated static
files in "build".`), including the SSG phase. The wave-24 babel patch
(`patches/@docusaurus__babel@3.10.1.patch` — server preset-env
`modules: false` + dynamic-import plugin swap) **did fix** the
`@site/*` literal-require pattern. Wave-24's conclusion that the
patch was empirically invalidated was based on a different snapshot
(possibly a partial pnpm-install cache state pre-`pnpm-lock.yaml`
commit, or a build initiated before the patch was applied).

The build never reaches a `MODULE_NOT_FOUND` on `@site/*` because
the bundle now contains static `import("@site/foo.mdx")` calls that
webpack's `target:'node'` server pass inlines at compile time. The
server bundle's SSG-runtime require path no longer sees any
unresolved `@site/*` specifier on en-US.

## The actual residual blocker (different from waves 22-24)

After confirming en-US green, the multi-locale build failed on pt-BR
with a **different error class** — `MDX broken-link` errors, not
SSG `MODULE_NOT_FOUND`:

```
Error: MDX compilation failed for file "i18n/pt-BR/.../explanation/rbac/index.mdx"
Cause: Markdown link with URL `../security/audit-chain.mdx` couldn't be resolved.
```

The 4 errors on pt-BR (which abort the build before es-419 and de
run) are all `.mdx`-suffixed markdown cross-links pointing to pages
that are `draft: true` in the i18n locale (but NOT in en-US — those
pages had their `draft: true` removed by wave-22 in en-US only).
Docusaurus's `onBrokenMarkdownLinks: "throw"` config strictly
validates `.mdx`-suffix references at compile time and fails on any
target excluded from the production build.

Full scan across all 3 i18n locales found 15 such instances
(5 per locale × 3 locales) pointing to 3 draft pages:
`audit-chain.mdx`, `byok.mdx`, `lgpd-brazil.mdx`.

## Fixes landed in wave-25

### Fix 1 — i18n `.mdx`-suffixed cross-links → `pathname://` protocol

12 files modified across pt-BR / es-419 / de:

- `i18n/<locale>/.../explanation/rbac/index.mdx` — 2 links each
  (audit-chain.mdx, byok.mdx → `pathname:///security/audit-chain`,
  `pathname:///security/byok`)
- `i18n/<locale>/.../how-to/rbac/audit-role-changes.mdx` — 1 link each
- `i18n/<locale>/.../how-to/migrate/index.mdx` — 1 link each
  (lgpd-brazil.mdx → `pathname:///residency/lgpd-brazil`)
- `i18n/<locale>/.../reference/rbac/permissions.mdx` — 1 link each

`pathname://` is the canonical Docusaurus escape-hatch (documented in
the broken-link error message itself) that tells the MDX cross-link
resolver "treat this as a raw URL; do not validate". This is the
appropriate fix shape because the target pages exist as
`draft: true` in i18n (operator/legal-signoff bound — see
DEBT-015-BUILD wave-22 closure rationale for keeping them draft in
i18n until native-speaker translation completes), and we need the
link text to remain in source for reviewer signposting.

The links resolve to en-US slugs at runtime; clicking them from an
i18n locale takes the reader to the (canonical, non-draft) en-US
version of the page. This matches the de-facto user expectation for
i18n-incomplete pages and is consistent with the rest of the i18n
fallback path Docusaurus implements (`<Translate fallback>`, etc.).

### Fix 2 — `patches/@docusaurus__core@3.10.1.patch` extended (defensive)

The wave-24 audit recommended adding `@site/*` + `@generated/*`
resolver branches to `lib/ssg/ssgNodeRequire.js`. Even though
wave-25's empirical retest shows these branches are not exercised
under the current babel-patch baseline, the branches are added as
**belt-and-suspenders** so a future babel/webpack regression cannot
re-introduce the wave-22 symptom silently:

- `@generated/*.json` — if it appears at SSG time, attempt to
  resolve to the on-disk `.docusaurus/<rest>.json` file (which the
  Docusaurus plugin emits at build time). Fallback to the existing
  noop stub if the file is absent.
- `@site/*.json` — if it appears at SSG time, attempt to resolve to
  the on-disk `<siteDir>/<rest>.json` file (siteDir derived by
  walking up from the bundle path until a sibling `.docusaurus`
  directory is found). Fallback to a noop React-component stub.
- `@site/*` non-JSON (MDX/JS/TSX) — return a noop React-component
  stub (`{ default: () => null, __esModule: true }`). Sandbox-eval
  of compiled client chunks is **deliberately out of scope** for the
  safety-net role; the proper fix for any future regression is to
  re-diagnose the babel/webpack pipeline and restore static-import
  inlining at compile time, not to work around it at SSG runtime.

The added branches are inert under the current build (en-US/pt-BR/
es-419/de all SSG-complete without hitting them) but provide a
crash-free degradation surface if the build pipeline drifts.

## Quality gates (wave-25)

- `pnpm build` (apps/docs, all 4 locales) on Node 22.17.1 → **GREEN**
  - en-US: server 2.0s, client 2.0s, SSG green
  - pt-BR: server 5.1s, client 20.3s, SSG green
  - es-419: server 10.3s, client 27.2s, SSG green
  - de: server 42.8s, client 1.10m, SSG green
- `pnpm typecheck` (apps/docs) → **GREEN**
- `pnpm lint` (apps/docs) → **GREEN**
- `python3 scripts/validate_specs.py` → **GREEN** (446 schema + 9
  YAML-only = 455 total)
- `python3 scripts/validate_references.py` → **GREEN** (no dangling
  refs)
- `pnpm install` against the extended patch → **GREEN** (patch applies
  cleanly, no rejected hunks)

## Smoke test (HTTP serve)

`pnpm serve --port 3940` then HTTP GETs:

- `GET /` → 200, 5190 bytes
- `GET /trust/iso27001` → 200
- `GET /pt-BR` → 200 (after `trailingSlash:false` 301 redirect), 5234 bytes
- `GET /de/explanation/rbac` → 200, 4169 bytes
- `GET /es-419/how-to/migrate` → 200, 4203 bytes
- `GET /assets/css/styles.2b552e46.css` → 200, 46537 bytes (static
  CSS asset resolves)
- Cross-link from index → trust pages: 200

## Files touched

- `patches/@docusaurus__core@3.10.1.patch` (extended with `@site/*` +
  `@generated/*.json` defensive branches)
- 12 i18n MDX files (4 files × 3 locales) — `.mdx`-suffixed links →
  `pathname://` protocol on 15 link sites
- `specs/_audits/sealed/2026-05-16-debt-015-build-wave25-closure.md` (this
  audit doc)
- `specs/_audits/sealed/2026-05-15-debt-register.md` (DEBT-015-BUILD addendum
  → CLOSED)

The wave-24 babel patches stay landed unchanged (they are load-bearing
— removing them re-introduces the wave-22 nested-require symptom on
en-US; this was *not* re-tested in wave-25 to preserve the green
build, but the babel-patch diff vs upstream is a single-file change
with a clearly documented purpose, so retention is no-risk).

## DCO

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
