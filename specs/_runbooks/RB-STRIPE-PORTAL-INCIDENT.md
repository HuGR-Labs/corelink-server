---
id: "RB-STRIPE-PORTAL-INCIDENT"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "stripe", "billing", "customer-portal", "r-prep"]
---

# RB-STRIPE-PORTAL-INCIDENT — Stripe Customer Portal Incident Triage

**Scope.** Operator runbook for the
`POST /v1/customer/billing/portal-session` endpoint and the downstream
Stripe Customer Portal hosted UI. Used when customers report that
"Open Stripe Portal" doesn't work, the redirect lands on an error
page, or returning from the portal lands somewhere broken.

**Audience.** On-call billing engineer (primary). Secondary:
customer-success on duty (drives customer-side communication).

**Companion docs.**
- Configuration spec: `specs/_audits/2026-05-15-stripe-customer-portal-spec.md`
- Webhook DLQ runbook: `specs/_runbooks/RB-WEBHOOK-DLQ-REPLAY.md`
- Webhook DLQ audit: `specs/_audits/2026-05-15-webhook-retry-dlq.md`
- Customer guide: `apps/docs/docs/how-to/billing/manage-subscription.mdx`

**Severity bands.**

- **SEV-3 (warning).** Single customer report; portal-session endpoint
  responding 2xx for >99% of calls in the last 30 min.
- **SEV-2 (page).** Portal-session endpoint error rate ≥ 5% sustained
  10 min, OR ≥ 3 distinct customers reporting in the same hour.
- **SEV-1 (page + war room).** Endpoint 5xx ≥ 50% sustained 5 min,
  OR Stripe Status page reports `billing` service degradation, OR an
  active customer pays >$10k/mo cannot reach the portal.

## 1. Symptom catalogue

