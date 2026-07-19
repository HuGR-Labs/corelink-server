# CoreLink Admin UI — Cloudflare Pages Project

**Created:** 2026-05-29  
**Stream:** Phase 1 / Stream B3  
**Author:** Automated operator SEAL

---

## Project Summary

| Field            | Value                                    |
|------------------|------------------------------------------|
| Project name     | `corelink-admin-ui`                      |
| Project ID       | `36d65631-252a-4f76-8ca7-8fc4c1c555a9`  |
| Production branch| `main`                                   |
| pages.dev URL    | `https://corelink-admin-ui.pages.dev`    |

---

## Custom Domain Status

| Domain                        | Status   | Notes                                             |
|-------------------------------|----------|---------------------------------------------------|
| `corelink-admin-ui.pages.dev` | active   | Always-on Pages default subdomain                |
| `humangr.com`  | pending  | Added 2026-05-29; cert provisions after DNS TTL  |
| `corelink-app.humangr.com`    | active   | Pre-existing domain (left untouched)             |

---

## DNS CNAME Status

| Record name              | Type  | Content                         | Proxied | Action taken                          |
|--------------------------|-------|---------------------------------|---------|---------------------------------------|
| `corelink-admin`         | CNAME | `corelink-admin-ui.pages.dev`   | yes     | Updated (was: corelink-prod worker)   |

Zone: `humangr.com`  
DNS record ID: `a5c5d015842ad841759dce865ce509b8`

**Note:** The CNAME previously pointed to a Cloudflare Worker (`corelink-prod.gustavoschneiter.workers.dev`). It has been redirected to the Pages project. If any Worker routing depended on that CNAME, verify Worker custom routes are still intact.

---

## Custom Domain Verification

`humangr.com` will transition from `pending` → `active` automatically once:
1. DNS propagates (Cloudflare proxied records are near-instant within CF network)
2. CF Pages provisions the TLS certificate (~1-5 minutes typical)

To check status:
```sh
curl -fsS \
  -H "Authorization: Bearer $CLOUDFLARE_PAGES_API_TOKEN" \
  "https://api.cloudflare.com/client/v4/accounts/6a1fc1c626fc2628823e60b9db01f5cd/pages/projects/corelink-admin-ui/domains" \
  | python3 -c "import sys,json; [print(r['name'], r['status']) for r in json.load(sys.stdin).get('result',[])]"
```

---

## Next Operator Steps

### Step 1 — Build the app
```sh
cd apps/admin-ui
npm run pages:build
# Produces: apps/admin-ui/.vercel/output/static
```

### Step 2 — Deploy to Pages
```sh
cd apps/admin-ui
npx wrangler pages deploy .vercel/output/static \
  --project-name corelink-admin-ui \
  --branch main
```

### Step 3 — Set runtime secrets
All secrets must be set before the deployed app can authenticate users or call APIs.

```sh
cd apps/admin-ui

# Clerk auth
npx wrangler pages secret put NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY --project-name corelink-admin-ui
npx wrangler pages secret put CLERK_SECRET_KEY --project-name corelink-admin-ui

# CoreLink API
npx wrangler pages secret put NEXT_PUBLIC_CORELINK_API_URL --project-name corelink-admin-ui
npx wrangler pages secret put CORELINK_ADMIN_API_KEY --project-name corelink-admin-ui

# Add any additional env vars from apps/admin-ui/.env.local (non-secret ones can use wrangler.toml [vars])
```

### Step 4 — Verify deployment
```sh
# pages.dev (always available)
curl -I https://corelink-admin-ui.pages.dev

# Custom domain (after cert provisions)
curl -I https://humangr.com
```

---

## Wrangler Config Reference

Config file: `apps/admin-ui/wrangler.toml`  
Build command (in package.json): `npm run pages:build`  
Build output directory: `.vercel/output/static`

---

## Security Notes

- Token used for project/domain operations: `CLOUDFLARE_PAGES_API_TOKEN` (Pages:Edit scope only)
- Token used for DNS CNAME update: `CLOUDFLARE_API_TOKEN` (Zone:DNS:Edit scope, humangr.com zone)
- No secrets were echoed, committed, or written to any file
- The CNAME update from Worker to Pages was a DNS-only change; Worker code is unmodified
