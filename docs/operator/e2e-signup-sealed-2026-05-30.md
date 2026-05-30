# E2E Clerk signup — SEAL doc 2026-05-30

**Task:** #369 — scripted Clerk signup E2E via Backend API (programmatic, no browser)
**Status:** HARD PAUSE — Stage 2 FAIL (webhook → D1 still blocked by signature mismatch)
**Script:** `scripts/e2e-clerk-signup.sh`
**Branch HEAD at run time:** `dd34980eb7bbeb3732ffc53a036f3c82e1a66678`

---

## Run summary

| Stage | Status | Detail |
|---|---|---|
| 0. Load credentials | PASS | CLERK_SECRET_KEY + CLOUDFLARE_API_TOKEN present in .env.local |
| 1. Clerk user created | PASS | user.id captured via Clerk Backend API POST /v1/users |
| 2. tenant row in D1 within 30s | **FAIL** | No row after 30s polling — webhook rejected at svix verify (HTTP 401) |
| 3. pat row in D1 | NOT RUN | Precondition (stage 2) failed |
| 4. Clerk publicMetadata populated | NOT RUN | Precondition (stage 2) failed |
| 5. /v1/users/me with PAT | NOT RUN | Precondition (stage 2) failed |
| 6. D1 cleanup | NOT RUN | Precondition (stage 2) failed |
| Cleanup: Clerk DELETE | PASS | All test users deleted 200 |

---

## Wrangler tail evidence (smoking gun)

Captured live from `wrangler tail corelink-signup-worker --format json` while creating test
user `user_3ES4w9al7UM25SHnHu2zqk9MKUG`:

```json
{
  "wallTime": 4,
  "cpuTime": 3,
  "outcome": "ok",
  "scriptVersion": { "id": "abeef77a-ee71-4fb9-b793-05c41fe6eba9" },
  "event": {
    "request": {
      "url": "https://corelink-signup.humangr.com/webhooks/clerk",
      "method": "POST",
      "headers": {
        "svix-id": "msg_3ES4wAdbQWfo4QTy1NjhZ2xRdpn",
        "svix-signature": "v1,PKPAY1l4SHgzu3fqqWhAsP6zZBUEVeXaLEmEcULGUrw=",
        "svix-timestamp": "1780158877",
        "user-agent": "Svix-Webhooks/1.84.0 (sender-9YMgn; +https://www.svix.com/http-sender/)",
        "cf-connecting-ip": "52.215.16.239",
        "cf-ipcountry": "IE"
      }
    },
    "response": { "status": 401 }
  }
}
```

**Key facts:**
- Clerk webhook IS delivered within ~4 seconds of user creation (Svix user-agent, all 3 svix headers present)
- Worker version `abeef77a` (latest deployment, deployed 2026-05-30T15:00 UTC) handles the request
- Response: HTTP 401 — maps to the `invalid_signature` branch in `apps/signup-worker/src/webhooks/clerk.ts:351`
- `cpuTime = 3ms`, `wallTime = 4ms`, `outcome = "ok"` → worker exited cleanly after 401; zero subrequests (confirms signature verify is the rejection point before any outbound calls)

---

## Root cause (identical to 2026-05-29 diagnosis)

The `CLERK_WEBHOOK_SECRET` bound on `corelink-signup-worker` does not match the
signing secret of the registered Clerk webhook endpoint in Svix (managed by Clerk).

The Tier-3 wave (2026-05-30T01:17–03:30 UTC) included secret changes
(`Source: Secret Change` deployment entries) but the CLERK_WEBHOOK_SECRET update
did not land with the correct value from the Clerk dashboard.

Evidence: svix signature `v1,PKPAY1l4SHgzu3fqqWhAsP6zZBUEVeXaLEmEcULGUrw=` was
computed by Svix using the endpoint's actual signing secret. The worker's
`verifySvixSignature` at `clerk.ts:100–133` recomputes HMAC-SHA256 over
`${svix-id}.${svix-timestamp}.${body}` with the bound secret — if it doesn't
match, returns false → 401.