| Customer-side symptom                                                  | Most likely cause                                  | Section |
|------------------------------------------------------------------------|----------------------------------------------------|---------|
| "Open Stripe Portal" button spins forever                              | Server 5xx (Stripe API down OR audit emit failed)  | §2      |
| Browser shows "stripe portal not configured" (Stripe's own error page) | Stripe portal config drifted / live mode mismatch  | §3      |
| Portal opens, user finishes, returns to a 404 / wrong locale           | Return URL allowlist drift                         | §4      |
| 403 with "tenant mismatch" in body                                     | JWT claim ≠ request body `tenant_id`               | §5      |
| Portal-session endpoint returns 401 for a signed-in user               | Clerk session expired mid-flow                     | §6      |

## 2. Server 5xx on `/v1/customer/billing/portal-session`

### 2.1 Triage (15 min budget)

1. Open Grafana dashboard `DASH-BILLING` → row "Customer Portal" →
   panel `Portal session 5xx rate`. Confirm spike.
2. Check Stripe Status: <https://status.stripe.com>. If `billing`
   service is degraded → escalate to SEV-2, post Slack
   `#billing-oncall`, set status page banner "Self-service billing
   temporarily unavailable; subscription changes still take effect via
   webhooks", and resume in §2.3.
3. If Stripe is green, query the audit DLQ:
   ```sh
   wrangler d1 execute corelink_prod --command \
     "SELECT event_type, count(*) FROM corelink_audit_dlq
      WHERE event_type='corelink.billing.portal_session_created'
        AND created_at > strftime('%s','now','-30 minutes')
      GROUP BY event_type"
   ```
   If rows > 0: the audit sink is the failure point — the handler is
   fail-CLOSING correctly but our audit pipeline is broken. Cross-
   route to `RB-WEBHOOK-DLQ-REPLAY.md` §3 (the DLQ surface is shared).
4. If Stripe is green AND audit DLQ is empty, suspect outbound
   network: check `STRIPE_SECRET_KEY` rotation log
   (`RB-SECRETS-DRIFT.md` §4) — a stale key returns 401 from Stripe
   which we surface as 5xx.

### 2.2 Decide

- Stripe API down: WAIT. Do NOT roll back our code — the issue is
  upstream. Set status page; tell customers their subscriptions
  remain active (webhook events from Stripe's side will drain on
  recovery and our DLQ will catch any missed events).
- Audit pipeline down: roll forward — run `RB-WEBHOOK-DLQ-REPLAY.md`
  §4 to drain the DLQ and unblock new sessions.
- Stale Stripe secret: rotate per `RB-SECRETS-DRIFT.md` §6.

### 2.3 Act + verify

1. After mitigation, hit the canary:
   ```sh
   curl -sS -X POST https://app.corelink.humangr.com/api/v1/customer/billing/portal-session \
     -H "Authorization: Bearer $CANARY_SESSION_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"return_url":"https://app.corelink.humangr.com/en/customer/billing","tenant_id":"tenant_canary"}'
   ```
2. Expect 200 + `portal_url` starting with `https://billing.stripe.com/p/session/`.
3. Confirm the canary audit row landed:
   ```sh
   wrangler d1 execute corelink_prod --command \
     "SELECT count(*) FROM corelink_audit_events
      WHERE event_type='corelink.billing.portal_session_created'
        AND tenant_id='tenant_canary'
        AND created_at > strftime('%s','now','-2 minutes')"
   ```
4. Tear down status banner; close incident.

## 3. "Portal not configured" Stripe error page

The customer redirected to Stripe but Stripe shows
*"You haven't configured a customer portal yet"*. This means the
Stripe Dashboard portal config was deleted, drifted between live and
test mode, OR the wrong API key (live vs. test) is in use.

1. Visit <https://dashboard.stripe.com/settings/billing/portal>
   (live mode). If "No active configuration" → restore from
   §6 below. If "Active" → check test mode dashboard same path; the
   error is from the OTHER mode → fix env var
   (`STRIPE_SECRET_KEY` vs `STRIPE_SECRET_KEY_TEST` selection in
   `apps/server`).
2. Replay the canary in §2.3.

## 4. Return URL allowlist drift

The customer comes back from the portal and lands on a CoreLink 404
or a different locale page than the one they started on.

1. Compare the active allowlist (Stripe Dashboard → Customer Portal →
   Default redirect on return) against
   `specs/_audits/2026-05-15-stripe-customer-portal-spec.md` §2.4.
2. If drift exists, edit the dashboard config to match the spec.
   Stripe rejects any return URL not in the allowlist BEFORE
   redirecting, so the fallback behavior is "stay on Stripe's
   landing page" — which is the bug the customer sees.
3. After fixing, walk through one full Open → Manage → Close cycle as
   the canary user.

## 5. 403 tenant mismatch

`tenant_id` in the request body did not match the Clerk JWT claim.
This is normally a client bug but can also indicate a session-
hijack attempt.

1. Pull the audit row (or the 403 access log line) — record
   `actor_user_id`, claimed `tenant_id` in body, JWT `tenant_id`.
2. If `actor_user_id` is a real customer + claimed `tenant_id` is one
   they previously belonged to: they probably switched orgs in Clerk
   mid-session; ask them to sign out + sign in. Close ticket.
3. If JWT `tenant_id` is a SaaS-ops tenant and body `tenant_id` is a
   customer tenant: **escalate to security** — possible session
   impersonation. Do NOT close. Page `RB-SECURITY-VULNERABILITY-INTAKE.md`.

## 6. Clerk session expired mid-flow

The customer says "I was signed in, clicked Open Portal, then got
asked to sign in again". Either their session TTL was hit between
page load and click, OR our middleware rejected the cookie.

1. Confirm by joining the access log on `actor_user_id` —
   `/customer/billing` GET 200 then `/v1/customer/billing/portal-session`
   POST 401 within a few seconds = session TTL hit.
2. If reproducible across many users: check Clerk dashboard for a
   recent session-TTL config change. Roll back if needed.
3. Tell the customer to sign in again + click the button again. The
   `PortalLauncher.tsx` button does not auto-retry on 401 by design
   (we never want to re-mint a portal URL after auth state changed).

## 7. Portal config restore (full re-create)

If the Stripe Dashboard portal config is missing entirely, recreate
via the Stripe CLI from spec §2.1 / §2.2:

```sh
stripe billing_portal configurations create \
  --business-profile[headline]="Manage your CoreLink subscription" \
  --business-profile[privacy_policy_url]="https://corelink.humangr.com/privacy" \
  --business-profile[terms_of_service_url]="https://corelink.humangr.com/legal/terms" \
  --features[invoice_history][enabled]=true \
  --features[payment_method_update][enabled]=true \
  --features[customer_update][enabled]=true \
  --features[customer_update][allowed_updates][]=email \
  --features[customer_update][allowed_updates][]=tax_id \
  --features[customer_update][allowed_updates][]=address \
  --features[subscription_update][enabled]=true \
  --features[subscription_update][default_allowed_updates][]=price \
  --features[subscription_update][products][0][product]="$STRIPE_PRODUCT_ID" \
  --features[subscription_update][products][0][prices][]="$STRIPE_PRICE_ID_FREE" \
  --features[subscription_update][products][0][prices][]="$STRIPE_PRICE_ID_STARTER" \
  --features[subscription_update][products][0][prices][]="$STRIPE_PRICE_ID_TEAM" \
  --features[subscription_update][products][0][prices][]="$STRIPE_PRICE_ID_ENTERPRISE" \
  --features[subscription_cancel][enabled]=true \
  --features[subscription_cancel][mode]=at_period_end \
  --default-return-url="https://app.corelink.humangr.com/en/customer/billing"
```

After running, verify in the Dashboard that the configuration
matches every row in `specs/_audits/2026-05-15-stripe-customer-portal-spec.md` §2.1.
Run the canary in §2.3.

## 8. Postmortem requirements

Every SEV-2+ portal incident MUST file a postmortem within 72h per
`RB-POSTMORTEM-PROCESS.md`. Required attachments:

- Timeline (detection → first action → resolution).
- Audit DLQ depth chart for the incident window.
- Customer-impact count (distinct `actor_user_id` that hit a 5xx).
- Stripe-side status pull (https://status.stripe.com/history) for
  the window — proves whether root cause was upstream.
