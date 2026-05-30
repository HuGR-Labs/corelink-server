# CoreLink Docs — Cloudflare Pages Project

**Created:** 2026-05-29
**Stream:** Tier 3 / Stream T3.2
**Author:** Automated operator SEAL

---

## Project Summary

| Field             | Value                                    |
|-------------------|------------------------------------------|
| Project name      | `corelink-docs`                          |
| Project ID        | `3b102837-842b-4cda-a7f0-967bf387cb18`  |
| Production branch | `main`                                   |
| pages.dev URL     | `https://corelink-docs.pages.dev`        |

---

## Deployment

| Field              | Value                                            |
|--------------------|--------------------------------------------------|
| Deployment ID      | `e66217f7`                                       |
| Deployment URL     | `https://e66217f7.corelink-docs.pages.dev`       |
| Files uploaded     | 1,320 files (1,062 new + 258 already present)    |
| Build tool         | Docusaurus 3.10.1                                |
| Build output dir   | `apps/docs/build/` (~13 MB)                      |
| Wrangler version   | 4.95.0                                           |

---

## Custom Domain Status

| Domain                        | Status  | Notes                                         |
|-------------------------------|---------|-----------------------------------------------|
| `corelink-docs.pages.dev`     | active  | Always-on Pages default subdomain             |
| `corelink-docs.humangr.com`   | active  | Custom domain; cert provisioned by CF Pages   |

---

## DNS CNAME Status

| Record name         | Type  | Content                       | Proxied | Action taken           |
|---------------------|-------|-------------------------------|---------|------------------------|
| `corelink-docs`     | CNAME | `corelink-docs.pages.dev`     | yes     | Pre-existing (no-op)   |

Zone: `humangr.com`

---

## Smoke Test Results (2026-05-29)

| URL                                              | HTTP | Latency  |
|--------------------------------------------------|------|----------|
| `https://corelink-docs.pages.dev`                | 200  | 0.56s    |
| `https://corelink-docs.humangr.com`              | 200  | 0.35s    |
| `https://e66217f7.corelink-docs.pages.dev`       | 200  | 0.49s    |

---

## Build Notes

- Source: `apps/docs/` — Docusaurus 3.10.1 with Diátaxis content structure
- Wave 3.13 content: `intro.md`, `quickstart.md`, `concepts/*`, `api/http.md`,
  `integrations/{bazel,turborepo,raw-curl}`, `security.md`, `troubleshooting.md`,
  `sidebars.ts` navbar wiring
- i18n: 4 locales built (en-US default, pt-BR, es-419, de)
- Algolia stub keys used (production keys injected at D-day via env vars)
- Sentry: not injected (no `SENTRY_DSN_DOCS` set at build time — correct for initial deploy)

---

## Operator Rebuild + Redeploy

```sh
cd apps/docs
pnpm run build
# then:
export CLOUDFLARE_API_TOKEN=$CLOUDFLARE_PAGES_API_TOKEN
/path/to/wrangler pages deploy build \
  --project-name corelink-docs \
  --branch main \
  --commit-dirty=true
```

---

## Security Notes

- Token used for Pages project/domain operations: `CLOUDFLARE_PAGES_API_TOKEN` (Pages:Edit scope)
- Token used for DNS queries: `CLOUDFLARE_API_TOKEN` (Zone:DNS:Edit scope, humangr.com zone)
- No secrets echoed, committed, or written to any file
- DNS CNAME was pre-existing; no DNS mutation was required
