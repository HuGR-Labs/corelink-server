# CoreLink — Go-Live Acceptance Set (BOUNDED · FINAL)

**Date:** 2026-07-10 · **Owner:** stakeholder · **TL:** corelink-server
**Status:** 🚀 **LAUNCHED (2026-07-10).** The launch surface is code-complete and in
`main` (integration wave #720), and **deployed to prod** — image `3ba152f5-r1` serving,
live Clerk+Stripe bound. The operator switch (section B) is executed; only non-blocking
polish remains.

> **This document is the closed list.** It is deliberately bounded. New "findings"
> do not reopen it — they are triaged against the launch line below and default to
> **roadmap**, not launch. The structural gate `scripts/validate_docs_reality.py`
> (PR #705 / task #65) welds the docs↔code door shut so drift cannot silently
> manufacture new launch blockers again. **The flood is closed.**

---

## The single root cause (why it felt infinite)

The "enxurrada de pendências" was **one** problem wearing many hats: **product-docs,
marketing, SDKs and the CLI had drifted ahead of code that was already wired and
working** (cache routes live+tested, checkout works, erase live). Every re-audit
surfaced a *different face* of that same drift, so it read as an endless stream.
It was never endless — it was one gap, enumerated repeatedly. The integration wave
#720 closes the drift on both sides at once (code caught up where thin, docs made
honest where ahead), and the reality-gate keeps them married.

---

## LAUNCH LINE (the recommendation)

**LAUNCH = the cache + storage-governance product** (per `CLAUDE.md`). Auth = Clerk,
billing = Stripe. Everything below the line is **post-launch roadmap** and is
explicitly OUT of the go-live acceptance set — including runners self-serve
(expansion campaign #1, phase 3) and enterprise BYOK activation for real customers.

---

## A. LAUNCH — code-complete, in `main` (verify: it's merged, not promised)

| # | Capability | Proof (code, not comment) |
|---|---|---|
| A1 | Cache surfaces: native CAS/AC, Bazel REAPI v2, Turborepo, sccache/WebDAV, **+ stock-Bazel HTTP alias** | `routes/{bazel_v2,turbo_v8}.rs`, WebDAV; alias PR #709 |
| A2 | Checkout + billing (Clerk+Stripe), fail-closed price-map boot guard | `tier_select.rs`, signup-worker `stripe.ts`, guard #689 |
| A3 | Storage governance: per-tenant HMAC CAS, GDPR physical-erase seam (410 + bytes-gone) | `POST /_internal/cas/:tenant/:hash/erase` (live) |
| A4 | DSR self-service portal `/v1/privacy/dsr/*` + erasure attestation served | PR #717 (mount) + #712 (audit drain, fail-closed customer log) |
| A5 | $-ceiling recalibrated → effectively-unlimited default + per-tenant backstop | PR #719 (ADR-0068 reconcile) — fixes the clw 402 tripwire |
| A6 | Enterprise seams WIRED (activate at sale, no rebuild): BYOK activation writer, multi-region prod driver, OTel export, tenant bulk-export | PR #714 / #715 / #713 / #716 |
| A7 | Honest surface: onboarding recipes reach wired routes, marketing = present-vs-roadmap, CLI missing cmds built, real JS + Python SDKs, 501 stubs closed | PR #706 / #703 / #708 / #707 / #704 / #711 |
| A8 | Structural anti-drift gate (docs↔code) | `validate_docs_reality.py` #705 |

**Full per-capability BUILT/PARTIAL/ABSENT proof:** `docs/operator/CAPABILITY-INVENTORY.md`.

## B. LAUNCH — operator switch — ✅ DONE (2026-07-10)

The switch that was owner-only (CF secrets are write-only; live keys live in the
owner's dashboards) is **executed**:

1. ✅ **Clerk live keys** — bound on prod (`CLERK_SECRET_KEY`, `CLERK_WEBHOOK_SECRET`
   verified present via CF API; owner-confirmed the values are live).
2. ✅ **Stripe live keys + webhook** — `STRIPE_SECRET_KEY` + `STRIPE_WEBHOOK_SECRET`
   bound; the live "Corelink prd" webhook points at the signup-worker (downgrade
   authority). Owner-confirmed live. (See `LAUNCH-KEYS-CLERK-STRIPE.md`.)
3. ✅ **Wave deployed to prod** — image `3ba152f5-r1` built + pushed to all 5 CF
   Containers registries, 5 pins repinned (PR #723), `cf-deploy-prod` migrated D1
   (additive) + rolled the primary `prod` region; running image confirmed via the CF
   Containers API = `corelink-prod-corelinkserver-prod:3ba152f5-r1`; product health
   200 on api/signup/app. Fan-out to the 4 regional envs (sam/lhr/nrt/syd) follows.

**Go-live executed. B1–B3 done.** Residual operator polish (non-blocking): enable
SSL for the BetterStack status page (`status.humangr.com`) in the BetterStack console
(the one smoke [FAIL], a status page — not the product/money path).

---

## C. ROADMAP — post-launch, explicitly OUT of the launch set (do NOT reopen as blockers)

| Item | Why it's roadmap, not launch |
|---|---|
| Runners cold-signup (GitHub App 144561227: setup_url + creds + Install button) | Expansion #1 / phase 3 — a *feature evolution*, not the launch product. Chain is server-wired; residual is operator secrets. Task #68. |
| Enterprise BYOK activation for a real customer | Sold post-launch; owner owes the AWS IAM role (per enterprise-tier decision). Seam is wired (#714). |
| Multi-size runner ladder | Needs owner size-taxonomy + $/slot pricing decision. Driver seam agreed with runners TL. |
| Docs-site bundle-size 480→250 KB (R-S18-X, #53) | Docusaurus perf polish; red on `main` independently; docs build+deploy fine. |
| SOC2 | Needs an external auditor; parked by owner decision. |
| worker-vitest unit for runner-revoke owner_tenant-optional (#67) | Fast-follow test; behavior shipped + reconciled with runners contract (#718). |

---

## The commitment

The launch set is **A (done) + B (owner switch)**. It will not grow. Any future
"missing piece" is triaged against the launch line above; unless it breaks the
cache + storage-governance product for a self-serve SMB customer, it is roadmap.
The reality-gate enforces this mechanically. **Go/no-go is now a business decision
(flip the keys), not an engineering one.**
