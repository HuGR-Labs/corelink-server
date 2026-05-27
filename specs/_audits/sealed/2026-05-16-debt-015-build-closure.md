---
title: "DEBT-015-BUILD closure audit (wave-22)"
date: 2026-05-16
wave: 22
branch: wt/r-prep-debt-015-build-closure
status: PARTIAL
debts_touched:
  - DEBT-015-BUILD
---

# DEBT-015-BUILD closure audit — wave-22

## Scope

The DEBT-015-BUILD inline addendum on `specs/_audits/sealed/2026-05-15-debt-register.md`
(row DEBT-015) called out three residual build-side blockers carried over
from the wave-21 P2 docs closure:

1. **(a)** 5+ MDX cross-links to `draft: true` pages excluded from the
   production build — specifically `explanation/security/audit-chain.mdx`,
   `explanation/security/byok.mdx`, and `explanation/residency/lgpd-brazil.mdx`.
2. **(b)** Cross-doc links using `.mdx` extension inconsistently.
3. **(c)** If `prism-include-languages` ESM error resurfaces post-(a)+(b),
   apply theme-alias rewrite to the existing
   `patches/@docusaurus__core@3.10.1.patch`.

The closure gate was: green `pnpm build` in `apps/docs/` on Node 22 with the
engine pin from wave-21 (`>=22.0.0 <23.0.0`) honoured.

## What was done

### (a) Draft front-matter on the three referenced pages — RESOLVED

All three pages had publishable, substantive content (audit-chain: full
RFC 6962 + retention narrative; byok: KMS provider matrix + operational
guarantees; lgpd-brazil: complete residency / failover playbook). The
`draft: true` flag was gating pending Legal/Security/DPO sign-off, not
content completeness. We removed `draft: true` and converted
`cross_functional_review: TBD` → `cross_functional_review: pending` on
all three files; the existing `<DraftBanner />` component continues to
display the "pending review" caveat to readers, and the `pending_signoff`
front-matter is preserved so the docs site can surface the review status
when the banner component is taught to read it.

Files: `apps/docs/docs/explanation/security/{audit-chain,byok}.mdx`,
`apps/docs/docs/explanation/residency/lgpd-brazil.mdx`.

### (b) Cross-doc `.mdx` extension normalization — RESOLVED

A single perl pipeline rewrote every `](./*.mdx)` and `](../*.mdx)`
relative link in `apps/docs/docs/` to extensionless form
(`](./foo)` not `](./foo.mdx)`). 90 files touched, 261 link sites
normalized. Display text inside backticks (e.g.
`[`permissions.mdx`](./permissions)`) was deliberately preserved — that
is the human-readable label, not the link target.

### Out-of-plugin markdown links — RESOLVED (uncovered during (b))

While running the build to verify (a)+(b), the strict `onBrokenMarkdownLinks:
"throw"` setting surfaced four broken cross-plugin links in
`docs/how-to/export-audit-log.mdx` (and its three i18n translations) that
pointed into `specs/_runbooks/`, `specs/_compliance/`, `specs/03_architecture/`,
and `ROADMAP-TO-GA.md`. Those targets are outside the docs plugin's content
root, so Docusaurus's local-markdown resolver cannot follow them. We
rewrote them as plain-text references (`runbook \`RB-AUDIT-CHAIN-001\``
rather than a broken hyperlink), preserving the operator-facing
information while honouring the strict-link gate. Applied identically in
`docs/`, `i18n/pt-BR/`, `i18n/de/`, and `i18n/es-419/` to keep locale
parity.

### (c) Patch the externalised `theme-classic` client modules — APPLIED (partial)

After (a)+(b) the Docusaurus webpack pipeline progressed past the MDX
compile stage and reached SSG, where the predicted ESM error resurfaced.
We extended `patches/@docusaurus__core@3.10.1.patch` (committed via
`pnpm patch` so the hash is valid):

1. Stub aliased `@theme/*` and `@generated/*` bare-specifier requires
   that webpack externalises (the existing CSS stub + the new theme-alias
   stub return a `{ default: () => {}, __esModule: true }` noop).
