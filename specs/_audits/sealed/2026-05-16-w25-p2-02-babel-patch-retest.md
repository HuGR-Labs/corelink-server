---
title: "W25-P2-02 — empirical retest of babel-patch load-bearing claim"
date: 2026-05-16
wave: 30+
branch: wt/r-prep-w25-p2-02-babel-patch-retest
status: CLOSED
debts_touched:
  - DEBT-015-BUILD (post-closure follow-up)
provenance:
  - specs/_audits/2026-05-16-wave25-adversarial-review.md  # §P2-02 (lines 108-120)
  - specs/_audits/2026-05-16-debt-015-build-wave25-closure.md
  - specs/_audits/2026-05-16-p2-absorption-sweep-w25-28.md  # W25-P2-02 row
conclusion: OBSOLETE
---

# W25-P2-02 — babel-patch empirical retest

## TL;DR

The wave-24 babel patch (`patches/@docusaurus__babel@3.10.1.patch`) is
**OBSOLETE** post-wave-25. Empirical retest: `pnpm build` is **green on
all four locales** (en-US, pt-BR, es-419, de) on Node 22.17.1 **with
the babel patch removed**. The wave-25 `pathname://` i18n link-flips
plus the defensive `@site/*` and `@generated/*.json` resolver branches
in `patches/@docusaurus__core@3.10.1.patch` (`lib/ssg/ssgNodeRequire.js`)
together fully defuse the wave-22/24 SSG `MODULE_NOT_FOUND` symptom —
the babel-level `modules: false` + `@babel/plugin-syntax-dynamic-import`
swap is no longer load-bearing on the current build pipeline.

Cleanup action: babel patch file deleted; `pnpm.patchedDependencies`
entry removed from root `package.json`; only the `@docusaurus/core`
patch remains. Repo returns to a green-build state.

## 1. Scope + provenance

This audit closes the **P2-DEFER-POST-GA** follow-up flagged in:

- `specs/_audits/2026-05-16-wave25-adversarial-review.md` §P2-02
  (lines 108-120): "DEBT-015-BUILD wave-25 doc asserts babel-patch is
  'load-bearing' without empirical re-test" — fix shape was a single
  `pnpm build` run with babel patches reverted, 5-minute experiment to
  close the load-bearing assumption.
- `specs/_audits/2026-05-16-p2-absorption-sweep-w25-28.md` row
  **W25-P2-02** — disposition `DEFER-POST-GA`, queued for wave-30+
  R-prep.
- `specs/_audits/2026-05-16-debt-015-build-wave25-closure.md` — the
  wave-25 closure that asserted retention "no-risk" without retest.

## 2. Babel-patch file path + pnpm patch flow

**File:** `patches/@docusaurus__babel@3.10.1.patch` (repo root).

**Registration:** Root `package.json` → `pnpm.patchedDependencies`:

```json
"pnpm": {
  "patchedDependencies": {
    "@docusaurus/core@3.10.1": "patches/@docusaurus__core@3.10.1.patch",
    "@docusaurus/babel@3.10.1": "patches/@docusaurus__babel@3.10.1.patch"
  }
}
```

**Disable flow** (per pnpm documentation):

1. Delete the patch file from `patches/`.
2. Remove the corresponding entry from `pnpm.patchedDependencies`.
3. Re-run `pnpm install` (regenerates `pnpm-lock.yaml`; pnpm
   re-materialises the dependency from the upstream tarball without
   applying the patch).

**Patch payload** (vs upstream `@docusaurus/babel@3.10.1`,
`lib/preset.js`): two server-target deltas in `getTransformOptions`:

- (i) Server `@babel/preset-env` config gains `modules: false`,
  matching the client target.
- (ii) Server dynamic-import plugin swapped from
  `babel-plugin-dynamic-import-node` → `@babel/plugin-syntax-dynamic-import`
  (also matching client).

The stated rationale (per the patch comment) was to prevent the server
bundle from emitting `Promise.resolve().then(() => require("@site/..."))`
wrappers around `import("@site/...")` calls, which Node 22's CJS
loader cannot resolve at SSG time.

## 3. Baseline WITH-patch result

**Commit:** repo HEAD `99269ed` (wave-30 merged; parent main at
`0f77f48`).

**Patch state:** both `@docusaurus/core` and `@docusaurus/babel`
patches applied (confirmed via
`node_modules/.pnpm/@docusaurus+babel@3.10.1_patch_hash=…/node_modules/@docusaurus/babel/lib/preset.js`
containing the wave-24 `modules: false` + `@babel/plugin-syntax-dynamic-import`
deltas).

**Command:** `cd apps/docs && rm -rf build .docusaurus && pnpm build`

**Result tail:**

```
[INFO] [en-US] Creating an optimized production build...
[SUCCESS] Generated static files in "build".
[INFO] [pt-BR] Creating an optimized production build...
[SUCCESS] Generated static files in "build/pt-BR".
[INFO] [es-419] Creating an optimized production build...
[SUCCESS] Generated static files in "build/es-419".
[INFO] [de] Creating an optimized production build...
[SUCCESS] Generated static files in "build/de".
[INFO] Use `npm run serve` command to test your build locally.
```

All four locales SSG-green. This is the wave-25-SEAL state.

## 4. WITHOUT-patch result

**Disable steps applied:**

- Removed `"@docusaurus/babel@3.10.1": "patches/@docusaurus__babel@3.10.1.patch"`
  entry from root `package.json`.
- Re-ran `pnpm install` (lockfile updated; upstream
  `@docusaurus/babel@3.10.1` re-materialised from cache without
  patch overlay).