---

## What IS verified (Tier-3 production SEAL)

The prior Tier-3 backend SEAL (`docs/operator/tier3-backend-sealed-2026-05-30.md`)
confirmed all 5 stages via directly injected svix-signed payloads (using
`user_diag_...` synthetic clerk_user_ids). The D1 database has 10 tenant rows
and 6 PAT rows from those test runs. All bindings are correct:

- CONFIG_DB → `d64742ea-e102-40b2-a844-ff02e3f94562` ✅
- CORELINK_API_SVC service binding → `corelink-prod` ✅  
- CLERK_SECRET_KEY bound ✅
- CORELINK_INTERNAL_AUTH_KEY bound ✅
- STRIPE_WEBHOOK_SECRET correct (Stripe E2E passed) ✅
- CLERK_WEBHOOK_SECRET: **bound but WRONG VALUE** ❌

---

## Fix (operator action required — 5 minutes)

1. Open Clerk Dashboard → Webhooks
2. Click the endpoint pointing to `https://corelink-signup.humangr.com/webhooks/clerk`
3. Click **Signing Secret** → **Reveal** → copy the `whsec_...` value verbatim
4. Run from repo root:
   ```sh
   cd apps/signup-worker
   printf '%s' 'whsec_<paste_here>' | \
     /Users/gustavoschneiter/Documents/HuGR/corelink-server/node_modules/.bin/wrangler \
     secret put CLERK_WEBHOOK_SECRET
   ```
   (Use `printf '%s'` not `echo -n` to avoid trailing newline on some shells)
5. Confirm a new `Source: Secret Change` deployment appears in `wrangler deployments list`
6. Re-run `bash scripts/e2e-clerk-signup.sh` — should go ALL GREEN within 2 minutes

**No code changes needed.** The wiring is correct; only the secret value is wrong.

---

## Test user artifacts (cleaned up)

| user_id | email | created | deleted |
|---------|-------|---------|---------|
| `user_3ES48Sv51JVwmX4cWrAKnH6qTbC` | `e2e-test-1780158474@example.com` | run 1 | 200 OK |
| `user_3ES4Xj9g9Edjf4wdGLLIN9uuNy4` | `e2e-diag-1780158676@example.com` | diagnostic | 200 OK |
| `user_3ES4w9al7UM25SHnHu2zqk9MKUG` | `e2e-tailtest-1780158877@example.com` | wrangler tail | 200 OK |

All test users deleted via Clerk Backend API DELETE → all returned HTTP 200.
No orphan D1 rows — no tenant row was ever written (webhook never passed svix verify).

---

## Security checklist

- `.env.local` never echoed or committed
- `CLERK_SECRET_KEY` never echoed to stdout
- `pat_plaintext` never generated (chain didn't reach stage 3)
- All test users cleaned up via Clerk DELETE
- No secrets committed in this doc
- Test emails used `@example.com` (RFC 2606 reserved; no real mailbox)

---

## Script gate status

```sh
bash -n scripts/e2e-clerk-signup.sh  # exit 0 ✅
```

Script is syntactically valid and ready to run. Re-run after operator fixes
CLERK_WEBHOOK_SECRET — expected output:

```
[PASS] Clerk user created — user.id: user_...
[PASS] Tenant row found in D1 after Xs
[PASS] PAT row found in D1
[PASS] Clerk publicMetadata populated
[PASS] /v1/users/me returned 200 with correct tenant_id=...
[PASS] D1 test rows cleaned up
[PASS] Clerk DELETE returned 200 — test user removed
ALL STAGES PASS
```

---

## Next run instructions

After the operator corrects `CLERK_WEBHOOK_SECRET`:

```sh
bash scripts/e2e-clerk-signup.sh 2>&1 | tee /tmp/e2e-clerk-$(date +%s).log
```

Update this doc (or create `e2e-signup-sealed-pass-YYYY-MM-DD.md`) with the
full output and mark #369 CLOSED.
