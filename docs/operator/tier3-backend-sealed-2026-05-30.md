# Tier 3 backend SEAL — 2026-05-30

Self-serve onboarding + billing backend is production-ready. The remaining
work is admin-ui frontend (deferred to `wt/admin-ui-opennext`, dispatched
as a focused agent run).

## What's live in prod

### Signup chain (5 stages)

| Stage | Verified | Notes |
|---|---|---|
| 1. Svix signature verify | ✅ HTTP 401 on invalid sig | `apps/signup-worker/src/webhooks/clerk.ts:91` |
| 2. Tenant INSERT to D1 | ✅ Row present, `clerk_user_id` link | `corelink-config-prod` D1 |
| 3. PAT mint via Service Binding | ✅ HTTP 200 with `token_plaintext` | Bypass for CF Error 1014 |
| 4. PAT INSERT to D1 | ✅ Argon2id hash + `admin` scope | `corelink-config-prod` D1 |
| 5. Clerk metadata patch | ✅ 4xx-tolerant; 5xx re-thrown | Idempotency for webhook retries |

D1 state at SEAL time: 10 tenants, 6 PATs (test runs).

### Stripe checkout backend (Phase 1b ready)

- `apps/admin-ui/src/app/api/checkout/session` — bridge route (gated on Clerk
  session token).
- Main worker `/v1/onboarding/tier-select` — returns HTTP 401 without auth
  (route wired, auth gate active).
- `apps/signup-worker/src/webhooks/stripe.ts` — handlers for
  `checkout.session.completed`, `customer.subscription.updated`,
  `customer.subscription.deleted`. Idempotency via Stripe `evt_*` event id.
- `/webhooks/stripe` returns HTTP 400 invalid_signature without sig (route
  wired, sig verify active).
- `tenant_billing` D1 table: ready for upsert.

Stripe webhook endpoint `we_1TcaMDLh0hhAZjwol9KDCJTp` already created via
Stripe API in this session; signing secret in `STRIPE_WEBHOOK_SECRET`.

## Key non-obvious facts (worth re-reading)

### CF Error 1014 (CNAME Cross-User Banned)

`fetch("https://other-worker.humangr.com/...")` from Worker A → Worker B on
the same account is rejected by the public edge with HTTP 403 body
`error code: 1014`. The 403 never reaches Worker B; `wrangler tail` on B is
empty. Fix: `[[services]]` binding in Worker A's `wrangler.toml`. Saved as
[[cf-worker-to-worker-service-binding]] memory entry.

### Direct `/_internal/pat/mint` does NOT write to D1

The container's `pat::mint` only returns the plaintext + hash. The D1 INSERT
happens in the signup-worker TypeScript code AFTER it gets the response.
Direct curl calls to `/_internal/pat/mint` for diagnostic purposes will
produce un-persisted tokens (verified: `pat_not_found` reason on auth lookup).

### Idempotency contract on signup

- D1 INSERT-OR-IGNORE on tenant + PAT happens BEFORE Clerk metadata patch.
- 4xx from Clerk → log + return HTTP 200 with `metadata_published: false`.
- 5xx from Clerk → throw → Svix retries the whole webhook.
- Implication: if Clerk metadata fails non-transiently, tenant + PAT
  persist but the user's `publicMetadata` is empty — they cannot read the
  `pat_plaintext` from their session. Reconcile out-of-band by calling
  `/api/users/<user_id>` PATCH against Clerk with the persisted row.

## Commits sealed this session

| SHA | Subject |
|---|---|
| `4f4e46b8` | Service Binding + CONFIG_DB for E2E provisioning |
| `d33f25f3` | Tolerate Clerk metadata 4xx; retry on 5xx |
| `3daebca6` | Edge-runtime patch for synthetic `/_not-found` + `/_error` |
| `71e3d95b` | Defer Clerk widget load; remove ClerkProvider from root layout |

## What still needs work

### #371 — admin-ui OpenNext migration (dispatched as focused agent)

Branch `wt/admin-ui-opennext`. Three blockers known + briefed:
1. `output: "standalone"` required
2. `outputFileTracingRoot` back to monorepo root
3. Strip `runtime = "edge"` from 10 files
4. (NEW) esbuild "Invalid alias name" on Next 15.5.18 internals — agent will
   triage between OpenNext version bump, next-intl plugin disable, Sentry
   plugin disable, or Next downgrade.

Unblocks #369 (real Clerk signup) and #370 (Stripe checkout E2E from UI).

### Pages project will be retired

Once OpenNext migration lands and `corelink-admin-ui` Worker takes the
custom domain, the existing `corelink-admin-ui` Pages project becomes
inert. Decision deferred: keep as fallback or delete.

### Deferred — NEXT_PUBLIC env vars on Pages project

The Pages env has `CLERK_SECRET_KEY`, `STRIPE_*` but not the
`NEXT_PUBLIC_*` build-time vars (they're inlined from
`apps/admin-ui/.env.production.local`). After OpenNext migration, the same
vars need to land in the Worker env (via `wrangler secret put` or the
Workers dashboard).
