# Edge security, per-plane (SOTA) — machine API vs human web

**Problem (found 2026-07-10, proven from a datacenter IP):** the zone `humangr.com`
runs **free Bot Fight Mode** (`fight_mode: true`), which issues a **browser managed
challenge** zone-wide. That is categorically wrong for the **machine API**
(`corelink-api.humangr.com`, PAT-authenticated) — a real customer's CI (GitHub
Actions etc., datacenter IPs) gets **HTTP 403 "Just a moment…"** instead of cache
hits. Free BFM is **not scopable** (can't be skipped per-host by a WAF rule — verified:
a `skip` rule with `phases:[http_request_sbfm]` + `products:[securityLevel,…]` had
**no effect**). Dogfood missed it because runners egress from **inside** Cloudflare
(CF Containers) and residential curl isn't challenged.

## The SOTA model — security shaped to the traffic type, per surface

### Plane 1 — machine API (`corelink-api.humangr.com`): NO browser challenge, ever
Security is app-layer + rate control (never a JS challenge):
- PAT/HMAC auth, per-tenant scoping — **already built**.
- Per-tenant quota + `$`-ceiling (ADR-0068) — **already built**.
- Container `ratelimit_layer` — **already built**.
- **+ Cloudflare Rate Limiting rule** on the API host (volumetric anti-DoS floor,
  generous, per-IP) — edge-sheds floods before they reach the Worker. Defense-in-depth,
  NOT the per-tenant control (that's app-layer).

### Plane 2 — human web (`corelink-app` / `corelink-docs` / marketing): keep bot protection, with the *scopable* tool
- **Super Bot Fight Mode** (Pro+) challenging bots **on the HTML hosts only**.
- The **skip rule already created** (`http.host eq "corelink-api.humangr.com"`,
  action `skip`, ruleset `0ba65fb65f0145bb95f475b8288dafed`, rule
  `75f751372c8f4b13b935390385b75057`) exempts the API plane — correct + active under SBFM.
- (Later polish: Turnstile invisible on signup/login forms.)

### The enabler: **Pro plan** (~$20/mo)
Super Bot Fight Mode (scopable), WAF managed rules, and real rate-limiting exist only
on **Pro+**. Free BFM is on/off-only. Pro is trivial for a $30/mo SaaS and unlocks the
*correct* posture instead of "turn all protection off".

## Execution split
- **Owner (billing — the only gated step):** upgrade `humangr.com` → **Pro** in the CF
  dashboard (Membership/Plans → Pro).
- **TL (has WAF write; runs the moment Pro is live):**
  1. Enable Super Bot Fight Mode: challenge *definitely-automated*, allow *verified bots*,
     JS detection on — scoped to HTML hosts (the API is skip-exempt via the existing rule).
  2. Author the **Rate Limiting rule** on `corelink-api` (per-IP, generous e.g. block
     > ~5k req/min/IP — tuned to not clip legit heavy CI; the real per-tenant limit is
     app-layer).
  3. Turn **off** free `fight_mode` (superseded by SBFM).
  4. **Verify from a datacenter IP** (the `diag/cas-datacenter-probe` GH-Actions job):
     PUT → 201, GET → 200, bytes identical. And confirm an HTML host still challenges a
     scripted bot request.

## Bridge (only if the product must be unblocked BEFORE the Pro upgrade)
Turning free BFM off alone is a **bandaid** (removes protection, replaces nothing) — do it
ONLY paired with the API Rate Limiting rule, and re-protect HTML with SBFM the moment Pro
lands. Prefer: upgrade Pro first, flip everything at once, zero-compromise.
