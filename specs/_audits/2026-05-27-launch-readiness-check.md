---
id: "AUDIT-2026-05-27-LAUNCH-READINESS-CHECK"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "launch", "solo-startup", "readiness", "post-w36"]
---

# Launch readiness check (solo-startup gate)

> **Scope.** Read-only investigation of the five things a solo-founder needs working before public launch: landing page, signup flow, pricing page, privacy/terms, and customer pipeline. No code changes; this audit produces a gap report with concrete next-actions.

> **Method.** (a) static survey of `apps/docs/` and `apps/admin-ui/`, (b) curl smoke tests against the production hostnames recorded in `specs/_audits/sealed/2026-05-26-w32-phaseI-signoff.md`, (c) cross-reference Wave-32 phase audits for known deferred work.

> **Prod hostnames (per Wave-32 sign-off §3).** `corelink-docs.humangr.com`, `corelink-app.humangr.com`, `corelink-api.humangr.com`, `humangr.com`, `corelink-signup.humangr.com`. The spec-suggested `*.corelink.humangr.com` form (e.g. `app.corelink.humangr.com`) does **not** resolve — the actual DNS in Wave-32 used the flat `corelink-*` pattern. This audit uses the live names.

## §1 L1 — Landing page

- **Status:** YELLOW.
- **Code path:** `apps/docs/docs/index.mdx` (Docusaurus, `slug: /`, `routeBasePath: /` so this IS the site root); navbar + footer in `apps/docs/docusaurus.config.ts`.
- **Production URL:** `https://corelink-docs.humangr.com/` → **HTTP 200**.

### Findings
- The doc-site root is wired as the marketing page. Content above the fold is *documentation-shaped*, not marketing-shaped: H1 "CoreLink documentation", three-card nav (Get started in 10 min / API reference / Pricing), then a Diátaxis explanation paragraph.
- The "what is CoreLink" line is one sentence in the meta description and in body copy: "CoreLink is a multi-tenant content-addressable cache for build, package, and ML workloads on Cloudflare." Functional but not founder-grade positioning.
- **No primary signup CTA above the fold.** The three cards point to (1) quickstart docs, (2) API reference, (3) pricing. There is no "Sign up" / "Start free" / "Get a sandbox in 60 seconds" button. The closest CTA is the quickstart card.
- Body is Docusaurus SPA — server-rendered HTML at `/` is only ~5.4 KB and most copy hydrates client-side; the meta description is set ("CoreLink — multi-tenant content-addressable cache on Cloudflare").
- Footer correctly links Pricing, Tutorial, How-to, Reference, Security. Footer also links Privacy/Terms/Sub-processors — but those targets 404 (see §4).

### Recommendation
1. Add a hero block above the Diátaxis nav: H1 promise, one-line ICP, one-line outcome, single primary CTA. Single CTA target = `/pilot/apply` (current pilot form) or, post-launch, `https://corelink-app.humangr.com/sign-up`.
2. Keep the docs nav cards but demote them below the hero.
3. Validate first-render text (not JS-hydrated) so SEO + LLM crawlers see the value prop.

---

## §2 L2 — Signup flow

- **Status:** RED.
- **Code paths:**
  - Sign-up entry: `apps/admin-ui/src/app/sign-up/[[...sign-up]]/page.tsx` (Clerk `<SignUp />`).
  - Onboarding wizard: `apps/admin-ui/src/app/[locale]/onboarding/` — `tenant → region-plan → dpa → pat → billing → done`.
  - DPA accept: `apps/admin-ui/src/app/[locale]/onboarding/dpa/DpaStep.tsx` (+ scroll-tracking lib `src/lib/dpa-scroll.ts`).
  - Billing step: `apps/admin-ui/src/app/[locale]/onboarding/billing/BillingStep.tsx`.
  - Pilot intake (token-gated, separate path): `apps/docs/src/pages/pilot/apply.tsx` POST to `https://corelink-signup.humangr.com/v1/signup/pilot/{token}`.
- **Production URLs (live smoke 2026-05-27):**
  - `https://corelink-app.humangr.com/` → 200 but body is literal `"Not Found"` (`content-type: text/plain`); Pages project deployed, Next.js entry not serving SPA routes.
  - `https://corelink-app.humangr.com/sign-up` → **500**.
  - `https://corelink-app.humangr.com/en/onboarding` → **500**.
  - `https://corelink-app.humangr.com/en/customer` → **500**.
  - `https://corelink-signup.humangr.com/health` → 200 `{"status":"ok","env":"prod"}` (pilot-intake Worker is up).
  - `https://corelink-signup.humangr.com/` → 404 (expected — API surface, no root page).

