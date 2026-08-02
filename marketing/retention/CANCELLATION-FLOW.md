---
id: "R-PREP-CANCELLATION-FLOW"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "R-PREP-CHURN-RETENTION"
tags: ["marketing", "retention", "cancellation", "admin-ui", "ux", "wireframe", "r-prep", "ga", "post-launch"]
---

# CoreLink Cancellation Flow — Customer-Facing Experience Design

> **Audience.** Product (admin-ui), Design, Engineering (admin-ui owner), Customer Success (retention-offer authoring), Legal (deletion policy review).
> **Purpose.** Specification for the in-product cancellation experience: 5 wireframe steps + retention-offer rules + post-cancel data-retention window + re-activation path.
> **Non-goal.** Building a dark-pattern retention wall. The principle: **legibility over friction**. The customer can complete cancellation in ≤ 3 clicks from any deep state; the retention offer is offered once, not repeatedly.
> **Companion docs.** [`CHURN-RISK-SIGNALS.md`](./CHURN-RISK-SIGNALS.md), [`RETENTION-PLAYBOOK.md`](./RETENTION-PLAYBOOK.md), [`../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md`](../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md), [`../../specs/_runbooks/RB-DSR-GDPR.md`](../../specs/_runbooks/RB-DSR-GDPR.md) (deletion is a DSR pathway), [`../../specs/_runbooks/RB-DPA-CHANGE.md`](../../specs/_runbooks/RB-DPA-CHANGE.md) (DPA wind-down language).
> **Roadmap link.** [`../../ROADMAP-TO-GA.md`](../../ROADMAP-TO-GA.md) §8 (T+1d+ post-launch features).

---

## 1. Design principles

1. **One path, one screen each.** Five steps, never branching, never modal-stacked. The customer always knows where they are.
2. **Cancel-completion ≤ 3 clicks from anywhere.** A "Cancel subscription" entry-point in the admin-ui billing settings is one click; reaching the confirm button is two more.
3. **Retention offer surfaces once.** Not "are you sure?" three times. We show the offer at step 3 — they can accept, decline-and-continue, or back out. Declining is not punished with re-prompts.
4. **No dark patterns.** No grayed-out confirm button; no "to cancel, call this number"; no pre-checked "stay subscribed."
5. **Receipt is permanent.** A "cancellation receipt" page is reachable forever via a stable URL emailed at cancel-time — customers can return and verify the cancellation completed, the retention clock, and the re-activation steps.
6. **Audit-chain entry.** Cancellation emits a `corelink.billing.subscription.cancelled` audit-chain event (per the audit-chain semantics in `specs/03_architecture/canonical/audit_model.md`) — same tenant audit trail customers already query via S-05.

---

## 2. Five-step wireframe

Each step below uses ASCII to express layout because wireframes-as-images don't version-control well. The admin-ui owner translates these into the React component tree.

### Step 1 — Entry point (Billing settings)

```
+---------------------------------------------------------------+
|  CoreLink admin   /  Billing   /  Subscription                |
+---------------------------------------------------------------+
|                                                               |
|   Plan: Team ($199/mo)            [Manage plan ↗]             |
|   Renews: 2026-06-15                                          |
|   Seats: 12 / 50                                              |
|                                                               |
|   Payment method: Visa •••• 4242   [Update ↗]                 |
|                                                               |
|   ───────────────────────────────────────────────────────     |
|                                                               |
|   Danger zone                                                 |
|                                                               |
|   [ Cancel subscription ]                                     |
|                                                               |
+---------------------------------------------------------------+
```

- Entry-point is a single button under "Danger zone." Not hidden behind a settings sub-page.
- Requires `role = admin` per `crates/corelink-tier-selection/src/tenant.rs`. Non-admin users see a tooltip pointing to their admin.
- Click → Step 2.

### Step 2 — Why are you cancelling? (survey)

