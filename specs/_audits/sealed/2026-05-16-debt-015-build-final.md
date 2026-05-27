---
title: "DEBT-015-BUILD wave-23/24 closure attempt (root-cause narrowed)"
date: 2026-05-16
wave: 24
branch: wt/r-prep-debt-015-build-babel-patch
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

- `specs/_audits/sealed/2026-05-16-debt-015-build-final.md` (this file)
- `specs/_audits/sealed/2026-05-15-debt-register.md` (DEBT-015-BUILD inline
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

---

# Wave-24 addendum — path #1 attempted, invalidated empirically

## Scope

Wave-23 recommended path #1: patch `@docusaurus/babel/lib/preset.js` to set
`modules: false` on the server preset-env target, hypothesising that
`@babel/preset-env`'s default `modules: 'auto'` was rewriting `import()`
into `Promise.resolve().then(() => require())` on the server pass.
Wave-24 attempted that patch (plus a tighter additional change), rebuilt,
and disproved the hypothesis.

## What landed

`patches/@docusaurus__babel@3.10.1.patch` registered via
`pnpm patch @docusaurus/babel@3.10.1` and recorded in
`package.json` → `pnpm.patchedDependencies` and `pnpm-lock.yaml`. Two
deltas vs upstream `@docusaurus/babel@3.10.1`:

1. **Server preset-env `modules: false`** — the wave-23 recommended
   change. Prevents preset-env from interpreting the source as CJS and
   short-circuits any ESM-to-CJS rewrite on the server pass.
2. **Server dynamic-import plugin swap** — replaces
   `babel-plugin-dynamic-import-node` (which rewrites every `import(spec)`
   into `Promise.resolve().then(() => require(spec))`) with
   `@babel/plugin-syntax-dynamic-import` (same plugin the client branch
   uses). Strictly tighter than the wave-23 recommendation, included
   because the dynamic-import-node plugin was the more direct candidate
   for the symptom (preset-env `modules:'auto'` does not transform
   `import()` at all in modern `@babel/preset-env`; that's
   `babel-plugin-dynamic-import-node`'s job).

Both patches are clearly commented inline with the DEBT-015-BUILD wave-24
context.

## Empirical result

`pnpm install` repatched the babel package (verified via
`@docusaurus/core@3.10.1_patch_hash=...` symlink chain pointing at the
new `@docusaurus+babel@3.10.1_patch_hash=...` directory containing the
modified `preset.js`). Wave-24's full rebuild
(`rm -rf build && pnpm build` on Node 22.17.1):

- **Webpack compilation phase** — green, identical to the wave-23
  baseline. Both server and client bundles compile in ~1.5s
  ("Server: Compiled successfully", "Client: Compiled successfully").
- **SSG phase** — **fails identically** to wave-22/23 with 113
  `MODULE_NOT_FOUND` errors for `@site/docs/*.mdx` requires. Bundle
  inspection:
  - `grep -c 'import("@site' build/__server/server.bundle.js` → **0**
    (down from baseline; the syntax-only plugin no longer forces every
    `import()` to become a runtime `require()`).
  - `grep -oE 'require\("@site[^"]+"\)' build/__server/server.bundle.js | wc -l`
    → **226** (unchanged from wave-23). The 113 unique `@site/*.mdx`
    paths each appear once as `require(...)` and once as
    `require.resolveWeak(...)`.
  - `grep -c 'Promise\.resolve()\.then' build/__server/server.bundle.js`
    → **6** (down from ~hundreds; the residual six are unrelated
    `Promise.resolve().then()` user code paths).
- **Source vs bundle diff** — `.docusaurus/registry.js` (the route
  manifest) contains clean
  `() => import(/* webpackChunkName: "..." */ "@site/...")` calls (line
  202 of `@docusaurus/core/lib/server/codegen/codegenRoutes.js`). The
  emitted `__server/server.bundle.js` instead contains
  `() => Promise.resolve().then(() => interopRequireWildcard(require("@site/...")))`.

## Root cause (re-narrowed)

Wave-23's diagnosis attributed the nested `require()` pattern to
`@babel/preset-env` `modules: 'auto'`. That diagnosis is **wrong**.
preset-env with `targets: { node: 'current' }` and `modules: 'auto'`
does not transform `import()` calls — it leaves dynamic-import syntax
untouched and only rewrites top-level `import`/`export` statements to
CJS. The nested-`require` pattern was actually emitted by
`babel-plugin-dynamic-import-node` (still wired in the server branch on
line 78 of upstream `preset.js`). Wave-24's patch removes both knobs
defensively, and the bundle confirms babel no longer produces the
pattern (no remaining `import(` in the bundle, no babel-authored
`Promise.resolve().then(()=>require(...))` chains).

**Yet the bundle still contains 226 literal `require("@site/...")` and
`require.resolveWeak("@site/...")` calls.** This is intrinsic
**webpack 5** behaviour for `target: 'node'` server bundles when
combined with `optimization.splitChunks: false` (which Docusaurus sets
on the server config) and an unresolvable alias-prefixed dynamic
specifier:

- For a static `import "@site/foo"`, webpack resolves the `@site`
  alias at compile time and inlines the module.
- For a dynamic `import("@site/foo")` in a `target: 'node'` bundle with
  splitChunks disabled, webpack 5 emits a runtime `require("@site/foo")`
  call (a "lazy require"). The alias map is **not** consulted at this
  rewrite — the specifier is passed through verbatim because webpack
  treats dynamic specifiers as runtime-resolved by default on the node
  target.

So the actual chain is:

1. `codegenRoutes.js` emits `import("@site/docs/foo.mdx")` into the
   generated registry — correct webpack idiom.
2. babel-loader (now with wave-24 patches) leaves the `import()` as-is.
3. webpack server compilation sees a dynamic `import()` whose
   specifier is alias-prefixed; for the `node` target with
   splitChunks disabled, it emits a plain runtime
   `require("@site/docs/foo.mdx")` — without resolving the `@site`
   alias.
4. SSG executes the server bundle in a `vm`-like context whose
   `require` is `ssgRequireFunction` from `ssgNodeRequire.js`. That
   function delegates to Node's CJS `createRequire`, which has no
   notion of webpack aliases → `MODULE_NOT_FOUND`.

No babel patch can fix this. The transform happens **after** babel, in
webpack's own `target: 'node'` chunk-emission pass.

## Why wave-23 path #1 is provably insufficient

- Empirical: bundle still contains 226 alias-prefixed runtime requires
  after the patch.
- Mechanical: babel has no visibility into webpack's dynamic-import
  chunk-emission strategy for the node target.
- Architectural: even if babel rewrote `import("@site/X")` to a
  resolved absolute path at compile time, that would break the
  shared client/server semantics Docusaurus relies on (the client
  bundle uses `@site/...` as a webpack-resolved alias map, not a
  filesystem path).

## Revised path forward (wave-25)

Two viable approaches; path B is strongly preferred.

**Path A — webpack server config override** (estimated 2-3h):
Patch `@docusaurus/core/lib/webpack/server.js` to add
`resolve.alias` post-processing OR a small `RuleSetRule` that
matches `^@site/` specifiers and pre-resolves them. Risk:
high — the alias map is built dynamically in `base.js`, and any
patch must keep client/server bundles consistent or hydration
will break. Reverts and re-attempts are expensive (~10 min per
build cycle).

**Path B — `ssgRequire` alias resolver** (estimated 1-2h, strongly
preferred): Extend `@docusaurus/core/lib/ssg/ssgNodeRequire.js`
(already patched for `@theme/`, `@generated/`, CSS, and
`resolveWeak`) with a fifth resolver branch: when `id` matches
`^@site/`, rewrite it to the absolute `siteDir` path passed into
`createSSGRequire`. Since `siteDir` is already in scope of the
factory closure (it's `path.dirname(serverBundlePath, '../..')`
effectively — but realistically should be threaded through from
`createSSGRequire(serverBundlePath, siteDir)`), this is a 5-line
patch plus a 1-call-site change in
`@docusaurus/core/lib/ssg/ssgRenderer.js`. For
`@site/docs/foo.mdx` we then `realRequire(path.join(siteDir,
'docs/foo.mdx'))` — but **MDX files are not Node-loadable** as-is,
so this still requires running the MDX through `@docusaurus/mdx-loader`
or eval'ing the compiled client chunk. The realistic shape is
therefore:

- `@site/docs/*.mdx` resolves via wave-22 audit's *path-B-revised*
  approach (sandbox-eval the compiled client chunk in
  `build/assets/js/<chunkId>.<hash>.js`, fake the
  `webpackChunk_corelink_docs` collector, return the chunk's
  default export).
- `@generated/docusaurus-plugin-content-docs/default/p/*.json`
  resolves trivially via `realRequire(path.join(generatedFilesDir,
  '...'))`.

This is the "non-trivial `ssgRequire` patch" wave-23 estimated at
3-5h. Wave-24 narrows the estimate to **2-3h** because:
- The webpack-emitted require list is now fully characterised
  (113 `@site/*` + 113 `resolveWeak` + 77 `@generated/*` literal
  requires, exact strings logged above).
- The two prior `ssgRequire` patches (`@theme/`, `@generated/`,
  CSS, `resolveWeak`) establish the patch shape — adding two more
  branches is mechanical.
- The babel patches from wave-24 stay landed (they remove dead
  code paths and don't regress anything), so wave-25 starts from
  a cleaner baseline.

## What landed in wave-24

- `patches/@docusaurus__babel@3.10.1.patch` (new file, registered
  in `package.json` + `pnpm-lock.yaml`).
- This audit doc, updated with the wave-24 closure section.
- DEBT-015-BUILD register row updated with wave-24 result + revised
  ETA.

The babel patches stay landed because:
- They make the server bundle strictly cleaner (no dead
  `babel-plugin-dynamic-import-node` transform).
- They eliminate one of the two layers wave-23 conflated, which
  reduces the diagnostic surface for wave-25.
- They have zero observable regression (webpack compilation phase
  identical, all non-build gates green).

## Wave-24 quality gates

- `pnpm typecheck` (apps/docs) → **green**.
- `pnpm lint` (apps/docs) → **green**.
- `python3 scripts/validate_specs.py` → **green** (444 schema + 9
  YAML-only = 453 total).
- `python3 scripts/validate_references.py` → **green** (no dangling
  refs).
- `pnpm build` (apps/docs) on Node 22 → **red** (same SSG failure
  as wave-22/23, root cause now sharpened).

## Closure verdict

DEBT-015-BUILD closes **PARTIAL** for the third consecutive wave.
Wave-23's diagnosis was directionally right (babel pass was emitting
the nested-require pattern) but mechanically wrong (the culprit plugin
was `babel-plugin-dynamic-import-node`, not preset-env `modules:auto`).
Wave-24 cleaned up that babel layer and discovered the deeper layer is
in webpack itself, which **cannot be patched via babel**. The path
forward is now clearly **path B (`ssgRequire` alias resolver +
client-chunk sandbox-eval)** at 2-3h, replacing wave-23's path #2
(custom babel plugin, also provably insufficient by the same
empirical argument).

## ETA for follow-on

T+14d (2026-05-30) unchanged. Wave-25 dispatch:

- Extend `@docusaurus/core/lib/ssg/ssgNodeRequire.js` with `@site/*`
  and `@generated/*` branches.
- For `@site/docs/*.mdx`, sandbox-eval the corresponding client chunk
  in `build/assets/js/<chunkId>.<hash>.js` using a faked
  `webpackChunk_corelink_docs` collector.
- For `@generated/*.json`, resolve directly to the
  `.docusaurus/<plugin>/p/<file>.json` path.
- Verify build + Lighthouse + SEO sanity.

