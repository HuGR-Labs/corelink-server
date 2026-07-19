# Clerk Redirect URLs — Configuration State

**Run date:** 2026-05-29  
**Run type:** Phase 0 / Stream B10 — automated API configuration  
**Executed by:** autonomous agent (claude-sonnet-4-6)

---

## Clerk Instance

| Field             | Value                                       |
|-------------------|---------------------------------------------|
| Instance ID       | `ins_3EH6JXZQZtndiY5IdITV6MzLkag`          |
| Environment type  | `development`                               |
| Secret key prefix | `sk_test_xxxx...`                           |
| Primary domain    | `welcomed.eft-86.lcl.dev`                  |
| Frontend API URL  | `https://welcomed-eft-86.clerk.accounts.dev`|

---

## Redirect URLs — Before This Run

```
(empty — no redirect URLs configured)
```

---

## Redirect URLs — After This Run

All four URLs added via `POST /v1/redirect_urls`:

| URL                                     | Clerk ID                      | Purpose                        |
|-----------------------------------------|-------------------------------|--------------------------------|
| `https://humangr.com`    | `ru_3EPv51K6uYXlf0P6015CpV6TG0w` | Admin UI — Cloudflare Pages    |
| `https://www.humangr.com`               | `ru_3EPv4xKXZ8wNAR3b6LkYv2B1z96` | Production marketing site      |
| `http://localhost:3000`                 | `ru_3EPv57oRnIkorVZ3v9qowg4pERU` | Next.js local dev (default)    |
| `http://localhost:3001`                 | `ru_3EPv58WmY5FucSjAS6BDSjKahTV` | Next.js local dev (alternate)  |

**URLs added this run: 4**  
**Total redirect URLs after: 4**

---

## Allowed Origins (CORS) — After This Run

Updated via `PATCH /v1/instance` (`allowed_origins` field):

```json
[
  "https://humangr.com",
  "https://www.humangr.com",
  "http://localhost:3000",
  "http://localhost:3001"
]
```

Prior state: `null` (Clerk dev mode accepts all origins by default; explicit list now set for correctness).

---

## Publishable Key Note

The publishable key is stored in `.env.local` as `CLERK_PUBLISHABLE_KEY`.  
For Next.js (admin-ui), it must be exposed as `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY`.  
Cloudflare Pages env vars must include this key for the admin-ui build to pick it up.

**Operator TODO:** Verify that the Cloudflare Pages project `corelink-admin` has the env var  
`NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` set (Pages dashboard > Settings > Environment variables).

---

## Operator TODOs

Since this is a `development` instance, all changes were applied directly via the API.
When promoting to a **production** Clerk instance, the following must be re-applied:

1. Re-run this same `POST /v1/redirect_urls` flow against the production `CLERK_SECRET_KEY`
2. Set `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` in Cloudflare Pages environment variables (production)
3. Confirm the production domain `humangr.com` resolves and has a valid TLS cert
4. The production Clerk instance may restrict certain API operations — check Clerk dashboard for any
   pending domain verification requirements

---

## Verification

Re-listing `GET /v1/redirect_urls` after the run confirmed all 4 URLs present:

```
humangr.com present: YES
www.humangr.com present: YES
localhost:3000 present: YES
localhost:3001 present: YES
```

`GET /v1/instance` confirmed `allowed_origins` set to the same 4 entries.

---

_This file documents the live state of the Clerk dev instance redirect URL configuration._  
_Do not commit `.env.local` or any secret key values._
