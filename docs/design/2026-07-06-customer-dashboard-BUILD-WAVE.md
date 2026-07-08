# Customer Dashboard — Build Wave Plan (frozen-contract fan-out)

> Companion to `2026-07-06-customer-dashboard-ux-plan.md`. This is the **techlead decomposition**:
> what the orchestrator freezes, the disjoint agent WPs, the conflict-map, and the rigor gates — so
> ~8 agents build in parallel with **zero decisions and zero conflicts**. Review this BEFORE fan-out.

## Review refinements (added for SOTA + to prevent rework)
1. **⌘K command palette** (Linear signature) — jump to any page/action/token. Part of the shell.
2. **Theme toggle** (dark/light) — reuse the humangr.com system; consistency across the family.
3. **Usage alerts** — threshold notifications at 70%/100% of any cap (in-app; email later = BE WP).
4. **Keyboard + focus** — full keyboard nav, visible focus ring (a11y, Lighthouse-safe).
5. **The exact-contract rule** — every primitive's prop API is frozen by the orchestrator (§2). An
   agent that needs a primitive not in the kit **escalates to the lead** (who adds it to the frozen
   kit) — it NEVER invents one (AP-3). This is the anti-drift spine.
6. **Per-screen data-truth** — each agent is told exactly which fields are `[live] / [stub] / [empty]`
   (§3 table). No agent fabricates a number; stub fields render a teaching EmptyState, never a `0`.

## GO / NO-GO — HYBRID (sequential scaffold → parallel screens)
- **Disjointness:** achievable **only after** the scaffold is frozen. The shared files every screen
  would otherwise touch — `globals.css`(tokens), the primitive kit, `customer-types.ts`,
  `customer-client.ts`, `CustomerNav.tsx`, `layout.tsx` — are **all built + frozen by the lead first**.
  After that, each screen-agent writes ONLY its own two files → CONFLICT-FREE.
- **Verdict:** **W0 (scaffold) runs SEQUENTIAL, by the lead.** Then **W1–W8 (screens) run PARALLEL**
  (8 agents). Then a later **backend wave** (BE WPs, gated) — NOT in this fan-out.

---

## W0 — THE FROZEN CONTRACT (lead builds this first; agents only consume it)
One branch, lead-owned. Nothing fans out until W0 is merged + tagged as the baseline.

**W0.a Tokens** — `apps/admin-ui/src/app/globals.css` (+ optional Tailwind theme): Part-0 constitution,
scoped to `.cx-shell`, dark+light. Frozen var names are the ONLY palette/scale.

**W0.b Primitive kit** — `apps/admin-ui/src/components/ui/*` (new dir), each a typed component with a
**frozen prop API**. Agents import from `@/components/ui`. The contract (illustrative — the real W0
freezes all with full TS types + stories):
```ts
Button({variant:'primary'|'ghost'|'danger', size?:'sm'|'md', loading?, iconLeft?, ...}) 
Card({title?, meta?, actions?, padded?, children})            // the glass card
Stat({label, value, sub?, trend?})                            // big metric
Gauge({label, value, max, unit, hint?, dangerAt?})            // quota bar + "% of cap"
Table<T>({columns, rows, empty:<EmptyState/>, loading?})      // sortable, empty/loading slots
Badge({tone:'neutral'|'success'|'warn'|'danger', children}) · StatusDot({tone})
CopyField({value, redactAs?, label?})                         // one-click copy, redaction-safe
SnippetTabs({tabs:[{id,label,code}]})                         // per-surface config, copy per tab
HelpPopover({children})  ·  Tooltip({label, children})        // the "what is this?" layer
Modal({open,onClose,title,children}) · Sheet(...) 
ConfirmDialog({title, body, confirmLabel, danger?, onConfirm})// every destructive action
Toast (via useToast()) · EmptyState({icon,title,body,cta?})
Skeleton({rows?|variant}) · InlineError({error, onRetry})     // replace loading… / String(e)
Callout({tone, children}) · Checklist({items:[{done,label,cta?}]}) · Segmented · Breadcrumb
CommandPalette (⌘K, global) · AccountMenu · ThemeToggle
```
> Freeze rule: props above are the contract. Screen-agents pass data in; they do not restyle or
> reshape a primitive. Missing prop → escalate to lead.

