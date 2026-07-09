# DISPATCH → corelink-server TL — P0-O3: enterprise-claims audit against the Cache/API surface

> **From:** clw TL (coordinator, O3 owner-delegated) · **To:** corelink-server TL · **cc** owner · **Date:** 2026-07-01
> **Re:** P0-O3 — "descope unbuilt enterprise claims" (audit's flagged **#1 deal-killer / liability**). The
> go-live roadmap flagged the Cache surface as **"over-claiming enterprise"** (roadmap:48; your DD-RESPONSE:76).
> I ran the audit on the clw repo → **O3-CLEAN**; here's the exact standard to run on your surface + confirm.

## The bar (same as I applied to clw): assertion-of-present-capability = liability; conditional = SAFE
Flag only claims that a capability is **available now** when it isn't. Five classes:

1. **BYOK / customer-managed keys** — reality: Cloudflare-provider-managed, no BYOK. (If any API/pricing/DD
   material says "your own encryption key" → FLAG.)
2. **SOC 2 / ISO 27001 / PCI / HIPAA certified now** — reality: not certified; "report once obtained" is safe.
   Given your DD-RESPONSE already engaged this, just **confirm** no material asserts current certification.
3. **Customer-selectable / multi-region residency available** — reality: US-only (`enam`), selection not
   available. Flag any pricing/enterprise/API-doc "choose region" / "EU residency" promise.
4. **Immutable / WORM audit** as delivered — reality: R2 Object-Lock **deferred** (your A4 tracks it); only
   keyed/signed head exists. Flag any "immutable/WORM/SOC2-grade audit" claim; keep it conditional until
   Object-Lock lands.
5. **Uptime SLA** (99.9% etc.) as contractual — reality: no committed SLA at v1.

## Where to look (your surface)
API reference / developer docs, pricing & plan pages you own, the enterprise/DD packet, any status-page or
security-page copy, and the signup/billing worker's user-visible strings (note: O6 signup/legal 500s + the
"get-corelink/marketing worker" may live in your domain — sweep those too).

## What I need back
Per class (1–5): **CLEAN** (cite the honest/conditional wording) or **OVERCLAIM** (exact location + wording +
fix). Since your DD-RESPONSE:76 already touched enterprise claims, a fast confirm-with-cites is fine where
it's already descoped — I just need the explicit sign-off so I can close O3 for the whole stack. Launch-gating.

Cross-ref: `corelink-workspaces/docs/GO-LIVE-STATUS.md` (P0-O3) — the clw-repo clean result + the standard.

— clw coordinator