```
+---------------------------------------------------------------+
|  Before you go — one question                                 |
+---------------------------------------------------------------+
|                                                               |
|   We're sorry to see you cancel. Telling us why helps us      |
|   improve. You can skip this — it doesn't affect anything.    |
|                                                               |
|   ( ) Too expensive                                           |
|   ( ) Missing a feature we need                               |
|   ( ) Found a better alternative                              |
|   ( ) Project ended / no longer needed                        |
|   ( ) Performance / reliability                               |
|   ( ) Support issues                                          |
|   ( ) Other / prefer not to say                               |
|                                                               |
|   [ Optional text: tell us more, max 500 chars   ]            |
|                                                               |
|   [ Skip this ]                            [ Continue → ]     |
|                                                               |
+---------------------------------------------------------------+
```

- Survey answers feed CRM as the canonical "reason for cancellation" — drives quarterly review per [`quarterly-review-template.md`](./quarterly-review-template.md).
- "Skip this" is a real, equal-weight option. Skipping still continues to Step 3.
- Survey response is captured **before** the retention offer so the offer logic can be reason-aware. E.g. "Too expensive" routes to the discount-offer variant; "Missing a feature" routes to the roadmap-feature variant; "Project ended / no longer needed" routes to the no-offer variant (do not insult the customer by discounting a "we don't need it" answer).
- Click "Continue →" → Step 3.

### Step 3 — Retention offer (conditional)

The content of this screen depends on the Step 2 answer. Three offer variants + one no-offer variant.

**Variant A: "Too expensive" → discount offer**

```
+---------------------------------------------------------------+
|  We hear you on price                                         |
+---------------------------------------------------------------+
|                                                               |
|   One-time offer:                                             |
|   50% off Team for 3 months.                                  |
|   That's $99.50/mo instead of $199 through 2026-09-15.        |
|                                                               |
|   No catch:                                                   |
|   - Returns to $199 on month 4 — clearly communicated         |
|     30 days in advance.                                       |
|   - One-time only per tenant per calendar year.               |
|   - Cancellable any time during the 3-month window without    |
|     clawback.                                                 |
|                                                               |
|   [ ← Back ]    [ Continue to cancel ]    [ Accept offer ]    |
|                                                               |
+---------------------------------------------------------------+
```

**Variant B: "Missing a feature" → roadmap + Founder call**

```
+---------------------------------------------------------------+
|  What's missing?                                              |
+---------------------------------------------------------------+
|                                                               |
|   We anti-scoped a lot for GA to ship cleanly. Some of what   |
|   you need may be on the post-GA roadmap — or worth shipping  |
|   sooner if it'd save the relationship.                       |
|                                                               |
|   Offer:                                                      |
|   30-min call with our founder before you cancel. Specific    |
|   feedback on the roadmap. No commitment to stay.             |
|                                                               |
|   [ ← Back ]    [ Continue to cancel ]    [ Book the call ]   |
|                                                               |
+---------------------------------------------------------------+
```

**Variant C: "Performance / reliability" OR "Support issues" → SRE post-mortem**

```
+---------------------------------------------------------------+
|  We owe you a real answer                                     |
+---------------------------------------------------------------+
|                                                               |
|   If reliability or support didn't meet expectations, we      |
|   want to give you a post-mortem before you go — not a        |
|   retention pitch, an actual technical breakdown of what      |
|   happened from our side.                                     |
|                                                               |
|   Offer:                                                      |
|   30-min call with our on-call lead. You get a written        |
|   post-mortem; we earn the chance to make it right or         |
|   gracefully let you go.                                      |
|                                                               |
|   [ ← Back ]    [ Continue to cancel ]    [ Book the call ]   |
|                                                               |
+---------------------------------------------------------------+
```

**Variant D: "Project ended / no longer needed" → no offer, fast-path**

```
+---------------------------------------------------------------+
|  Got it — straightforward cancellation                        |
+---------------------------------------------------------------+
|                                                               |
|   No upsell, no save offer. We'll keep your data for 30 days  |
|   in case you come back for another project. After that it's  |
|   permanently deleted.                                        |
|                                                               |
|   [ ← Back ]    [ Continue to cancel ]                        |
|                                                               |
+---------------------------------------------------------------+
```

