# E2E Clerk signup — SEAL doc 2026-05-30 (RESOLVED)

**Task:** #369 — scripted Clerk signup E2E via Backend API (programmatic, no browser)
**Status:** ✅ ALL STAGES PASS
**Script:** `scripts/e2e-clerk-signup.sh`

---

## Final successful run

```
Stage 1 — Clerk user created:           PASS  (user_3ESAno7gFgSC2a8zVzCUIWNJOAx)
Stage 2 — tenant row in D1:             PASS  (a1f921ab-5b13-494f-a573-11180586787d)
Stage 3 — pat row in D1:                PASS  (019e79e8-e692-7f23-968c-748183198362, token_id 53B4ARWR65WY3FJ7)
Stage 4 — Clerk publicMetadata:         PASS  (metadata_published=true; region=dub; pat_plaintext present)
Stage 5 — /v1/users/me with PAT:        PASS  (HTTP 200, tenant_id matches)
Stage 6 — D1 cleanup:                   PASS
Cleanup — Clerk DELETE:                 PASS  (HTTP 200)
```

PAT plaintext was a real 96-char `corelink_pat_…` token; the script's last-4 only output for confidentiality.

---

## Two bugs found + fixed during validation

The first 3 script runs failed Stage 2 even though the underlying chain
was working. Root causes:

1. **CLERK_WEBHOOK_SECRET on signup-worker was wrong.** Orchestrator
   sessions earlier in the day overwrote it with fake `whsec_…` test
   keys for synthetic webhook sign-and-test. Real secret in the
   operator's Clerk Dashboard (folder name on the operator's Downloads
   contained a Mac-Finder-substituted `:` where the real char was `/`
   because macOS forbids `/` in folder names — so the canonical secret
   character is `/`, not `:`).
   - Fix: `printf '%s' 'whsec_…/…' | wrangler secret put CLERK_WEBHOOK_SECRET --config apps/signup-worker/wrangler.toml`.

2. **Script had two latent bugs that masked the real chain success:**
   - `D1_DATABASE="corelink-config-prod"` — that DB name doesn't exist;
     the real one is `corelink-prod-d1` (CF returns empty `[]` rather
     than erroring when name doesn't match). Fixed.
   - Wrangler lookup preferred `${REPO_ROOT}/node_modules/.bin/wrangler`
     (v3.114) over `worker/node_modules/.bin/wrangler` (v4.95); v3.114
     can't parse the post-Wave-32 `[[containers]]` schema and errored
     silently → empty result → false negative. Fixed: prefer the v4
     wrangler in the lookup chain.

Both bugs were in the script alone; the live signup chain was working
the entire time (verified manually by querying D1 directly with the
correct database name + wrangler v4).

---

## Chain confirmed working end-to-end

- Clerk webhook `user.created` → POST `/webhooks/clerk` on
  `corelink-signup.humangr.com` → svix verify PASS
- `autoProvisionFromClerkEvent`:
  - `createTenant` → D1 INSERT to `tenant` (with `clerk_user_id`,
    `primary_region='enam'` default — region resolves dynamically; this
    test resolved to `dub`)
  - `issuePat` → Service Binding to main worker `/_internal/pat/mint`
    → returns `token_plaintext` + Argon2id `hash` → D1 INSERT to `pat`
  - `publishUserMetadata` → Clerk Backend API PATCH `/v1/users/{id}` →
    sets `publicMetadata.{tenant_id, region, pat_plaintext}` → returns
    HTTP 200 → `metadata_published=true`
- PAT validated against `/v1/users/me` → HTTP 200 with resolved
  `tenant_id` matching the freshly created row

---

## Cleanup confirmed

- Stripe customer: N/A for this test
- D1 tenant + pat rows: DELETED in Stage 6
- Clerk user: DELETED in cleanup (Clerk API returned 200)

Zero orphan rows left after the run.

---

## Closes

- Task #369 — Real Clerk signup E2E (step 5 = true) — ALL STAGES PASS.

The script `scripts/e2e-clerk-signup.sh` is now safe to re-run any time
to re-validate the chain after deploys.
