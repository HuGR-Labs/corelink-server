# Node 22 LTS Migration Note (DEBT-015)

**Status:** Active guidance — applies to tenant integration scripts, SDK
consumers, and internal CI workflows touching the `@corelink/client` JS SDK.

**Date:** 2026-05-16 (Wave-21 docs migration; commit lands on
`wt/r-prep-debt-015-node22-esm`).

**Closes:** DEBT-015 docs-side portion (P2). Build-side blockers in
`apps/docs/` are tracked under the same register row but remain owned by the
Docusaurus build follow-up (see `apps/docs/README.md` "Known limitation:
local production build" §).

## Why now

| Node line  | Status      | EOL date     | CoreLink recommendation              |
|------------|-------------|--------------|--------------------------------------|
| Node 18    | EOL         | 2025-04-30   | **Drop immediately.** No SDK fixes.  |
| Node 20    | Maintenance | 2026-04-30   | Best-effort. Plan migration this Q.  |
| **Node 22**| **Active LTS** | **2027-04-30** | **Default for all new tenant code.** |
| Node 24    | Current     | (not LTS)    | Not yet validated by CoreLink.       |

Node 22 became the active LTS line on 2024-10-29 and now carries the
longest support runway (until 2027-04-30) of any supported Node line.

## What changed in CoreLink docs

1. **SDK guides** (`apps/docs/docs/how-to/sdk-js/01-authenticate.mdx` and
   its three i18n mirrors: pt-BR, de, es-419) — prerequisite bumped from
   "Node.js 20+" to "Node.js 22+ (LTS until 2027-04-30)" with an explicit
   ESM-default callout deprecating CommonJS `require()` for new tenant code.
2. **CI integration recipes** (`06-ci-integration.mdx` × 4 locales) — Docker
   base images bumped: `node:20` → `node:22`, `cimg/node:20.11` →
   `cimg/node:22.11`.
3. **REAPI Node batch example** (`reference/reapi/_examples/.../node-batch-update.mdx`)
   — prereq updated to "Node 22+ (LTS until 2027-04-30; ESM-only)".
4. **Cross-runtime matrix** (`docs/sdk/javascript.md`) — Node 18 marked
   Unsupported (EOL), Node 20 best-effort, Node 22 marked Supported (current
   LTS, recommended) with an ESM-by-default callout pointing back to this
   note.
5. **`apps/docs/package.json` engine constraint** — bumped from
   `>=20.0.0 <22.0.0` to `>=22.0.0 <23.0.0`. NB: the upstream Docusaurus
   build still has the SSG blockers documented in `apps/docs/README.md`;
   the bumped engine reflects the *intended* runtime once the build-side
   portion of DEBT-015 closes. Local previews continue to use
   `node:22-bookworm` per the README guidance.

## Migration playbook for tenant operators

If you maintain CI scripts or build pipelines that call `@corelink/client`
or run `corelink` CLI from a Node host:

1. **Pin your CI base image to `node:22` or `cimg/node:22.x`.** Replace any
   `node:20`, `node:20.x`, `node:18`, or `cimg/node:20.x` with `node:22` /
   `cimg/node:22.x`.
2. **Ensure `package.json` declares `"type": "module"`.** The SDK ships
   ESM only. Tenant scripts using `require("@corelink/client")` will fail
   at module resolution.
3. **Use `.mjs` for CLI helpers** or rename `.js` → `.mjs` if `"type":
   "module"` is not appropriate at the package root.
4. **Update `engines.node`** in your tenant repos:
   ```json
   "engines": { "node": ">=22.0.0 <23.0.0" }
   ```
5. **Drop any `--experimental-vm-modules` or ESM-loader shims** for Node
   18/20 — Node 22 has native ESM with no flag gates required.
6. **Verify `Buffer`/`fetch`/`crypto.subtle` usage** still works (they
   all do in Node 22; no shim needed).
7. **Run your existing test suite** against Node 22 before flipping the CI
   base image. Most regressions are timing-sensitive or rely on
   undocumented Node 18 internals.

## Internal CI references

CoreLink first-party CI already runs on Node 22 (see
`apps/docs/README.md` line 130: the production docs build runs inside
`node:22-bookworm`). No changes needed to first-party CI from this WI.

## Out-of-scope for this WI

- The `apps/docs` SSG build still requires the upstream Docusaurus
  blockers from the DEBT-015 register row (MDX draft links, cross-doc
  extension normalization). The engine pin bump in `package.json` is a
  *forward-declaration* — it does not by itself unblock the local
  production build on a Node 22 host.
- Server-side Node usage: CoreLink server is Rust; this note applies only
  to JS/TS tenant code and the docs app.

## References

- Register entry: `specs/_audits/2026-05-15-debt-register.md` row DEBT-015
- Docusaurus build caveat: `apps/docs/README.md` §"Known limitation: local
  production build"
- Upstream Node release schedule: https://nodejs.org/en/about/previous-releases