- **Critical UX rule:** every variant has a "Continue to cancel" button that is visually equal to the "Accept offer" / "Book the call" button. No graying, no smaller font, no "Are you sure?" interstitial.
- "Accept offer" → Step 5 (confirmation of acceptance, not cancellation).
- "Continue to cancel" → Step 4.

### Step 4 — Confirm cancellation

```
+---------------------------------------------------------------+
|  Confirm cancellation                                         |
+---------------------------------------------------------------+
|                                                               |
|   You're cancelling: Team tier on {tenant}                    |
|                                                               |
|   Effective date:    2026-06-15 (end of current billing       |
|                      period — you keep full access until      |
|                      then; you will not be charged again).    |
|                                                               |
|   Data retention:    30 days after effective date.            |
|                      Re-activation reverts to current state.  |
|                      After 30 days, data is permanently       |
|                      deleted per our DPA + privacy notice.    |
|                                                               |
|   Export your data:  [ Run corelink cas export ↗ ]            |
|                      Guide: /how-to/leave-corelink            |
|                                                               |
|   Audit chain:       This action will be logged with          |
|                      event id corelink.billing.subscription.  |
|                      cancelled at confirm time.               |
|                                                               |
|   [ ← Back ]                       [ Confirm cancellation ]   |
|                                                               |
+---------------------------------------------------------------+
```

- Confirm requires typing the tenant slug (`type {tenant_slug} to confirm`) — same pattern used for tenant-deletion. Prevents misclicks; not a dark pattern because the action is irreversible-ish (re-activation is possible but state changes accrue).
- Click "Confirm cancellation" → server emits `corelink.billing.subscription.cancelled` audit event, Stripe subscription is set to cancel-at-period-end, cancellation receipt is generated and emailed. → Step 5.

### Step 5 — Cancellation receipt + re-activation path

```
+---------------------------------------------------------------+
|  Cancellation confirmed                                       |
+---------------------------------------------------------------+
|                                                               |
|   You cancelled the Team subscription on {tenant}.            |
|   Cancellation id: cn_01HZ...                                 |
|                                                               |
|   Timeline:                                                   |
|   - 2026-05-15 (today)  Cancellation confirmed.               |
|   - 2026-06-15          Final billing period ends.            |
|                         Access ends. Data retention starts.   |
|   - 2026-07-15          Data permanently deleted.             |
|                                                               |
|   Receipt URL:                                                |
|   https://app.corelink.humangr.com/r/cn_01HZ...                       |
|   (Bookmark this — it's permanent and lists your re-          |
|    activation options.)                                       |
|                                                               |
|   What's next:                                                |
|   - Export your data now: [ Run corelink cas export ↗ ]       |
|   - Re-activate before 2026-07-15: [ Re-activate ↗ ]          |
|   - Permanent deletion request before retention window ends:  |
|     [ File DSR deletion request ↗ ]                           |
|                                                               |
|   We sent a copy of this receipt to {admin email}.            |
|                                                               |
+---------------------------------------------------------------+
```

- Receipt URL is **publicly addressable** (signed token in the path) — admin can revisit even without logging in. Server retains the receipt forever; cancellation history is a permanent record per audit-chain semantics.
- The receipt email links back to this same URL plus includes inline:
  - Cancellation id + timestamp
  - Effective end of access
  - Retention window expiry
  - Re-activation deeplink
  - DSR-erasure shortcut for customers who want deletion **before** the 30 d window (their right under GDPR Art. 17 / LGPD Art. 18).

---

## 3. Retention-offer matrix (Step 3 routing)

