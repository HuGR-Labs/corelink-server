# admin-ui CF Pages build state — 2026-05-29 (RESOLVED)

## Status
**Build shippable.** `pnpm run pages:build` produces a deployable
`.vercel/output/static` tree with the synthetic `/_not-found` and `/_error`
routes converted to Edge runtime via a post-build patch.

## Root cause (corrected — was NOT Sentry)
Next.js 15 always emits internal `/_not-found.func` and `/_error.func`
as `nodejs24.x` runtime, regardless of:

- `not-found.tsx` exporting `runtime = "edge"`
- The root layout being `runtime = "edge"`
- `dynamic = "force-static"` on the not-found route

The launcher (`___next_launcher.cjs`) requires `async_hooks` — Node-only.
`@cloudflare/next-on-pages` then rejects the build because not every
function is Edge.

Verified by removing the Sentry wrapper entirely: same error. Sentry was
exonerated.

## Fix
Two scripts run between `vercel build` and `next-on-pages`:

1. `scripts/seed-vercel-project.mjs` — writes `.vercel/project.json` so
   `vercel build` skips its auth check (we never deploy to Vercel; we
   only need its build output for next-on-pages to consume).
2. `scripts/patch-vercel-synthetic-routes.mjs` — overwrites
   `_not-found.func`, `_not-found.rsc.func`, `_error.func`, and
   `_error.rsc.func` with minimal Edge-runtime handlers returning 404/500
   HTML (or JSON for the `.rsc` variants). Inherits the `environment`
   block from a sibling Edge function so build IDs stay consistent.

Updated `pages:build`:
```
vercel build && node scripts/patch-vercel-synthetic-routes.mjs && next-on-pages --skip-build
```

## Currently live
- Pages project `corelink-admin-ui` exists
- Custom domain `corelink-admin.humangr.com` provisioned
- DNS CNAME points at `corelink-admin-ui.pages.dev`
- Nothing actually deployed yet — next step is `pnpm pages:deploy`

## Future cleanup
- When `@opennextjs/cloudflare` matures (handles synthetic routes
  natively), drop the patch script. Track the next-on-pages migration
  guide.
- When Next.js exposes a route-segment knob for `/_not-found` runtime,
  drop the patch as well.

## Files touched in this fix (all committed)
- `apps/admin-ui/scripts/seed-vercel-project.mjs` (new)
- `apps/admin-ui/scripts/patch-vercel-synthetic-routes.mjs` (new)
- `apps/admin-ui/package.json` — `pages:build` pipeline updated
- `apps/admin-ui/src/app/not-found.tsx` — added `dynamic = "force-static"`
  (defensive; not strictly required after the patch lands but cheap and
  keeps the page out of SSR if future Next versions respect it)
- `apps/admin-ui/next.config.ts` — confirmed Sentry wrapper is fine

## Prior fixes still relevant (already committed earlier)
- `next.config.ts`: `outputFileTracingRoot: __dirname` (was `../../`)
- `sign-in` + `sign-up` route pages: `runtime = "edge"` (was `nodejs`)