**W0.c Shared types** — extend `apps/admin-ui/src/lib/customer-types.ts` with every field the screens
render, each tagged `[live]/[stub]/[empty]` in a comment mirroring `customer_d1.rs` truth.

**W0.d Client API** — `apps/admin-ui/src/lib/customer-client.ts`: freeze ALL wrappers incl. the
currently-missing `removeTeamMember` (DELETE …/team/:id) and `deleteAccount` (POST …/account/delete),
plus typed stubs for not-yet-wired reads (byok-config, hit-rate) that throw `NOT_WIRED` so screens
show the honest empty-state, not a crash.

**W0.e Nav + shell** — `CustomerNav.tsx` (all grouped entries from UX-plan §2.1, icons, active state)
+ `customer/layout.tsx` (top bar: tenant/region/docs/⌘K/help/AccountMenu/ThemeToggle + grouped sidebar
+ global error boundary). `data-testid`s preserved.

W0 DoD: kit renders in a `/customer/_kit` dev page (Storybook-lite), typecheck clean, Lighthouse a11y
100 on public pages (theme scoped to .cx-shell), all `data-testid`s intact, nav reaches every screen.

---

## W1–W8 — SCREEN AGENTS (parallel; each owns EXACTLY two files, consumes W0 only)
Each agent: reads the UX-plan screen spec + W0 kit/types/client; writes ONLY its owned files; uses
ONLY kit primitives + frozen client fns; honors the data-truth; returns the compact card.