| Step 2 answer | Variant | Offer | Authority |
|---|---|---|---|
| Too expensive | A | 50% / 3 mo same-tier OR free downgrade to lower tier | AE pre-authorized |
| Missing a feature | B | 30-min Founder call | Founder availability |
| Found a better alternative | A or B (last_login-based) | 50% / 3 mo if low-info; Founder call if engaged tenant | AE / Founder mixed |
| Project ended / no longer needed | D | No offer | n/a |
| Performance / reliability | C | SRE post-mortem call | SRE on-call |
| Support issues | C | Founder takeover + post-mortem | Founder |
| Other / prefer not to say | A (default) | 50% / 3 mo | AE pre-authorized |
| Skipped survey | A (default) | 50% / 3 mo | AE pre-authorized |

- Discount amounts are bounded by the [`RETENTION-PLAYBOOK.md`](./RETENTION-PLAYBOOK.md) authority matrix. Variant A is the "default save"; AE has standing authorization.
- Variants B and C require a real human action (Founder availability, SRE availability) and degrade gracefully to a Calendly-style booking widget — no fake availability promises.

---

## 4. Post-cancel data retention policy

- **Retention window:** 30 calendar days from access-end date (not from cancel-confirm date).
- **What is retained:** all CAS blobs, action cache entries, audit chain entries, tenant config, seat list. Functionally a frozen snapshot — readable upon re-activation, not modifiable.
- **What is purged immediately on cancellation confirm:** payment method on file (Stripe payment-method detachment runs on `subscription.cancelled` webhook); active API tokens (revoked at access-end).
- **DSR-erasure shortcut:** customer may request earlier deletion via the DSR deletion pathway (see [`../../specs/_runbooks/RB-DSR-GDPR.md`](../../specs/_runbooks/RB-DSR-GDPR.md)). Earlier deletion fulfills GDPR Art. 17 / LGPD Art. 18 obligations and waives the 30 d re-activation window.
- **Final purge:** at 30 d + 0, scheduled job (cron `cleanup-cancelled-tenants.py` — to be implemented) emits a `corelink.tenant.purged` audit chain entry, removes blobs from R2, removes tenant config from primary DB, retains only a tombstone record (tenant_id, cancelled_at, purged_at) for billing forensics. Tombstone is itself purged after 7 years per accounting retention obligations.

---

## 5. Export-tools handoff

The cancellation flow surfaces `corelink cas export` at Steps 4 and 5. This is a **requirements list with per-item status** — two of the four are met today, two are OPEN and must not be described to a customer as shipped.

- **Pre-existing** at GA (the customer escape-hatch is a hard requirement — see [`../sales/OBJECTION-HANDLING.md`](../sales/OBJECTION-HANDLING.md) Obj-1). — **MET.** `corelink cas export --tenant me --out <dir>` is wired in the CLI (`tools/cli/src/commands/cas.rs`), as is the portability bundle `corelink tenant export --out <file>`.
- **Documented** with a worked example. — **PARTIALLY MET.** The customer-facing offboarding walk-through is `apps/docs/docs/how-to/leave-corelink.mdx` (slug `/how-to/leave-corelink`). There is **no** `apps/docs/docs/cli/cas-export` page — that path was cited here but never existed; a dedicated `cas export` reference page is OPEN.
- **Multi-destination** — a worked example per target: S3-compatible bucket, local filesystem, Docker registry. — **OPEN, 1 of 3 wired.** Only a **local directory** is implemented; any remote URI is rejected outright ("only a local directory is wired today. Direct-to-S3/GCS streaming needs an object-store client + credentials (flagged gap)" — `tools/cli/src/commands/cas.rs`). Pushing into a customer bucket is operator-assisted for now (and see the runbook gap flagged in `../sales/FAQ-MASTER.md` M2).
- **Egress-bounded** — the customer pays no marginal cost; we eat the egress per the COGS model in [`../sales/PRICING-WORKSHEET.md`](../sales/PRICING-WORKSHEET.md) because we have committed to no-cost departure. — **MET** (policy commitment, no tooling dependency).
- **Auditable** — the customer's own audit chain should be the receipt of what they took. — **PARTIALLY MET, and the named event does not exist.** There is no `corelink.cas.exported` event anywhere in the codebase; do not cite it. What an export *does* leave in the chain today is the per-operation trail the CLI's own reads produce — `corelink.cas.list.attempted` and `corelink.cas.read.attempted` / `.served` (slugs at `crates/corelink-handler-cas/src/audit.rs:61-62,70`; emitted at `crates/corelink-handler-cas/src/handler.rs:338,583`), because `cas export` is a client-side page-and-download loop over the normal CAS routes. A **single, first-class export-receipt event** is OPEN.

