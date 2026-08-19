# Inc-2 — `/_internal/*` control-plane network lockdown (design)

**Status:** DRAFT — needs owner sign-off on the mechanism + a Cloudflare-dashboard
step before any prod change. **Author:** tech-lead session 2026-08-19.
**Origin:** 2026-08-19 `corelink-redteam-brutal` findings (the recurring theme
under both confirmed control-plane findings) + the standing `index.ts` SECURITY
NOTE (F4). Inc-1 (`quota_read` dedicated-only, #1183) already shipped; this is
the root-cause increment.

## 1. Problem

Every `/_internal/*` path is reachable from the **public internet**
(`corelink-api.humangr.com/*`, `corelink-oci.humangr.com/*`) and gated **only**
by an application-layer constant-time compare of the `x-corelink-internal-auth`
header (`worker/src/index.ts:1878-1909`). There is **no network-layer gate** — no
Cloudflare Access, no WAF allowlist, no mTLS (`index.ts:826-833`,
`internal_pat.rs:6-25`). Consequences:

- A single leaked shared/consumer secret is exploitable from anywhere on Earth
  (the red-team's confirmed MEDIUM dual-control + LOW quota findings both ride on
  this; the far higher-value `/_internal/pat/mint` — which mints **any-tenant
  PATs** — is on the same public surface).
- Every dedicated-key rotation / provisioning gap is a public-internet exposure
  window, not an internal one.

**Goal:** remove public reachability of `/_internal/*` so a leaked bearer is only
usable by a party that ALSO holds a network-layer credential (CF Access service
token) or is a same-account Worker (Service Binding). Defense-in-depth on top of
the per-consumer dedicated keys, not a replacement for them.

## 2. Consumer map (grounded)

From the worker edge (`worker/src/index.ts`; the edge is the public surface, so
this is the authoritative list for the lockdown). The catch-all
`if (path.startsWith("/_internal/"))` (`index.ts:834`) forwards the whole family
to the `_system` DO / container, re-signing with the resolved consumer key
(`index.ts:1878-1909`, forward block `~1943-2079`).

| Path | Consumer / key | Caller | Same-account? | Lockdown path |
|---|---|---|---|---|
| `/_internal/pat/mint` | pat_mint | signup-worker (**already via Service Binding `CORELINK_API_SVC`**), + worker-internal reuse | signup=yes | **Service Binding** (already there for signup); drop the public route |
| `/_internal/admin/*` (pilots) | admin (shared) | operator / curl | no (human) | **CF Access service token** (operator) |
| `/_internal/admin/public-mirror/promote` (F3.2) | admin (shared) | **operator (me)** | no (human) | **CF Access service token** — MUST stay operator-reachable |
| `/_internal/dsr/anchor` | dsr_anchor | githugr backend | no (other repo) | **CF Access service token** (githugr) |
| `/_internal/dsr/{erase,verify,access,portability,rectification}` | erase | signup-worker crons over **public `CORELINK_API_BASE`** | yes (Worker) | **Service Binding** (move crons off the public base) |
| `/_internal/cas/{t}/{h}/erase` | erase | internal DSR cascade | yes | Service Binding / stays behind the erase orchestrator |
| `/_internal/audit/drain` | erase | signup-worker cron over **public `CORELINK_API_BASE`** | yes (Worker) | **Service Binding** |
| `/_internal/tenant/{t}/quota` | quota_read (Inc-1: dedicated-only) | signup-worker + operator | mixed | Service Binding (signup) + CF Access (operator) |
| `/_internal/replication/*` | (worker-handled, `ReplicationCoordinatorDO`) | regional replica-workers | yes | already same-account; add binding gate |

**Separate family — NOT `/_internal/*` (no leading underscore), out of scope for
this lockdown but same public-exposure smell:** `/internal/v1/runner/{mint,revoke}`
(runner_mint, corelink-runners dispatcher), `/internal/v1/auth/introspect|resolve-tenant`
(fabric_introspect, **NO edge gate today**), `/internal/v1/billing/usage`
(billing_ingest), `/internal/v1/auth/{tenant/lookup,token-exchange,rotate}`
(githugr/clw). Track as **Inc-3** (same mechanism, but these are worker-handled
exact routes, not the container proxy — a distinct change).

> ⚠️ Recon caveat: the container-side route enumeration was read on a stale branch;
> the **worker-side** map above is what governs the public lockdown and is
> line-cited against `worker/src/index.ts` on `main`. Re-confirm the container
> route set on `main` before touching container code (none is planned for Inc-2).

## 3. Mechanism decision

Two caller classes ⇒ two mechanisms (compose, don't pick one):

1. **Same-account Workers → Service Bindings.** signup-worker (pat/mint already
   bound; move its DSR/audit crons off `CORELINK_API_BASE` onto a binding),
   replication workers. Worker-to-worker, no public hop, no shared secret on the
   wire at all. **Preferred** wherever the caller is a same-account Worker.
2. **External HTTP callers → Cloudflare Access service tokens.** githugr, clw,
   corelink-runners dispatcher, and the **operator** (me, for admin + mirror
   promote). CF Access sits in front of the `/_internal/*` route and requires a
   `CF-Access-Client-Id` + `CF-Access-Client-Secret` header pair (a service token
   minted in the CF Zero-Trust dashboard) BEFORE the request reaches the Worker.
   The existing `x-corelink-internal-auth` per-consumer gate stays underneath
   (defense-in-depth: network token AND app secret).

**Rejected:** WAF/IP allowlist (CF Worker egress + cron IPs are not stable →
fragile, fails closed unpredictably); mTLS (every external caller needs a client
cert — heavier than service tokens for no extra benefit here).

**Why not "just Service Bindings for everything":** githugr / clw / the runners
dispatcher / a human operator are not same-account Workers; a binding cannot
address them. CF Access service tokens are the standard CF-native answer for
"authenticated non-Worker caller of an internal endpoint."

## 4. Owner-required steps (cannot be done from code)

These need the operator's Cloudflare Zero-Trust dashboard / account admin:

1. **Create a CF Access application** scoped to `corelink-api.humangr.com/_internal/*`
   (and `corelink-oci.humangr.com/_internal/*` if it serves any) with a
   **service-token** policy.
2. **Mint service tokens** (one per external caller for least-privilege + clean
   revocation): `operator`, `githugr`, `clw`, `runners`. Hand each Client-Id/Secret
   to the respective caller's secret store.
3. Confirm the account tier supports CF Access service tokens (Zero Trust free
   tier includes a service-token quota; verify the count needed fits).

I will prepare everything else (code + wrangler + the caller-side header wiring in
repos I own) and stage it behind a flag so the cutover is a single coordinated
flip once the tokens exist.

## 5. Work-packages + sequencing (money/compliance-safe)

Ordered so **nothing that gates money (pat/mint) or compliance (erase) breaks**;
each WP is independently deployable and reversible.

- **WP-0 (prep, no behavior change):** add the CF Access application + service
  tokens (owner). No code. Verify a token can reach `/_internal/*` while the
  public path still works (Access in "audit/log" mode first, not "enforce").
- **WP-1 (bindings for same-account crons):** move signup-worker DSR/audit crons
  from `CORELINK_API_BASE` (public) to a Service Binding to the main worker.
  Deploy signup-worker; prove the crons still drain/erase. (corelink-server repo:
  none; signup-worker repo: the binding + call-site.) Reversible (revert the
  call-site).
- **WP-2 (external caller tokens wired):** teach githugr / clw / runners /
  operator tooling to send the CF Access service-token headers on their
  `/_internal/*` calls. Coordinate with those TLs (handoff docs in their repos per
  [[cross-tl-response-as-doc]]). Still non-enforcing.
- **WP-3 (ENFORCE + drop public route):** flip CF Access to enforce; remove the
  public `/_internal/*` route arm (or restrict it) so only Access-authenticated +
  bound callers get through. This is the actual lockdown. Do it region-by-region
  or behind a quick rollback (re-add the route). Prove: an unauthenticated
  `/_internal/pat/mint` from the public internet → CF Access challenge/403 (not the
  app-layer 401); every legit caller still 2xx.
- **WP-4 (verify + close):** re-run the red-team's control-plane probes from an
  un-tokened public vantage → all `/_internal/*` blocked at the network layer;
  operator mirror-promote + signup crons + githugr anchor + erase cascade all
  green. Update `secrets-checklist` + the `index.ts` SECURITY NOTE (F4 → closed).

## 6. Conflict-map / risks

- **F3.2 dependency:** I use `/_internal/admin/public-mirror/promote` for the
  cross-tenant cache. The operator service token MUST be able to reach it, or
  F3.2 mirror populates break. Explicitly in WP-2/WP-3 acceptance.
- **Money path:** `/_internal/pat/mint` — signup is already on a binding, but
  verify NO other public caller mints PATs before dropping the public route
  (WP-3 gate). A missed caller = signup/onboarding outage.
- **Compliance path:** erase/verify fan-out + audit drain are signup-worker crons
  on the public base — WP-1 must land + be proven BEFORE WP-3 drops the route, or
  GDPR erase + audit drain break.
- **Cross-repo coordination:** githugr, clw, corelink-runners each need their
  caller updated (WP-2). Their TLs must land those before WP-3 enforce. Sequencing
  gate: WP-3 only after every caller in WP-1/WP-2 is confirmed migrated.
- **Rollback:** each WP reverts independently; WP-3 rollback = re-add the public
  route / set Access to non-enforce.

## 7. Non-goals

- The no-underscore `/internal/v1/*` family (runner/fabric/billing/auth) — Inc-3,
  same mechanism, separate change.
- Per-tenant authz on the reads (defense-in-depth follow-up, not required to close
  the network exposure).
- Replacing the per-consumer dedicated keys — they stay as the app-layer gate
  under CF Access.

## 8. Open questions for the owner

1. **Mechanism OK?** CF Access service tokens (external) + Service Bindings
   (same-account). Any preference for a different CF primitive?
2. **CF config:** will you create the Access app + mint the 4 service tokens
   (operator/githugr/clw/runners)? I'll supply the exact scope + policy JSON.
3. **Cross-TL coordination:** OK for me to open handoff docs in githugr / clw /
   corelink-runners for their WP-2 caller updates, or do you want to drive that?
4. **Sequencing appetite:** all-at-once cutover vs region-by-region for WP-3.