| WP | Screen | Owner files (disjoint) | Data-truth (what's real) | Key spec |
|----|--------|------------------------|--------------------------|----------|
| **W1** | Home / onboarding checklist | `components/customer/HomeClient.tsx`, `customer/page.tsx` | plan/usage/byok [live]; activity [stub→empty] | Checklist(DPA→PAT→connect→first-hit) + ROI hero |
| **W2** | Connect a tool | `components/customer/ConnectClient.tsx`, `customer/connect/page.tsx` (new route) | endpoints [live]; token via kit | SnippetTabs (Bazel/Turbo/sccache/npm/pip/CAS) env-safe + "Test connection" |
| **W3** | Tokens | `components/customer/KeysClient.tsx` (rewrite), `customer/keys/page.tsx` | PAT CRUD [live] | PatModal on create, scope HelpPopovers, ConfirmDialog revoke, EmptyState |
| **W4** | Usage & savings | `components/customer/UsageClient.tsx` (rewrite), `customer/usage/page.tsx` | cas_bytes/quota [live]; reads/writes/daily/hit-rate [stub→teaching empty] | 3 Gauges (req/storage/$-ceiling read-only) + hero slot + honest empties |
| **W5** | Audit + verifier | `components/customer/AuditClient.tsx` (rewrite), `customer/audit/page.tsx` | flat list [live]; chain verifier [mock→feature-flag] | Filters+pagination+labels; embed the verifier via CustomerClient (flag if routes mock) |
| **W6** | Team | `components/customer/TeamClient.tsx` (rewrite), `customer/team/page.tsx` | list/invite/remove [live] | role HelpPopovers, remove+ConfirmDialog("revokes N PATs"), seats-vs-cap, invite mgmt |
| **W7** | Plan & billing | `components/customer/BillingClient.tsx` (rewrite), `customer/billing/page.tsx` | status/plan/portal/checkout [live]; invoices/pm [stub→empty] | two-axis plan card, ONE portal + ONE upgrade, break-even, humanized enums |
| **W8** | Trust & Settings | `components/customer/TrustClient.tsx`+`SettingsClient.tsx`, `customer/trust/page.tsx`+`customer/settings/page.tsx` (new) | byok status [live], DSR [live], account-delete [live]; byok-config/region [not-wired→empty] | BYOK status+DSR surfaced+region+$-ceiling(read)+Danger zone(account delete, MFA, Confirm) |

CONFLICT-MAP: **CONFLICT-FREE** — every shared file is frozen in W0; W1–W8 write only their own two
files (all in distinct paths). No agent edits nav/client/types/tokens/kit.

RETURN-SHAPE (each agent): `WP<n> DONE · files:<paths> · uses-kit:<list> · client-fns:<list> ·
data-truth-honored:Y · typecheck:Y · testids-preserved:Y · escalations:<none|list> · screenshot-note:<1 line>`

DoD (V1, every screen): (1) renders with W0 primitives ONLY — zero raw `<section>`/`loading…`/`String(e)`
/inline styles; (2) every jargon term has a HelpPopover/Callout; (3) Skeleton+InlineError+EmptyState
present; (4) every destructive action behind ConfirmDialog; (5) stub fields → teaching empty, never a
fake number; (6) typecheck clean; (7) `data-testid`s preserved; (8) tokens-only (grep: no hex, no px
outside the 4-grid, no font-weight>560).

MERGE ORDER: W0 → (W1..W8 any order, CONFLICT-FREE) → lead cold-verifies each vs DoD → squash-free
merge. Lighthouse a11y must stay green on public pages (theme scoped).

---

## OWNER DECISIONS (locked 2026-07-06)
1. **Data-honesty CONFIRMED as a real gap** (verified in `customer_d1.rs:7-24` — the "HONEST v1
   contract": stub fields return `[]`/`0`/`501` deliberately, **no fabricated data was ever shipped**).
   Per Owner: this is a strategic gap the **TL owns closing** with SOTA rigor → the backend WPs below
   are a **committed backlog I attack**, not "someday". FE wave ships now with teaching empty-states;
   backend fills them.
2. **Runners + Workspaces:** build the **full screens now** → **W9 (Runners) + W10 (Workspaces)** added
   to the parallel wave (10 screen-agents). Their empty/coming states are honest until BE-7/8 land.
3. **Wave-1 = FE-only** (W0–W10) + **⌘K palette + theme toggle + quota alerts** in W0.

## BACKEND BACKLOG — committed, TL-owned, SOTA (closes the confirmed stub gap)
Prioritized; each is its own PR, gated on real schema, no hot-path regression, no faked data.
- **BE-1 usage metering** — per-op reads/writes + per-day rollup table (today `reads/writes=0,daily=[]`).
  ⚠ hot-path: counter writes on CAS read/write — must be async/batched (D1 write budget), zero added
  latency to the cache path. Highest value (Usage page's core).
- **BE-2 cache-hit-rate** — hit/miss counters → the hero ROI metric (today: missing).
- **BE-3 recent_activity** — an activity feed (reuse/extend `customer_audit_events` 0077).
- **BE-4 billing invoices + payment_method** — from Stripe (list invoices; emit PM) (today `[]`/absent).
- **BE-5 PAT last_used** — stamp on auth (today `None`).
- **BE-6 real team list** — read `team_member` rows (today only a synthesized Owner).
- **BE-7 $-ceiling read/update** endpoint (gate exists; no customer read/write) + $-saved + dedup savings.
- **BE-8 BYOK self-serve** (`POST /v1/customer/byok`, 4-KMS) · **BE-9 real audit-chain routes**
  (de-mock W5) · **BE-10 runners** entitlement/consumed-vCPU-h/repo-allowlist reads · **BE-11 workspaces**.
Sequencing: BE-1/BE-2 (the ROI core) first; BE-3..6 (quick, table/Stripe reads) next; BE-7..11 with
their FE screens. Each BE-n unblocks the "teaching empty-state" on its FE screen → real data.

## Decisions I need from the Owner before freezing W0 (to avoid rework)
1. **Data-honesty policy** — confirm: ship screens NOW with teaching empty-states for stub/not-wired
   fields, and land the BE WPs behind them (vs. blocking those screens). *(Recommend: ship + empties.)*
2. **Wave-1 scope** — confirm FE-only (W0–W8) first, backend wave after. *(Recommend: yes.)*
3. **Add ⌘K palette + theme toggle + alerts** to W0? *(Recommend: yes — Linear-grade.)*
4. **Runners/Workspaces** — full screens now (mostly empty pending BE-7/8) or defer to the backend
   wave? *(Recommend: defer the full screens; keep a nav entry + "coming" state.)*