- Verified upstream copy at
  `node_modules/.pnpm/@docusaurus+babel@3.10.1_clean-css*/node_modules/@docusaurus/babel/lib/preset.js`
  is now the resolved one (no `patch_hash=` in the directory name),
  and the server branch matches upstream
  (`isServer ? babel-plugin-dynamic-import-node : @babel/plugin-syntax-dynamic-import`,
  no `modules: false` on the server preset-env config).

**Command:** `cd apps/docs && rm -rf build .docusaurus && pnpm build`

**Result tail (full output reproduced):**

```
[INFO] Website will be built for all these locales:
- en-US
- pt-BR
- es-419
- de
[INFO] [en-US] Creating an optimized production build...
[webpackbar] ✔ Server: Compiled successfully in 8.21s
[webpackbar] ✔ Client: Compiled successfully in 8.99s
[SUCCESS] Generated static files in "build".
[INFO] [pt-BR] Creating an optimized production build...
[webpackbar] ✔ Server: Compiled successfully in 7.23s
[webpackbar] ✔ Client: Compiled successfully in 7.78s
[SUCCESS] Generated static files in "build/pt-BR".
[INFO] [es-419] Creating an optimized production build...
[webpackbar] ✔ Server: Compiled successfully in 9.60s
[webpackbar] ✔ Client: Compiled successfully in 10.17s
[SUCCESS] Generated static files in "build/es-419".
[INFO] [de] Creating an optimized production build...
[webpackbar] ✔ Server: Compiled successfully in 10.95s
[webpackbar] ✔ Client: Compiled successfully in 11.89s
[SUCCESS] Generated static files in "build/de".
[INFO] Use `npm run serve` command to test your build locally.
```

**All four locales GREEN sans babel patch.** Zero
`MODULE_NOT_FOUND`, zero `@site/*` literal-require failures, zero MDX
broken-link errors. SSG completes for every locale.

(Observation: build wall-times are actually *faster* sans patch on
this run — likely cache-warm artefacts from the WITH-patch baseline,
not a real speedup. The salient datum is green/red, not timing.)

## 5. Conclusion — OBSOLETE

The babel patch is **OBSOLETE**.

The wave-22/24 nested-`require("@site/...")` SSG failure observed in
those waves does not reproduce on the current `main` (HEAD `99269ed`)
even with the babel patch fully reverted. The defusing mechanism is
already supplied by:

1. **Wave-25 i18n `pathname://` link-flips** — 15 `.mdx`-suffixed
   cross-links in 12 i18n files (pt-BR + es-419 + de) converted to
   `pathname://` protocol, removing the `onBrokenMarkdownLinks: throw`
   compile-time class of failure.
2. **Wave-25 defensive `@site/*` + `@generated/*.json` resolver
   branches** in `patches/@docusaurus__core@3.10.1.patch`
   (`lib/ssg/ssgNodeRequire.js`) — these branches gracefully absorb
   any residual literal `@site/*` / `@generated/*` requires that
   reach the SSG runtime: `@generated/*.json` resolves to the on-disk
   `.docusaurus/<plugin>/p/*.json`, and `@site/*` returns a noop
   default-export stub (acceptable because the affected modules are
   meaningful only in the browser bundle anyway).

These two layers form the **actual load-bearing fix**. The babel-level
transforms the patch performed are now redundant: with `(2)` in place,
even if upstream `babel-plugin-dynamic-import-node` re-introduces the
nested-`require()` pattern in the server bundle, the SSG resolver
either redirects it to the real JSON file or returns an inert stub.

This finding **supersedes** the wave-25 closure assertion that the
babel patch is "load-bearing — removing them re-introduces the
wave-22 nested-require symptom on en-US" — empirical re-test shows
that claim is no longer true on the current pipeline.

## 6. Cleanup commit

The cleanup performed in this branch:

- **Deleted:** `patches/@docusaurus__babel@3.10.1.patch`.
- **Modified:** root `package.json` — removed the
  `@docusaurus/babel@3.10.1` entry from `pnpm.patchedDependencies`.
- **Modified:** `pnpm-lock.yaml` — pnpm regenerated the lockfile
  without the babel patch overlay (the `@docusaurus/babel@3.10.1`
  resolution moves from the patched variant to the upstream variant
  already present in the lockfile's package tree).
- **Smoke evidence:** §4 above is itself the regression-net pin —
  the WITHOUT-patch transcript is the proof that the docs-build
  smoke stays green sans patch. Any future regression that
  re-introduces a babel-level dependency on the patch will be caught
  by the same `pnpm build` gate that already runs in CI.

No new test file is added; the existing `pnpm build` gate is the
binding regression net. Adding a synthetic "patch-must-not-be-present"
assertion test would be cargo-cult — the meaningful invariant is
"docs build is green", which is already enforced.

## 7. Gates run + results

All from `apps/docs/` unless noted:

| Gate | Command | Result |
| --- | --- | --- |
| Build (sans babel patch, 4 locales) | `pnpm build` | green (§4 transcript) |
| Typecheck | `pnpm typecheck` (`tsc --noEmit`) | green (0 errors) |
| Lint | `pnpm lint` (`eslint --max-warnings=0 tests`) | green (0 warnings/errors) |
| Spec validator (repo root) | `python3 scripts/validate_specs.py` | green — `Todos validados: 449 com schema completo, 9 com YAML only (458 total)` |
| Cross-reference validator (repo root) | `python3 scripts/validate_references.py` | green — `Nenhuma dangling reference detectada` (514 docs analysados) |

Build with the babel patch removed is **the** regression-net pin.

## 8. DCO sign-off

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