### Findings
- The admin-ui code path is real and the flow design is correct (Clerk sign-up → locale router → tenant → region-plan → DPA accept with scroll-tracking → PAT generation → billing → done).
- The `/sign-up`, `/en/onboarding`, and every other `[locale]/*` route return **500** on prod. The Pages project deployed at Phase F (commit recorded in `2026-05-26-w32-phaseF-apply-admin-ui-closure.md`) but Phase F's own §G explicitly flagged "Apply Pages secrets (3x) post token rotation — **BLOCKING go-live**". Those secrets (`CLERK_SECRET_KEY`, `STRIPE_SECRET_KEY`, `RESEND_API_KEY`) were never applied per the trace I can see — and the sign-up page handler explicitly bails to a "Clerk publishable key not configured" placeholder when env is missing.
- Even if the 500s clear, the **billing step is a scaffold**, not real Stripe. `BillingStep.tsx:57` carries the inline comment: *"In production this is replaced with Stripe Elements bound to the setup intent client_secret. For the scaffold we accept the payment method ID directly so the flow is testable."* The form is literally a text input where the user is expected to paste a `payment_method_id`. No public-shippable signup can pass that step.
- The pilot intake at `/pilot/apply` is token-gated: it requires a 32-char Crockford base32 invitation token (validated client-side and re-validated by `corelink-signup.humangr.com`). It is a **closed pilot funnel**, not an open signup — fine for hand-picked design partners, not for "anyone can sign up".

### Recommendation
1. Apply the three blocking Pages secrets to `corelink-admin-ui` (Clerk, Stripe, Resend) — this is operator-only work, ~30 min. Re-smoke `/sign-up` and `/en/onboarding`.
2. Replace `BillingStep.tsx` scaffold with Stripe Elements bound to the SetupIntent. The server action `createSetupIntentAction` already exists; the client just needs `<Elements>` + `<PaymentElement>` instead of the raw text input. ~1 day effort.
3. Decide the launch funnel: (a) **closed pilot only** (current state once secrets are applied + 500s clear) — keep `/pilot/apply` as the single front door, drop the unfinished `/sign-up`+billing path; (b) **open signup** — finish Stripe Elements, gate trial vs. paid, wire webhook.
4. If choosing (b): document the end-to-end happy path as an audit doc + add Playwright coverage in `apps/admin-ui/playwright`.

---

## §3 L3 — Pricing page

- **Status:** YELLOW.
- **Code path:** `apps/docs/src/pages/pricing.tsx` (+ `pricing.module.css`); rate card data in `apps/docs/src/lib/pricing.ts`; calculator at `apps/docs/src/pages/pricing/calculator.tsx`.
- **Production URL:** `https://corelink-docs.humangr.com/pricing` → **HTTP 200**.

### Findings
- Five-tier page is live and renders the canonical Free / Starter / Team / Pro / Enterprise taxonomy with monthly/annual toggle.
- Headline prices are concrete numbers: Free $0, Starter $29, Team $149, Pro $549, Enterprise "Contact us". **However**, every tier in `apps/docs/src/lib/pricing.ts` carries `provisional: true`, and the file's docstring states the values are "TBD per GA pricing review — they are NOT a commitment." `CONTENT-REVIEW.md` confirms Finance + Legal + Product sign-off is `[ ]` pending across every pricing page.
- **No Stripe Checkout button.** Every tier's CTA renders `<a href="/pilot/apply">` (line 135 in `pricing.tsx`): "Start pilot" for paid tiers, "Contact sales" for Enterprise. That target requires an invitation token (see §2). There is no `stripe.checkout.sessions.create` call, no Checkout Session URL handoff, no Stripe-hosted page. Stripe is wired *post-signup* only, via the Customer Portal launcher at `apps/admin-ui/src/app/[locale]/customer/billing/PortalLauncher.tsx` (existing customers managing their subscription, not new customers paying).
- The Stripe products themselves: `STRIPE_SECRET_KEY` (test) was placed in `.env.local` per Wave-32 spec §1.4 but the prod Pages secret was never put (see §2 finding). Product/price IDs are not visible in the codebase — assume not provisioned.

