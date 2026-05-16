---
title: "DEBT-015-BUILD wave-23 closure attempt (root-cause narrowed)"
date: 2026-05-16
wave: 23
branch: wt/r-prep-debt-015-build-final
status: PARTIAL
debts_touched:
  - DEBT-015-BUILD
supersedes:
  - 2026-05-16-debt-015-build-closure.md
---

# DEBT-015-BUILD wave-23 closure attempt — root cause narrowed

## Scope

Wave-22 (`2026-05-16-debt-015-build-closure.md`) closed blockers (a)
draft-front-matter, (b) MDX cross-link normalization, and (c) the
theme-alias / externalised-clientModule patch. It left DEBT-015-BUILD
**PARTIAL** with one new same-class residual: at SSG time Node executes
literal `require("@site/docs/*.mdx")` and
`require("@generated/docusaurus-plugin-content-docs/default/p/*.json")`
strings emitted into the server bundle, throwing `MODULE_NOT_FOUND`
site-wide. The wave-22 closure proposed two paths:

- **(A)** Docs webpack-config override (force-inline `@site/*` +
  `@generated/*` aliases in the server bundle).
- **(B)** Invasive `ssgRequire` patch translating
  `@site/docs/foo.mdx` → compiled output under
  `.docusaurus/docusaurus-plugin-content-docs/default/`.

Wave-23 was budgeted 50 minutes to pick the minimal-blast-radius path
and ship the green build. We attempted **path (A)** and produced a
**sharper root-cause diagnosis** that invalidates the two-paths framing
from wave-22 and narrows the next attempt.

## What was attempted (path A)

We added a `configureWebpack`-hook plugin to `docusaurus.config.ts` that,
when `isServer === true`, asserts three reinforcing knobs:

1. `output.asyncChunks: false` — disables async-chunk creation for
   `import()` calls; the imported module's body must be inlined.
2. `module.parser.javascript.dynamicImportMode: 'eager'` — parser-level
   directive to expand every `import(spec)` into
   `Promise.resolve(require(spec))` with `spec` resolved at compile
   time.
3. `optimization.splitChunks: false` — reassert Docusaurus's own
   server-config default for defence in depth.

We also tried a more aggressive variant (`externals: []` +
`externalsPresets: { node: false }`), which broke `node:`-scheme
built-in resolution (predictable — that flag governs the node-built-ins
externalisation preset, not the user-alias path) and was reverted.

We rebuilt with `rm -rf .docusaurus build && pnpm build` to ensure no
cache reuse. **The literal-require count was unchanged** at 113
`@site/*` + 77 `@generated/*` literal `require()` calls in
`apps/docs/build/__server/server.bundle.js`. The build still fails
identically at SSG-time on the first
`require("@site/docs/.../foo.mdx")` invocation.

## Root cause (refined narrative)

The wave-22 audit described the problem as "webpack's server target
externalising things it should bundle". Wave-23's reading of the
emitted module wrapper

```js
/***/ 1030
(__unused_webpack___webpack_module__, __webpack_exports__,
 __webpack_require__) {
  "use strict";
  /* harmony export */ __webpack_require__.d(__webpack_exports__, {
  /* harmony export */   A: () => (__WEBPACK_DEFAULT_EXPORT__)
  /* harmony export */ });
  /* harmony import */ var _..._interopRequireWildcard_js__WEBPACK_IMPORTED_MODULE_0__
    = __webpack_require__(807);
  /* harmony default export */ const __WEBPACK_DEFAULT_EXPORT__ = ({
    "01336fd1": [
      ()=>Promise.resolve().then(
        ()=>(0,_..._interopRequireWildcard_js__WEBPACK_IMPORTED_MODULE_0__.A)(
          require("@site/docs/trust/iso27001.mdx")
        )
      ),
      "@site/docs/trust/iso27001.mdx",
      require.resolveWeak("@site/docs/trust/iso27001.mdx")
    ],
    ...
```

reveals the actual chain:

1. `.docusaurus/registry.js` is authored with **ES2020 dynamic
   `import(spec)`** for each route chunk.
2. `babel-loader` runs `@docusaurus/babel`'s preset which, for the
   server target, configures `@babel/preset-env` with
   `targets: { node: 'current' }` and **the default `modules: 'auto'`**
   — this transforms `() => import(spec)` into
   `() => Promise.resolve().then(() => _interopRequireWildcard(require(spec)))`.
3. The transformed code reaches webpack with a literal CJS
   `require(spec)` **nested inside two arrow functions**.
4. Webpack's static analyzer correctly processes the *outer-module*
   ES-import (the `interopRequireWildcard` helper becomes
   `__webpack_require__(807)`), but **does not rewrite the
   double-nested `require(spec)` call inside the lazy thunk** — that
   require is left as a literal Node-level CJS require, NOT a
   `__webpack_require__`. At runtime in the SSG harness, Node's CJS
   resolver receives the bare specifier `@site/docs/trust/iso27001.mdx`
   and has no idea what `@site` is (the webpack alias map exists only
   inside the compilation, not at runtime).

