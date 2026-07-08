# admin-ui — screen-by-screen audit + SOTA rebuild plan (2026-07-07)

Owner rated the app 1/10 across the board (placeholders, Linear standard not respected, missing
options). This is the code-grounded map from an 8-agent parallel audit + the frozen Linear contract
every build agent will transcribe. **Judgment (scope/contract) stays with the lead; agents execute.**

## The core reframe
"Placeholder" splits into **two very different problems**:
- **FE-fixable NOW** (no new backend): Linear violations (spacing / raw tables / bare headers /
  off-scale fonts), broken WIRING (token/locale not passed), a wrong-screen, and missing FE features
  that hit *existing* endpoints. This is most of the visible ugliness.
- **Backend-gated**: the DATA doesn't exist yet (BE-1/2/5/6/7/8/10/11). Screens honestly show
  EmptyStates because fabricating a metric is a locked owner mandate. These need BE work to be "full."

## Consolidated screen map (18 screens)

| # | Screen | Class | The real problem | FE-now / BE-gated | Effort |
|---|--------|-------|------------------|-------------------|--------|
| 1 | Home | BUILT-thin | cards 4px-cramped (lin-checklist misused); billing snapshot fetched but dropped; no deep-links | FE-now (ROI/activity BE-1/2/3) | M |
| 2 | Connect | BUILT-complete | 0px gap between cards (no rhythm); raw `<a><Button>`; no token-aware CTA | FE-now | S |
| 3 | Tokens | BUILT-complete core | missing rotate / expiry-TTL / prune-revoked; raw checkbox; last-used stub | FE-now (last-used BE-5, BYOK BE-8) | M |
| 4 | Usage | BUILT-thin | **dead** date-range control; no charts; savings/hit-rate EmptyState | BE-1/2/7 gated | L |
| 5 | Audit | BUILT-thin + orphan | **bug**: client `since/event_types` vs backend `from/to/kind` (filters dropped); `/audit/visualization` orphan raw-HTML page | FE-now | L |
| 6 | Runners | PLACEHOLDER-ish | no price/tiers/entitlement/runs; cramped cards | BE-10 gated | L |
| 7 | Workspaces | PLACEHOLDER | all prose, no list/CRUD/pin; cramped | BE-11 gated | L |
| 8 | Trust | BUILT-thin | only BYOK status live; no audit-export/DPA/subprocessors/attestation; cramped | FE-now (BYOK cfg BE-8) | M |
| 9 | Team | BUILT-thin | no change-role / last-active / pending-invite mgmt / suspend / seat-cap; bare header | FE-now (multi-member BE-6) | M |
| 10 | Billing | BUILT-thin | no plan ladder/comparison/downgrade/in-app-cancel/add-on mgmt; orphan PortalLauncher; hardcoded $ | FE-now (invoices/pay-method BE) | L |
| 11 | Settings | PLACEHOLDER | 3 of 4 cards are prose, zero controls; no profile/org/security/MFA/sessions/notif | FE-now + BE-7 (spend-cap) | L |
| 20 | Admin Audit | BUILT-complete | export-format toggle; drawer appends instead of overlay; raw checkbox | FE-now | S |
| 22 | Admin Tenants | BUILT-thin | deep-dive cards **never fetch** (permanently dead); no operator actions; no default list; "filter" doesn't exist | FE-now (endpoints exist) | M |
| 32 | Team Invite | BROKEN | **wrong screen** — it's a DPA gate for the inviter; hardcoded `tenantId=""` breaks its one action | FE + invite-accept BE | L |
| 40 | DSR Landing | BUILT-thin | bare 6-button menu; no identity/SLA/contact; off-scale fonts | FE-now | S |
| 41 | DSR Status | BROKEN | **token never passed** → empty/skeleton forever; raw unstyled tables | FE-now (Clerk token wiring) | L |
| 42 | Consent New | BUILT-thin | **wrong audience** — DPO/controller authoring form shown to end user; stubbed notice | FE + product decision | L |
| 43 | Consent Dashboard | BROKEN | locale-broken nav links (404); relative unauthed fetch; raw tables | FE-now | M |
| 50 | Pricing | complete except | **no bundle** (cache+runner combo); `mt-20`=80px off-grid | FE + money decision | S/M |