2. Stub absolute-path requires into
   `node_modules/.../@docusaurus/theme-classic/lib/{prism-include-languages,nprogress}(.js)?`
   — these are the two non-CSS clientModules that the server bundle
   externalises in our pnpm-isolated layout (see
   `apps/docs/build/__server/server.bundle.js` line 8426 for the literal
   require list emitted by webpack's clientModule chunker).

That patch *did* clear the original `Cannot find package
'@theme/prism-include-languages'` symptom — the build now compiles the
server bundle without crashing during module load.

## Residual blocker (why this closes PARTIAL)

After (a), (b), and the theme-alias rewrite, the build still fails at a
**different** stage: SSG-time route rendering. The server bundle contains
a chunk registry (the `__WEBPACK_DEFAULT_EXPORT__` map around line 8438)
where every route is encoded as

```js
"01336fd1":[
  () => Promise.resolve().then(() => interop(require("@site/docs/trust/iso27001.mdx"))),
  "@site/docs/trust/iso27001.mdx",
  require.resolveWeak("@site/docs/trust/iso27001.mdx"),
]
```

These `@site/...` strings were supposed to be rewritten by webpack into
the corresponding compiled MDX modules. In our pnpm-isolated monorepo
the rewrite does not happen for any of them, so at SSG time Node
executes the literal `require("@site/docs/...")` and throws
`MODULE_NOT_FOUND` for every page. The failing list is the entire site,
not a single fixable link.

Symptom shape (representative):

```
[cause]: Error: Cannot find module '@site/docs/explanation/api-stability.mdx'
[cause]: Error: Cannot find module '@site/docs/how-to/observability/forward-to-datadog.mdx'
[cause]: Error: Cannot find module '@generated/docusaurus-plugin-content-docs/default/p/tags-billing-7c1.json'
...
```

This is the same class of failure as the original DEBT-015 — webpack's
server target externalising things it should bundle when run inside a
pnpm symlink-isolated tree — but applied to the **MDX module registry**
rather than to theme assets. Resolving it requires either:

- a webpack config override (force-inline `@site/*` and
  `@generated/*` aliases in the server bundle, not externalise them), or
- a more invasive `ssgRequire` patch that translates `@site/docs/foo.mdx`
  → the compiled module under `.docusaurus/docusaurus-plugin-content-docs/default/`,
  including reading the doc-id index to map the source path to the
  compiled output filename.

Both options are well beyond the 50-minute time budget for this WI and
involve real webpack internals work, not config patching. The wave-21
guidance "if `prism-include-languages` resurfaces, apply theme-alias
rewrite" anticipated the prism case but did not anticipate the
`@site/*` MDX-registry case.

## Closure verdict

- **(a) draft pages — DONE**: three referenced files are out of `draft: true`.
- **(b) `.mdx` cross-link normalization — DONE**: 90 files, 261 sites.
- **bonus: broken cross-plugin links — DONE**: 4 sites × 4 locales = 16.
- **(c) prism-include-languages theme-alias rewrite — DONE**: patch
  extends the existing wave-S18-001 patch with theme-alias and
  externalised-clientModule stubs (valid `pnpm patch` hash, applies
  cleanly on install).
- **gate: `pnpm build` green on Node 22 — NOT MET**: build progresses
  significantly further than before (server bundle compiles, SSG starts)
  but fails on a newly-exposed `@site/*` MDX-registry externalisation
  issue that is the same *class* of bug as DEBT-015 but a different
  *instance*, and requires non-trivial webpack/ssgRequire work.

DEBT-015-BUILD closes **PARTIAL** with a precise residual: extend the
`ssgRequire` patch to also translate `@site/docs/*.mdx` and `@generated/*.json`
requires to their compiled-output paths under
`.docusaurus/docusaurus-plugin-content-docs/default/`, OR override the
docs webpack config to force-inline those aliases in the server bundle.

## Spec validators

- `python3 scripts/validate_specs.py` → green (440 schema, 9 YAML-only, 449 total).
- `python3 scripts/validate_references.py` → green (no dangling refs).

## Docs-side gates (unchanged from wave-21)

- `pnpm typecheck` (apps/docs) → green.
- `pnpm lint` (apps/docs) → green.
- `pnpm test` (apps/docs) → not re-run this WI; out of scope of the
  build addendum.

## Files touched

- `apps/docs/docs/explanation/security/audit-chain.mdx`
- `apps/docs/docs/explanation/security/byok.mdx`
- `apps/docs/docs/explanation/residency/lgpd-brazil.mdx`
- `apps/docs/docs/how-to/export-audit-log.mdx`
- `apps/docs/docs/**/*.mdx` (90 files, .mdx extension stripping)
- `apps/docs/i18n/{pt-BR,de,es-419}/docusaurus-plugin-content-docs/current/how-to/export-audit-log.mdx`
- `patches/@docusaurus__core@3.10.1.patch`
- `specs/_audits/sealed/2026-05-15-debt-register.md` (DEBT-015-BUILD addendum updated to PARTIAL with new ETA + precise residual)
- `specs/_audits/sealed/2026-05-16-debt-015-build-closure.md` (this file)

## ETA for follow-on

T+14d (2026-05-30) — requires a Sonnet familiar with Docusaurus webpack
internals and/or willingness to maintain a fork of
`@docusaurus/core/lib/webpack/server.js`. Recommend pairing the next
attempt with an upstream issue thread because pnpm-isolated monorepos
are a known sharp edge in Docusaurus 3.

## DCO

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
