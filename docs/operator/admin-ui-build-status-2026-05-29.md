# admin-ui CF Pages build state — 2026-05-29

## Status
**Build NOT shippable** at HEAD `0c665b83`. Two issues fixed (committed), one blocker remains.

## What was fixed (committed)
1. `next.config.ts`: `outputFileTracingRoot` was `path.join(__dirname, "../../")` causing `@cloudflare/next-on-pages` to construct doubled paths `apps/admin-ui/apps/admin-ui/.next/...`. Anchored to `__dirname` so paths single-prefix.
2. `src/app/sign-in/[[...sign-in]]/page.tsx` + `sign-up/[[...sign-up]]/page.tsx`: changed `runtime = "nodejs"` → `"edge"` so the Vercel build → next-on-pages chain accepts these routes.

## Remaining blocker — `/_not-found` not in Edge Runtime

`pnpm run pages:build` exits with:
```
The following routes were not configured to run with the Edge Runtime:
  - /_not-found
```

`src/app/not-found.tsx` already exports `runtime = "edge"`, but the Vercel build step generates an INTERNAL `/_not-found` handler (note the underscore prefix) that doesn't inherit the runtime export from `not-found.tsx`.

Root cause hypothesis: **Sentry's Next.js instrumentation** (`@sentry/nextjs` 8.55.x) wraps `notFound()` calls with error-capture handlers + emits an internal `/_not-found` route at Node.js runtime to keep error reporting alive.

## Workarounds to try (next session)

1. **Remove Sentry temporarily**: comment out `withSentryConfig` wrapper in `next.config.ts`, rebuild. If build succeeds, Sentry is the cause.
2. **Use OpenNext.js Cloudflare adapter** (`@opennextjs/cloudflare`) instead of `@cloudflare/next-on-pages` — newer, handles internal routes better.
3. **Pin Sentry to a version with edge-aware not-found instrumentation** if such exists.
4. **Drop @sentry/nextjs entirely**, use a manual `init()` call in `instrumentation.ts` with edge-runtime guard.

## Currently live
- Pages project `corelink-admin-ui` exists (created by Stream B3)
- Custom domain `corelink-admin.humangr.com` provisioned
- DNS CNAME points at `corelink-admin-ui.pages.dev`
- Nothing actually deployed to the project yet — visiting the domain shows the Pages "not configured" placeholder

## Operator action
- Pick a workaround above
- `cd apps/admin-ui && pnpm run pages:build` until `.vercel/output/static` exists
- `worker/node_modules/.bin/wrangler pages deploy .vercel/output/static --project-name corelink-admin-ui`

## Files touched in fix attempts (committed)
- `apps/admin-ui/next.config.ts` — outputFileTracingRoot
- `apps/admin-ui/src/app/sign-in/[[...sign-in]]/page.tsx` — edge
- `apps/admin-ui/src/app/sign-up/[[...sign-up]]/page.tsx` — edge