If the customer cancels without exporting, the 30 d retention window is the second chance — they can re-activate, export, and re-cancel cleanly.

---

## 6. Suspension vs cancellation — distinct paths

- **Cancellation** (this document) = customer-initiated, intentional, surveyed, offered-save, audit-logged, retention-windowed.
- **Suspension** = system-initiated when payment fails for 14+ days (per `crates/corelink-billing-webhooks`). Suspension is **reversible by updating payment method**; it does not trigger the cancellation flow, does not surface the retention offer, does not start the 30 d retention clock. Suspension converts to cancellation at day 60 of unresolved payment — at which point the cancellation receipt is generated automatically and emailed to admin.
- The suspension→auto-cancellation path is the **only** way to enter the cancellation flow without explicit customer action. All other paths require the admin to click the Step 1 button.

---

## 7. Re-activation path

- **Window:** 30 days from access-end.
- **Mechanism:** admin visits the receipt URL → clicks "Re-activate" → re-enters payment method → tenant state restores from the frozen snapshot. No data re-onboarding required.
- **Pricing:** re-activation reverts to the pre-cancellation tier at list price. Any retention discount accepted before cancellation does not carry over to re-activation (otherwise customers would cancel-then-reactivate to harvest discounts).
- **Audit trail:** re-activation emits `corelink.billing.subscription.reactivated` audit event with explicit reference to the prior `subscription.cancelled` event id.

---

## 8. Engineering hand-off checklist

- [ ] Admin-ui: 5 wireframe screens implemented per §2.
- [ ] Survey response captured in CRM with reason-code enumeration matching §3.
- [ ] Stripe subscription cancel-at-period-end wired to the confirm button.
- [ ] Cancellation receipt URL — signed token, permanent, publicly addressable.
- [ ] Email template: cancellation receipt (SES + sender domain per H-8).
- [ ] Audit-chain events: `corelink.billing.subscription.cancelled`, `corelink.tenant.purged`, `corelink.billing.subscription.reactivated`.
- [ ] Cron job `cleanup-cancelled-tenants.py` at 30 d + 0 with dry-run rehearsal.
- [ ] DSR-erasure shortcut routed to `specs/_runbooks/RB-DSR-GDPR.md` workflow.
- [ ] Re-activation deeplink in the receipt page + email.
- [ ] Tooltip + access control: non-admin users see locked Step 1 entry-point with admin handoff.
- [ ] Suspension→cancellation auto-path tested end-to-end (60 d payment-fail clock).

---

## Cross-references

- Signal catalog: [`CHURN-RISK-SIGNALS.md`](./CHURN-RISK-SIGNALS.md)
- Retention plays: [`RETENTION-PLAYBOOK.md`](./RETENTION-PLAYBOOK.md)
- Quarterly review: [`quarterly-review-template.md`](./quarterly-review-template.md)
- Internal ops runbook: [`../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md`](../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md)
- DSR (deletion) runbook: [`../../specs/_runbooks/RB-DSR-GDPR.md`](../../specs/_runbooks/RB-DSR-GDPR.md)
- DPA wind-down language: [`../../specs/_runbooks/RB-DPA-CHANGE.md`](../../specs/_runbooks/RB-DPA-CHANGE.md)
- Customer-facing playbook: [`../lighthouse-kit/CUSTOMER-PLAYBOOK.md`](../lighthouse-kit/CUSTOMER-PLAYBOOK.md)
- Pricing + COGS floor: [`../sales/PRICING-WORKSHEET.md`](../sales/PRICING-WORKSHEET.md)
- Roadmap: [`../../ROADMAP-TO-GA.md`](../../ROADMAP-TO-GA.md) §8