**Real bugs surfaced (fix regardless):** (5) audit filter param mismatch drops filters server-side;
(41) DSR status token never wired → dead page; (43) consent links drop `[locale]` → 404 + unauthed
relative fetch; (32) team-invite `tenantId=""`; (50) `mt-20` off-grid.

**Systemic:** deleting the generic `.cx-main table/h2/dl/ol` CSS in #651 (correct for the kit cascade)
left the raw-HTML screens (41/43, audit-visualization) unstyled — they must move onto the kit anyway.

---

## THE FROZEN LINEAR CONTRACT (agents transcribe this 1:1 — never restyle, never invent)

**Law:** `.lin-*` classes + `components/ui/linear/*` are frozen primitives. Tokens-only. No raw hex,
no off-grid spacing, **no font-weight > 560**. A primitive you need but don't have → **escalate to
lead**, never invent (AP-3). Data-honesty is locked: unwired field → EmptyState, never a fake `0`.

### Tokens (exact — copy)
- **Surface:** `--bg #08090a` · `--bg-2 #0d0e10` · `--panel rgba(255,255,255,.024)` · `--panel-2 rgba(255,255,255,.045)` · `--line rgba(255,255,255,.08)` · `--line-2 rgba(255,255,255,.12)`
- **Text (opacity ramp on #f7f8f8):** `--t1 #f7f8f8` · `--t2 .62` · `--t3 .54` (FROZEN at .54 — a11y AA; ux-plan's .44 is superseded) · `--t4 .30`
- **Semantic (status dots + destructive confirm ONLY):** `--ok #5bd8a6` · `--warn #e6c07b` · `--danger #f0787a` · `--danger-solid #e5484d` (dark ink #08090a on it, NOT white)
- **Radius:** chip 6 · control 8 · card 12 · modal 16
- **Spacing (ONLY legal steps):** `--s1..--s16` = 4,8,12,16,20,24,32,40,48,64px. **80px is illegal.**
- **Type:** Inter; weights **400/450/510/560 ONLY**; scale 12/500(mono-caps) · 13/500 · 14/400(default) · 16/400(body ≤66ch) · 15/560(card-title) · 18/560(h3) · 24/560(h2) · 24–28/560(page-h1); tracking headings −0.022em, body −0.006em, mono 0.
- **Layout:** card padding `--s5` (20px); content max-width 1040px; ≥8px within group, ≥24px between groups; **never >3 card columns**; focus ring 2px `--line-2` never removed.

### Components (import `@/components/ui/linear`) — 30 primitives, key APIs
`Button{variant:primary|ghost|danger, size:sm|md, loading, iconLeft, href, download}` ·
`Card{title, meta, actions, padded=true, hover}` · `Stat{label, value, sub, trend}` ·
`Gauge{label, value, max, unit, hint, warnAt=.7, dangerAt=.9}` · `Badge{tone, dot}` · `StatusDot{tone}` ·
`Pill` · `Callout{tone:info|warn|danger}` · `EmptyState{icon, title, body, cta}` ·
`Field{label, htmlFor, help}` · `Input/Select/Textarea` (native attrs) · `CopyField{value, label, redactAs}` ·
`Modal{open, onClose, title, footer}` · `ConfirmDialog{open, onClose, title, body, confirmLabel, danger, onConfirm}` ·
`Menu{trigger}+MenuItem+MenuSep` · `Segmented<T>{options, value, onChange}` · `Checklist{items}` ·
`InlineError{error, onRetry}` · `Skeleton` · `SnippetTabs{tabs}` · `CodeBlock` · `Tooltip/HelpPopover{label}` ·
`ToastProvider+useToast()` · `CommandPalette` · `ThemeToggle`.

### Lead-added kit primitives (2026-07-07 — use these, do not inline)
- **`.lin-t1 / .lin-t2 / .lin-t3 / .lin-t4`** — text-color utility classes for the opacity ramp. Put these on raw `<p>/<span>` body text inside kit surfaces instead of `style={{color:var(--t2)}}`.
- **`<Button href target rel>`** — the kit `Button` now accepts `target`/`rel` in the anchor (`href`) form. **External links (docs, dashboards) must use `target="_blank" rel="noopener noreferrer"`.**

### Vertical rhythm fix (the #1 recurring violation)
Stacked Cards MUST carry rhythm: `.lin-mt` (16px) related, `.lin-mt-lg` (24px) between groups.
**`.lin-checklist` (4px gap) is for checklist rows ONLY — never as a generic card vstack** (the bug on
Home/Trust/Workspaces/Connect). `.lin-card` has zero margin by design.

### Page shell
Dashboard screens: `.cx-shell` + `.cx-main`. Public: `PublicShell` (`.lin`). Header = page-h1 (24–28/560)
+ subtitle `--t2`. Never reintroduce generic `.cx-main` element selectors (white-on-white btn bug).

### Per-screen DoD (verify gate)
Kit primitives only (zero raw `<section>`/`<table>`/inline styles/`String(e)`); every jargon term has
HelpPopover/Callout; Skeleton+InlineError+EmptyState present; destructive action behind ConfirmDialog;
grep-clean of hex / off-grid px / weight>560; typecheck+lint clean; `data-testid`s preserved; **lead
visually reviews the screenshot.**

---

## Build sequencing (proposed)

**Shared-file freeze FIRST (lead, pre-dispatch — prevents the AP-1 race):** any new `lib/customer-client.ts`
methods + `lib/customer-types.ts` types are added as stubs by the lead and frozen; agents only READ them
and edit their own screen dir. `globals.css` is frozen (lead applies the `mt-20`→scale fix + any missing
primitive once, then read-only).

**Wave 1 — FE-now, pure disjoint (no backend, high visual payoff):** 2 Connect · 40 DSR-landing ·
20 Admin-audit · 1 Home · 8 Trust · 3 Tokens(FE parts) · 5 Audit(bug+visualization) · 22 Admin-tenants
(wire existing fetch) · 41 DSR-status(wire+reskin) · 43 Consent-dashboard(wire+reskin) · 10 Billing
(plan ladder) · 9 Team(FE parts) · 11 Settings(FE controls). Each = own screen dir → conflict-free.

**Wave 2 — decision-gated:** 32 Team-invite (needs invite-accept endpoint) · 50 Pricing bundle
(owner: real checkout SKU — backend + Stripe + pricing sign-off).

**CUT from launch (owner decision C, 2026-07-07):** 42 Consent-new + 43 Consent-dashboard. Already
unlinked from every nav surface (customer sidebar / ⌘K / public footers) — reachable only by direct
URL. Code kept, routes remain, NOT surfaced. The consent-api 404 shared-lib fix is dropped (not
launch-relevant). Consent capture revisited post-launch if sold as a compliance feature.

**Wave 3 — backend WPs (true SOTA, no debt) in parallel:** BE-1/2 usage metering · BE-5 last-used ·
BE-6 team multi-member · BE-7 $-ceiling · BE-8 BYOK self-serve · BE-10 runners entitlement/allowlist/runs ·
BE-11 workspaces list/CRUD. Then the FE for 4 Usage / 6 Runners / 7 Workspaces lights up against real data.

Cap ≤6–8 concurrent build agents; worktree isolation OR strictly disjoint screen dirs; rolling
merge-on-green; lead cold-verifies + screenshots each before merge.