### Recommendation
1. Decide whether `provisional: true` lifts before launch. If yes, push the four reviewers (Legal/Finance/Product + DPO where applicable) per `CONTENT-REVIEW.md`. If no, add a banner: "Prices are indicative for the design-partner program; final pricing locks at GA."
2. Pick a Stripe story for launch:
   - **Option A — Self-serve.** Provision Stripe products for Starter/Team/Pro, swap pricing CTAs from `/pilot/apply` to `stripe.checkout.sessions.create({ price: 'price_xxx' })` redirect, handle the success webhook in `corelink-signup.humangr.com` to provision the tenant.
   - **Option B — Sales-led.** Keep all CTAs pointing at `/pilot/apply` (manual invitation token issuance per pilot) and explicitly remove the "Start pilot" button from Free (it currently exists and is misleading — Free should be self-serve or removed from the page).
3. Either way: remove the per-tier `provisional: true` flag from the source-of-truth file once the decision is made, or mark the page `draft: true` so it doesn't claim commitments it doesn't have.

---

## §4 L4 — Privacy Policy + Terms

- **Status:** RED.
- **Code paths (admin-ui app, server-side):**
  - `apps/admin-ui/src/app/[locale]/legal/terms/page.tsx`
  - `apps/admin-ui/src/app/[locale]/legal/dpa/page.tsx`
  - `apps/admin-ui/src/app/[locale]/legal/cookies/page.tsx`
  - `apps/admin-ui/src/app/[locale]/privacy/page.tsx`
  - `apps/admin-ui/src/app/[locale]/privacy/sub-processors/page.tsx`
  - Content sources: `apps/admin-ui/src/content/{dpa,privacy-notice}.{en,pt,es,de}.{md,ts}`, `apps/admin-ui/src/content/sub-processors.{json,ts}`.
- **Code paths (docs site, expository):**
  - `apps/docs/docs/explanation/privacy/gdpr.mdx`, `apps/docs/docs/explanation/privacy/lgpd-full.mdx`
  - `apps/docs/docs/explanation/compliance/dpa.mdx`, `apps/docs/docs/explanation/compliance/sub-processors.mdx`
  - `apps/docs/src/pages/trust/index.tsx`, `apps/docs/src/pages/trust/sub-processor-register.tsx`
- **Footer wiring:** `apps/docs/docusaurus.config.ts:158-161` declares `Privacy → /legal/privacy`, `Terms → /legal/terms`, `Sub-processors → /legal/sub-processors`.
- **Production smoke (2026-05-27):**
  - `https://corelink-docs.humangr.com/legal/privacy` → **404**.
  - `https://corelink-docs.humangr.com/legal/terms` → **404**.
  - `https://corelink-docs.humangr.com/legal/sub-processors` → **404**.
  - `https://corelink-app.humangr.com/en/legal/terms` → **500** (same auth-misconfig blast radius as §2).
  - `https://corelink-app.humangr.com/en/legal/dpa` → **500**.
  - `https://corelink-app.humangr.com/en/privacy` → **500**.

### Findings
- The footer of the public marketing/doc site points at `/legal/privacy`, `/legal/terms`, `/legal/sub-processors` — and **all three 404 on production.** No Docusaurus page or redirect exists at those paths. The actual content lives in `apps/docs/docs/explanation/privacy/*.mdx` and `apps/docs/docs/explanation/compliance/*.mdx` (e.g. `/explanation/privacy/gdpr`, `/explanation/compliance/dpa`).
- The full legal corpus exists inside the **admin-ui app** (gated, internal-feeling URLs: `/en/legal/terms`, `/en/legal/dpa`, `/en/legal/cookies`, `/en/privacy`, `/en/privacy/sub-processors`). That is fine for in-product DPA accept during onboarding, but it is **not the right surface for the public-facing footer link** — a prospect browsing `corelink-docs.humangr.com` cannot reach Terms or Privacy from the footer today.
- Even when the admin-ui 500s clear, these are auth-gated routes under `[locale]/*` — Clerk middleware may bounce anonymous visitors. The legal pages must be publicly reachable (LGPD Art. 9, GDPR Art. 13/14 both require pre-collection disclosure).
- `CONTENT-REVIEW.md` shows Legal sign-off `[ ] pending` on every compliance + privacy doc.

### Recommendation
1. **Fastest fix (~2 hours, no code re-deploy).** Add three redirect entries in `apps/docs/docusaurus.config.ts` via `@docusaurus/plugin-client-redirects`:
   - `/legal/privacy` → `/explanation/privacy/gdpr` (or new combined page)
   - `/legal/terms` → new `/legal/terms` static page (must be authored — does not exist in docs)
   - `/legal/sub-processors` → `/explanation/compliance/sub-processors`
