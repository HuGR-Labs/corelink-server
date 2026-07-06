# CoreLink Customer Dashboard — UX Study & Decomposed Plan

> Goal: turn the current customer dashboard (functional but "ugly, cramped, unclear, no
> explanations, useless") into a **foderástico, hyper-complete, impeccable** self-serve
> control plane — with the **Linear design doctrine respected in full** (spacing, type,
> color, limits, components — nothing ad-hoc).
>
> Grounded in three code-level research passes (product/JTBD, current-dashboard audit, data/API
> surface). Every claim traces to a file/endpoint. Status tags: **[live]** wired to real D1,
> **[stub]** UI renders but prod returns 0/[]/empty, **[built-unwired]** component exists but
> disconnected, **[missing]** no surface at all.

---

## PART 0 — DESIGN CONSTITUTION (Linear doctrine — non-negotiable, referenced by every screen)

All values below are **the only allowed values**. No screen invents a size, weight, or color.
Encoded once as CSS custom properties / a Tailwind theme; every component consumes tokens.

### 0.1 Color (monochrome, opacity-based — no hue)
```
--bg        #08090a          page
--bg-2      #0d0e10          raised surface (rare)
--panel     rgba(255,255,255,.024)   card fill (glass)
--panel-2   rgba(255,255,255,.045)   card hover / selected
--line      rgba(255,255,255,.08)    hairline border
--line-2    rgba(255,255,255,.12)    stronger border / focus
--t1        #f7f8f8          primary text
--t2        rgba(247,248,248,.62)    secondary / body
--t3        rgba(247,248,248,.44)    tertiary / labels
--t4        rgba(247,248,248,.30)    faint / disabled
white       #f7f8f8          the ONLY "accent" (primary button, active states)
semantic    success/danger used ONLY on status dots + destructive confirms, never as decoration
```
Light theme = the same via `.light` overrides (bg #fbfbfa, text #131316, dark-on-light borders).
Terminals / code blocks stay **dark** in both themes (Linear/Raycast rule).

### 0.2 Type (Inter; fixed scale in px @16 root)
```
micro   12 / 500  mono, uppercase labels, table headers, pills
small   13 / 500  meta, secondary
base    14 / 400  UI default, body in dense areas
body    16 / 400  paragraph copy (line-length ≤ 66ch)
h4/card 15 / 560  card titles
h3      18 / 560
h2      24 / 560  section
page-h1 24–28 / 560  page title (dashboard pages are compact, not marketing)
tracking headings −0.022em; body −0.006em; mono 0
```
Weights allowed: **400, 450, 510, 560** only. Never 700/800 inside the app.

### 0.3 Spacing (4px grid — only these steps)
`4, 8, 12, 16, 20, 24, 32, 40, 48, 64`. Card padding `20–22`. Section gap `14–16`.
Page padding `40 44` (desktop) / `28 20` (mobile). Content max-width **1040px**.

### 0.4 Radius / shadow / motion / density
```
radius   6 (chip) · 8 (control) · 12 (card) · 16 (modal)
shadow   card: 0 1px 1px rgba(0,0,0,.24), 0 10px 30px -14px rgba(0,0,0,.5)
         win:  + 0 24px 56px -20px rgba(0,0,0,.65)  (terminals, modals)
motion   160ms cubic-bezier(.4,0,.2,1)  — transforms ≤ translateY(-1px)/scale(1.02); no bounce
density  one idea per card; ≥8px between related, ≥24px between groups; never > 3 columns of cards
focus    2px --line-2 ring, never removed (a11y)
```

### 0.5 Component primitives (the kit — built once, used everywhere)
`Button` (primary/ghost/danger) · `Card` (+ header/metric variants) · `Stat` · `Gauge`
(quota bars) · `Sparkline`/`Chart` · `Table` (sortable, empty-state slot) · `Badge`/`StatusDot`
· `Pill` · `CopyField` (one-click copy, redaction-safe) · `CodeBlock`/`SnippetTabs` · `Tooltip`
· `HelpPopover` (the "what is this?" affordance) · `Modal`/`Sheet` · `ConfirmDialog` ·
`Toast` · `EmptyState` (icon + title + body + CTA) · `Skeleton` (replaces `loading…`) ·
`InlineError` (replaces raw `String(e)`) · `Callout` (info/warn/danger) · `Checklist` ·
`Segmented` · `Breadcrumb` · `AccountMenu`.

---

## PART 1 — UX STUDY

### 1.1 Personas → their "first screen"
| Persona | Tier lands | First screen after login | Cares about |
|---|---|---|---|
| **Solo dev** | Free→Solo | Onboarding checklist → cache-hit hero | one-line drop-in, a token in <2 min, "you saved X min", hard-cap gauge (no surprise bill) |
| **Team lead** | Starter→Pro | Overview (usage vs quota + spend) | seats, shared warmth, $-ceiling, when-to-upgrade |
| **Platform/DevEx** | Pro→Max | Surfaces & config | point many tools, PAT scoping, Runners, regions, CLI/SDK, JSON output |
| **Security/Compliance** | Max→Ent | Trust center | isolation/residency proof, BYOK+kill-switch, audit export + in-browser verifier, DSR + signed attestation |

### 1.2 Jobs-to-be-done (lifecycle)
**Activate:** sign up → provisioned tenant [live] → **accept DPA (hard gate, 403 dpa_required)** →
mint/copy a PAT → **point a tool** (Bazel/Turbo/sccache/npm/pip via copy-paste snippet) → **see
first cache hit** (the activation event; p50 ≤ 30 min target). This funnel is the #1 job and is
today **entirely absent inside the dashboard**.
**Operate:** watch usage vs the **three ceilings** (requests / storage / **$-ceiling**) → set the
$-ceiling → manage tokens → set up Runners (GitHub App → repo allowlist → concurrency) → use
Workspaces (`clw`) → manage team/seats.
**Trust:** control isolation/residency → export + **verify the audit chain in-browser** → configure
BYOK + kill-switch → request DSR/erasure + download the **signed attestation**.
**Commercial:** see plan (two axes) → usage-vs-cap → upgrade/downgrade → invoices/payment (Stripe).

### 1.3 The value story to surface (the hero the dashboard must PROVE)
Lead with **cache-hit rate → time saved → $ saved** (a ~90% hit cuts build compute ~5–10×), the
**moat personalized** ("X% of your warmth came free from the shared `_public` mirror"), **flat
concurrency vs metered minutes**, and **zero egress** ("free to pull"). These are the reasons the
product is worth paying for — today the dashboard surfaces **none** of them.

### 1.4 Two product axes (the model the IA must respect)
- **Cache tier** (single ladder): Free / Solo / Starter / Pro / Max / Enterprise. Hard-capped at v0.1
  (over-quota → 429/402 + upgrade CTA). Ceilings in `apps/admin-ui/src/lib/pricing.ts`.
- **Runner SKU** (separate entitlement): `runner_starter…max` (`runners_entitlement`: max_concurrency +
  max_vcpu_h). A tenant holds a cache tier **and** a runner SKU simultaneously — never merge them.
- **Pin/Workspaces**: metered add-on axis (Pin 100GB = $5/mo).

### 1.5 Current-state failures (the audit, condensed — all cited in the research)
1. **No onboarding / first-run** anywhere under `customer/*` (the `welcome` + `PatModal` +
   `onboarding/*` assets exist but are disconnected).
2. **Zero help/tooltips/explanatory copy** on the 6 real pages (only the orphaned viz page has any).
3. **Key actions crippled:** minted PAT dumped as inline text (no copy, no "shown once" — while a
   proper `PatModal` sits **unused**); **no config snippet** to point a cache at all; team is
   invite-only (backend `DELETE …/team/:id` has **no button**).
4. **Placeholder loading/error/empty:** literal `loading…`, raw `String(e)`, blank tables. Audit has
   **no error handling at all**.
5. **No confirmation** on destructive actions (revoke PAT = one click).
6. **Redundant/unlabeled controls:** Billing has **two** portal buttons + **two** upgrade buttons,
   indistinguishable; one prints a raw URL (looks broken).
7. **Flat hierarchy / weak density:** "N% of quota" as plain text (no gauge), daily series with **no
   chart**, raw machine values everywhere (tenant_id, enums, `event_type` codes, `.slice(0,10)` dates).
8. **Orphaned gold:** `audit/visualization` (chain head + Merkle proof modal + in-browser verify +
   export) is the best screen in the app and is **unreachable** (not in nav, bypasses the guard).
9. **Missing chrome:** no account menu, no sign-out, no tenant/org identity, no docs/support, no search.

### 1.6 Data reality (must be designed for — honesty over fake numbers)
- **[live]:** storage `cas_bytes`/`quota_bytes`, billing status/plan + Stripe **portal** + **checkout**,
  PAT CRUD + one-time reveal, team list/invite/remove(backend), BYOK **status** (read-only), audit flat
  list, DSR (`/dsr/*` tree), onboarding `welcome`+PAT reveal, runner **install** button.
- **[stub] (renders but returns 0/[] in prod):** usage `reads`/`writes`/`daily`, `recent_activity`,
  billing `invoices`, `payment_method` (never emitted). → **real backing table OR honest empty state.**
- **[built-unwired]:** `PatModal`, `audit/visualization` (its `/audit/chain/*` routes are **mock-only**),
  DSR/consent/welcome/settings-runners **not in nav**, team-remove, account self-delete.
- **[missing]:** cache-hit rate, egress, per-region, **$-ceiling display/control**, **BYOK self-serve
  config** (4-KMS engine exists, no endpoint/UI), runner entitlement/consumed-vCPU-h/repo-allowlist UI,
  **workspaces** surface, **config snippets**, in-dashboard onboarding checklist.

> **Design principle from 1.6:** never fabricate a metric. Where the backend is a stub, either ship the
> real table (flagged as a backend WP) or show a *teaching* empty state — never a hardcoded 0 that reads
> as "your cache is doing nothing."

---

## PART 2 — TARGET DASHBOARD (the impeccable spec)

### 2.1 Information architecture (unify the orphans; group by job)
Top bar: **HuGR/CoreLink wordmark · tenant switcher/name · region badge · Docs · Help(⌘?) ·
AccountMenu (email, plan, billing, sign-out)**. Left sidebar, grouped:

```
▸ Home            (onboarding checklist until activated → then Overview)
GET STARTED
  · Connect a tool     (per-surface config snippets + "test connection")
  · Tokens             (PATs — scopes explained, copy-safe)
OBSERVE
  · Usage & savings    (gauges + hit-rate + $-saved + moat dedup)
  · Audit log          (+ in-browser chain verifier, promoted from the orphan)
BUILD
  · Runners            (entitlement, repo allowlist, install, consumed vCPU-h)
  · Workspaces         (clw snapshots, Pin) [phase 3]
GOVERN
  · Trust & compliance (BYOK, residency/region, DSR/erasure, attestations, SOC2/ISO status)
  · Team               (members + roles-explained + remove + invites)
ACCOUNT
  · Plan & billing     (two-axis plan, usage-vs-cap, invoices, portal)
  · Settings           ($-ceiling, region, notifications, danger zone)
```
Every nav item has an icon + is reachable. DSR/consent/welcome stop being stranded top-level routes.

### 2.2 Screen specs (each = purpose · content · the "explain" layer · empty/loading/error)
- **Home / Onboarding checklist** — stateful, dismissible: ① accept DPA → ② create+copy PAT (opens
  `PatModal`) → ③ pick a tool + copy config → ④ **first cache hit** (celebrated). Below: the hero
  ROI once activated. Empty tenant = the checklist, never blank cards.
- **Connect a tool** — `SnippetTabs` (Bazel · Turborepo · sccache · npm · pip · CAS/SDK), pre-filled
  with the tenant endpoint + **env-var token reference (never `--pat` inline — CTRL-CRED-001)**; a
  **"Test connection"** button round-trips a real put/get and ticks the checklist. HelpPopover on each.
- **Tokens** — `PatModal` on create (copy + "you'll never see this again" + confirm). Scope
  **checkboxes each with a plain-language description + HelpPopover** (what `cache:r`/`cache:rw`/
  `find-missing`/`admin:audit` grant). Table with copy-safe prefix, last-used, status. **ConfirmDialog**
  on revoke. Empty state: "No tokens yet — create one to connect your first build cache" + CTA.
- **Usage & savings** — three **Gauge**s (requests / storage / **$-ceiling**) with "X% of cap" +
  projected month-end; **cache-hit-rate** trend as hero + p99 latency sparkline + **$-saved** stat +
  **moat dedup** callout. Time-range Segmented. (hit-rate/$-saved/dedup = backend WPs; until then,
  teaching empty states, not zeros.)
- **Audit log** — promote the verifier: from/to + **event-type/severity/actor filters** (client already
  supports `event_types`), pagination, human labels + a legend, and the **in-browser chain verifier**
  (chain head, Merkle proof modal, export) inline — through `CustomerClient`/guard, not bare fetch.
- **Runners** — entitlement (max_concurrency / max_vcpu_h) + **consumed vCPU-h** gauge, the GitHub-App
  install flow with status, **repo-allowlist management**, runner list. "Flat per parallel runner,
  unlimited minutes" framing + upgrade path (separate axis).
- **Workspaces** [phase 3] — `clw` snapshots list, hydrate hint, **Pin** management (refcount, eviction).
- **Trust & compliance** — **BYOK self-serve** (KMS picker: AWS/GCP/Azure/Vault + rotate + **kill-switch**),
  **region/residency** selector + attestation, **DSR/erasure** request + status + **signed-attestation
  download**, compliance status (SOC2/ISO/DPA versions). This is the security buyer's destination.
- **Team** — members with **role descriptions/HelpPopover** (what each RBAC role can do), **remove**
  (wire the backend DELETE, with ConfirmDialog showing "revokes N PATs"), role change, pending-invite
  resend/revoke, **seats-used vs plan cap**.
- **Plan & billing** — **two-axis** plan card (cache tier + runner SKU, independent upgrade paths),
  usage-vs-cap mirror, **one** portal button (kill the redundant/raw-URL one), **one** upgrade path
  with the break-even calculator, invoices (real backing or honest empty), Enterprise = "contact sales"
  locked rows (not hidden).
- **Settings** — **$-ceiling editable control** ("cap my monthly spend at $__", honest "at 100% you get
  429, not a bill"), region, notification prefs, **Danger zone** (account self-delete → wired to
  `POST /v1/customer/account/delete`, MFA-gated, ConfirmDialog).

### 2.3 Cross-cutting systems
- **Explain layer:** every jargon term (BYOK, CMK, PAT scope, CAS, read/write, residency, `_public`,
  each error code 402/429/403/410) has a HelpPopover or contextual Callout linking docs.
- **States:** `Skeleton` (not `loading…`), `InlineError` with a retry + friendly message (not
  `String(e)`), teaching `EmptyState` everywhere.
- **Safety:** `ConfirmDialog` on every destructive/irreversible action; PAT/secret handling stays
  in-memory, redaction-safe.
- **Copy system:** short, plain, benefit-first; error codes map to human guidance.
- **Feedback:** `Toast` on every mutation (created/revoked/invited/saved).

---

## PART 3 — DECOMPOSITION (phases → work packages)

Ordered by dependency + value. **FE** = admin-ui only. **BE** = needs a container/worker route or D1
table first (flagged, so we don't fake data). Each WP is one branch/PR (repo-hygiene rule).

### Phase 0 — Design system in code (foundation; unblocks everything)  · FE
- **WP0.1** Tokens → Tailwind theme + CSS vars (Part 0), light/dark, scoped to `.cx-shell`.
- **WP0.2** Primitive kit (Part 0.5): Button, Card, Stat, Gauge, Table, Badge/StatusDot, Pill,
  CopyField, SnippetTabs/CodeBlock, Tooltip, HelpPopover, Modal/Sheet, ConfirmDialog, Toast,
  EmptyState, Skeleton, InlineError, Callout, Checklist, Segmented, Breadcrumb, AccountMenu.
- **WP0.3** App shell v2: top bar (tenant/region/docs/help/AccountMenu+sign-out) + grouped icon
  sidebar (2.1) + global error boundary + `data-testid` continuity.

### Phase 1 — Wire the orphans + fix the crimes (highest ROI, mostly FE)
- **WP1.1** Replace all `loading…`/`String(e)`/blank tables with Skeleton / InlineError / EmptyState
  across overview/usage/audit/billing/keys/team.
- **WP1.2** Tokens: use `PatModal` (copy + shown-once + confirm) on create; add scope descriptions +
  HelpPopovers; ConfirmDialog on revoke; empty state.
- **WP1.3** Promote `audit/visualization` into the Audit page/nav via `CustomerClient` + guard.
  *(BE dependency: the `/v1/customer/audit/chain/*` routes are mock-only → WP4.3.)*
- **WP1.4** Billing cleanup: one portal button, one upgrade path, remove raw-URL flow, humanize enums.
- **WP1.5** Team: wire `DELETE …/team/:id` (remove + "revokes N PATs" confirm), role descriptions,
  seats-used vs cap.
- **WP1.6** Nav coverage: surface DSR, consent, runners, welcome; account self-delete in Settings danger
  zone (wire `POST …/account/delete`).

### Phase 2 — Activation (the funnel) · FE (+ small BE for the hit signal)
- **WP2.1** Home onboarding checklist (DPA → PAT → connect → first hit), stateful/dismissible.
- **WP2.2** "Connect a tool" screen: SnippetTabs per surface, env-var-safe tokens, **Test connection**.
- **WP2.3** First-cache-hit detection + celebration. *(BE: a "has-written / hit event" signal.)*

### Phase 3 — Value & metrics (make the ROI real)
- **WP3.1 (BE)** Usage backing: per-op/per-day reads/writes/daily (today stub=0), request-count usage,
  `recent_activity`. **WP3.2 (FE)** Gauges + daily chart + range selector on real data.
- **WP3.3 (BE+FE)** **Cache-hit rate** metric + trend; p99 latency sparkline.
- **WP3.4 (BE+FE)** **$-ceiling**: expose current ceiling + editable control (backend gate exists;
  needs read/update endpoint) + 70%/100% upgrade nudges.
- **WP3.5 (BE+FE)** **$-saved** + **cross-tenant dedup savings** (the moat, personalized).

### Phase 4 — Trust as product
- **WP4.1 (BE+FE)** **BYOK self-serve**: KMS picker (AWS live; GCP/Azure/Vault), configure/rotate/
  **kill-switch** (engine exists; needs `POST /v1/customer/byok`).
- **WP4.2 (FE)** Region/residency selector + attestation; DSR/erasure surfaced in Trust center with
  signed-attestation download (DSR backend is live).
- **WP4.3 (BE)** Real container routes for the audit-chain surface (head/events/history/proof) to
  de-mock WP1.3. **WP4.4 (FE)** Compliance status panel (SOC2/ISO/DPA).

### Phase 5 — Two-axis expansion (phase-3 product)
- **WP5.1 (FE)** Runners screen: entitlement + consumed vCPU-h + repo-allowlist mgmt + runner list.
- **WP5.2 (BE+FE)** Consumed vCPU-h metric. **WP5.3 (FE)** Workspaces + Pin surface.
- **WP5.4 (FE)** Plan & billing two-axis card + break-even calculator.

### Sequencing note
Phase 0 → 1 → 2 land a **coherent, honest, activating** dashboard with today's data (mostly FE, fast).
Phases 3–5 are gated on backend WPs (flagged) and land the metrics/trust/expansion depth. Ship 0–2
first for immediate impact; schedule the BE WPs behind them.

---

## Appendix — anchor files
UI: `apps/admin-ui/src/lib/customer-client.ts`, `customer-types.ts`, `lib/pricing.ts`,
`components/customer/*`, `components/onboarding/PatModal.tsx`, `app/[locale]/(authenticated)/customer/*`,
`.../welcome/*`, `.../settings/runners/*`, `app/[locale]/dsr/*`, `app/globals.css`.
Backend: `crates/corelink-container/src/routes/customer.rs`, `customer_d1.rs` (prod truth table),
`routes/dsr.rs`, `worker/src/index.ts` (customer_v1 `:2209`). Spec: `specs/_audits/sealed/2026-05-15-customer-dashboard-spec.md`.
Product: `docs/knowledge/launch/tier-model.md`, `tenancy/dollar-ceiling.md`, `marketing/sales/{PRICING-WORKSHEET,PROOF-POINTS}.md`.