This is **not** a webpack-config issue. No combination of webpack
`output.asyncChunks` / `module.parser.javascript.dynamicImportMode` /
`optimization.splitChunks` / `resolve.alias` / `externals` knobs can
fix it — by the time webpack sees the code, the `import()` has already
been rewritten to `require()` by babel, and the resulting nested CJS
require sits in an arrow-function expression that webpack's CJS
require-handling does not deeply rewrite.

## Why path (B) is also wrong as originally framed

Wave-22 proposed translating `@site/docs/foo.mdx` →
`.docusaurus/docusaurus-plugin-content-docs/default/<compiled-output>`
inside `ssgRequireFunction`. **There is no on-disk compiled output for
MDX in `.docusaurus/`** — only metadata JSON
(`site-docs-<route-id>-mdx-<hash>.json`, which contains
`{ id, title, description, frontMatter, ... }` but no React component
body). The compiled MDX React components live exclusively in the
**client bundle** at `build/assets/js/<chunkId>.<hash>.js` in
`globalThis.webpackChunk_corelink_docs.push(...)` format — these are
client-format webpack chunks that are not directly Node-loadable
without faking a `webpackChunk` collector and running the chunk in a
sandbox.

`@generated/...` paths *do* exist on disk (under
`.docusaurus/docusaurus-plugin-content-docs/default/p/*.json`), so 77
of the 190 literal requires would resolve trivially under a path-B
patch. The other 113 (`@site/docs/*.mdx`) would not.

## The actual fix shape (narrowed)

Three viable paths remain, in order of preference:

1. **Patch `@docusaurus/babel/lib/preset.js`** to set
   `modules: false` for the server preset-env config (matching the
   client preset-env config which already does this) — let webpack
   handle the `import()` transformation natively, which it does
   correctly (the literal `import("@site/foo")` would be statically
   analyzed, alias-resolved, and inlined under `splitChunks: false +
   asyncChunks: false`). New `patches/@docusaurus__babel@3.10.1.patch`
   added via `pnpm patch`. Estimated 1-2 hours including verification
   on Node 22 + Node 20 dev surfaces.
2. **Custom babel plugin in `apps/docs/babel.config.js`** that runs
   before preset-env on the server preset and rewrites the nested
   `require("@site/X")` back into a top-level `import "@site/X"`
   statement (or just inlines the resolved path). Estimated 2-3 hours
   (plugin authoring + AST testing).
3. **Sandbox client-chunk eval in `ssgRequireFunction`** — read
   `build/assets/js/<chunkId>.<hash>.js` for `@site/*.mdx` requires,
   eval it in a context with a faked `globalThis.webpackChunk_*` array,
   extract the module from the push payload, return it. Plus
   trivial-translate `@generated/*.json` to disk path. Estimated 3-5
   hours (worth attempting because it's contained to a single file
   patch and doesn't require shipping a babel-preset fork). Higher
   maintenance risk.

The wave-22 path (B) framing is replaced by path (3) above; path (1)
is the new recommended primary attempt because it removes the bug at
its root rather than working around symptoms.

## What landed in wave-23

- This audit doc with the refined root-cause narrative.
- Inline addendum on `2026-05-15-debt-register.md` for DEBT-015 row
  updated with wave-23 ETA + narrowed fix shape.
- `apps/docs/docusaurus.config.ts` reverted to the wave-22 baseline
  (configureWebpack plugin removed) because no webpack-config override
  shifts the symptom — keeping the dead-end code as defence-in-depth
  would mis-document the diagnosis.

## Closure verdict

DEBT-015-BUILD closes **PARTIAL** for the second consecutive wave,
with the failure now diagnosed precisely. The remaining work is **not
50-minute-shaped**; it requires either an upstream `@docusaurus/babel`
patch (preferred, 1-2 h) or a custom babel-plugin in the docs app
(2-3 h) or a non-trivial `ssgRequire` patch that sandbox-evals the
client chunks (3-5 h).

## Spec validators

- `python3 scripts/validate_specs.py` → green (443 schema, 9 YAML-only,
  452 total).
- `python3 scripts/validate_references.py` → green (no dangling refs).

## Docs-side gates (unchanged from wave-21/22)

- `pnpm typecheck` (apps/docs) → green.
- `pnpm lint` (apps/docs) → green.
- `pnpm build` (apps/docs) → red, with the same residual as wave-22
  (113 `@site/*` + 77 `@generated/*` literal requires from
  `.docusaurus/registry.js` reaching SSG runtime). The build progresses
  identically far as the wave-22 baseline; no regression introduced.

## Files touched

- `specs/_audits/2026-05-16-debt-015-build-final.md` (this file)
- `specs/_audits/2026-05-15-debt-register.md` (DEBT-015-BUILD inline
  addendum updated with wave-23 diagnosis + new ETA)

## ETA for follow-on

T+14d (2026-05-30) **unchanged** — the diagnosis-narrowing in this
wave shortens the next attempt's actual time-on-task by ~50 % (no more
chasing webpack knobs that cannot work), so the same date holds. Next
dispatch:

- Try path (1) first: `pnpm patch @docusaurus/babel@3.10.1` and set
  `modules: false` in the server preset-env clause.
- If that breaks downstream targeting, fall back to path (2): custom
  babel plugin in `apps/docs/babel.config.js`.
- Path (3) only if (1) and (2) both fail — it's a workaround, not a
  fix.

## DCO

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