2. **Proper fix.** Author canonical `apps/docs/src/pages/legal/{privacy,terms,sub-processors}.tsx` (or `.mdx`) drawing from the admin-ui content files as the single source of truth; mirror to the admin-ui in-product copy. Confirm Legal sign-off lifts `draft: true` per `CONTENT-REVIEW.md`.
3. Confirm the admin-ui `[locale]/legal/*` and `[locale]/privacy` routes are reachable while **unauthenticated** (`middleware.ts` should allowlist them).

---

## §5 L5 — Customer pipeline

- **Status:** NEEDS-GUSTAVO-INPUT.
- **Why not auditable from code:** prospects, design partners, and warm intros are not artifacts in the repo; the pilot intake at `corelink-signup.humangr.com/v1/signup/pilot/{token}` issues tokens manually per Wave-29 stream-2 design.

### Recommendation
Pick a single launch channel and commit one week to it. In rough order of fit for a solo founder shipping CoreLink:
1. **Show HN.** Day-of-launch post linking the landing + pricing + a 60-second video. Pre-warm 5–10 design partners so the post has comments in the first 30 minutes.
2. **Cold list of 50 named CI/ML platform leads** (Bazel/Buck adopters, Modal/Replicate/Anyscale infra leads, internal-platform leads at orgs running multi-region Cloudflare). One-line value prop + offer of a free pilot token. Target: 3-5 design partners.
3. **Twitter/X + r/devops + Hacker News "Ask HN: which CI cache do you use?"** — once design partners exist and there is a testimonial.
4. Avoid spreading thin across LinkedIn cold outbound, Reddit DMs, etc. before the product clears its own 500s.

---

## §6 Overall verdict

- **1 / 5 GREEN.** None fully — the closest is L1 (landing page live but missing a real signup CTA).
- **2 / 5 YELLOW** (fixable by agent or ≤1-day Gustavo effort): L1 landing CTA, L3 pricing-CTA-and-provisional-flag.
- **2 / 5 RED**: L2 signup flow (500s on every route + Stripe step is a scaffold) and L4 privacy/terms (footer links 404 on prod).
- **1 / 5 NEEDS-GUSTAVO-INPUT**: L5 customer pipeline.

The Wave-32 deploy is SEALED with health-check endpoints green, but the user-facing application surface is not actually usable: a prospect cannot read Terms, cannot click a real Sign-up CTA on the landing page, cannot pay through Stripe Checkout from the pricing page, and cannot complete onboarding because the admin-ui returns 500 for every route gated by `[locale]/*`. The blocker that Phase F flagged ("Apply Pages secrets — BLOCKING go-live") is still open.

## §7 Recommended next-action sequence

Ordered so that each step unblocks the next:

1. **(operator, ~30 min)** Apply the three Pages secrets to `corelink-admin-ui` per `specs/_audits/sealed/2026-05-26-w32-phaseF-apply-admin-ui-closure.md` §G: `CLERK_SECRET_KEY`, `STRIPE_SECRET_KEY`, `RESEND_API_KEY`. Re-deploy if Pages requires it.
2. **(operator, ~15 min)** Re-smoke `https://corelink-app.humangr.com/sign-up`, `/en/onboarding`, `/en/legal/terms`, `/en/legal/dpa`, `/en/privacy`. Confirm all return 200. If still 500, capture Pages logs and triage before continuing.
3. **(code, ~2 h)** Author/redirect the three `/legal/*` paths on `corelink-docs.humangr.com`: add `@docusaurus/plugin-client-redirects` config or create `apps/docs/src/pages/legal/{privacy,terms,sub-processors}.tsx`. Re-smoke footer links return 200. Unblocks L4.
4. **(code, ~1 day)** Replace the `BillingStep.tsx` scaffold with Stripe `<Elements>` + `<PaymentElement>` bound to the existing SetupIntent action. Manual end-to-end test with a Stripe test card. Unblocks L2 for self-serve.
5. **(code, ~2 h)** Add a hero block + single primary CTA to `apps/docs/docs/index.mdx` (or convert root to a `pages/index.tsx`). CTA target depends on launch funnel decision (sales-led `/pilot/apply` vs. self-serve `https://corelink-app.humangr.com/sign-up`). Unblocks L1.
6. **(decision, ~30 min)** Pick sales-led vs. self-serve for L3 pricing CTAs. If self-serve: provision Stripe products + swap CTAs to Checkout Session redirect. If sales-led: remove "Start pilot" from Free tier card. Lift `provisional: true` after Finance sign-off in `CONTENT-REVIEW.md`.
7. **(Gustavo, async)** Pick a launch channel per §5 and queue the first 5–10 design partners.
8. **(verification, ~1 h)** Re-run this audit end-to-end (full curl smoke + footer crawl + signup happy-path manual click) before any public-facing announcement.

## §8 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
