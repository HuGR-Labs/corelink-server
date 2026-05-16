---
title: "Customer-facing audit-chain visualization UI (wave-29 stream-6)"
date: 2026-05-16
wave: 29
stream: 6
branch: wt/r-prep-audit-chain-viz-ui
base_commit: 365dd38
status: SHIPPED
references:
  - WI-S09-008
  - wave-19 commit 3d835cb (streaming NDJSON + chain-head trailer)
  - wave-19 commit 7ec5435 (CLI verify-ndjson --url + chain-head-anchor header)
  - wave-8 scaffold commit 21f8ea8 (admin-ui prototype — superseded)
deliverables:
  - apps/docs/src/pages/customer/audit-chain.tsx
  - apps/docs/src/pages/customer/audit-chain.module.css
  - apps/docs/i18n/en-US/code.json (i18n strings)
  - apps/docs/i18n/pt-BR/code.json (i18n strings)
  - apps/docs/i18n/de/code.json (i18n strings)
  - apps/docs/i18n/es-419/code.json (i18n strings)
  - specs/04_sprints/S09/work_items/WI-S09-008-customer-audit-export.md (§14 closure-note)
freeze_clause: "§3.b customer-value-add pre-GA enhancement"
---

# Customer-facing audit-chain visualization UI — wave-29 stream-6

## 1. Scope

Wave-29 stream-6 restarts the wave-8 `wt/r-prep-audit-chain-viz` scaffold
(commit `21f8ea8`, never merged) and lands the customer-facing audit-chain
visualization as a standalone page inside the public docs surface
(`apps/docs`, Docusaurus 3, `docs.corelink.dev`).

The wave-8 prototype landed inside `apps/admin-ui` (Next.js admin shell);
the wave-29 deliverable instead lives in `apps/docs/src/pages/customer/`
so the customer-value-add is co-located with the rest of the GA-launch
docs surface (Diátaxis explanation + trust-center pages) and inherits the
4-locale i18n pipeline already wired for the docs site.

The page is read-only and consumes ONLY existing wave-19 customer-audit
endpoints — no new server-side handlers were introduced per the wave-29
stream-6 charter:

| Endpoint | Wave | Source |
|---|---|---|
| `GET /v1/audit/analytics/event-count` | wave-19 | `apps/server/src/routes/audit_analytics.rs` |
| `GET /v1/audit/analytics/timeline` | wave-19 | ibid. |
| `GET /v1/audit/export` (NDJSON streaming) | wave-15 + wave-18 + wave-19 | `apps/server/src/routes/audit_export.rs` |

The 32-byte BLAKE3 chain-head anchor is recovered from the
`X-CoreLink-Audit-Export-Chain-Head-Anchor` response header — the same
canonical header the wave-17/wave-19 CLI `verify-ndjson --url` mode binds
(see `audit_export.rs:148`, header constant `AUDIT_EXPORT_CHAIN_HEAD_ANCHOR_HEADER`).

## 2. UI surface

The page renders five sections (in render order):

1. **Hero card** — page title, subtitle.
2. **Honest pre-GA banner** — `Pilot data — SOC 2 Type II certification pending`,
   per the wave-26 Trust Center copy review. Calls out that integrity is
   cryptographically enforced (RFC 6962 + BLAKE3) and verified daily by the
   cron documented in WI-S09-008, but the SOC 2 evidence collection is still
   in flight ahead of GA.
3. **Window** — date-range picker. The UI caps the window at **7 days** per
   the §UX guideline; the API permits 7 years. The error copy directs power
   users to the API for longer windows.
4. **Chain-head anchor** — 32-byte BLAKE3 hex displayed in a monospace block,
   with an inline deeplink to the verify-ndjson CLI docs (Diátaxis explanation
   `/security/audit-chain`).
5. **Volume metrics + timeline** — event-count + bytes-flushed (estimate) +
   pure-CSS bar-chart timeline keyed off the wave-19 `TimelineResponse` shape.

The export action issues an authenticated `fetch` against `/v1/audit/export`
(bearer + `X-Tenant-Id`) and triggers a `Blob` download — direct `<a download>`
cannot inject the Authorization header.

## 3. Auth

Customer-scoped Clerk JWT. The Clerk shell on the customer dashboard exposes
the active session token at `window.__corelink.getToken()` and the tenant id
at `window.__corelink.tenantId`; both are populated by the admin-ui Clerk
provider before navigation here.

When the shell is absent (e.g. a marketing-link landing) the page surfaces a
non-blocking instruction strip rather than redirecting — the surrounding docs
site is a public surface and a hard redirect would break the wave-25 a11y
baseline.

## 4. i18n

Four canonical locales, sourced from `apps/docs/i18n/<locale>/code.json`:

| Locale | Strings |
|---|---|
| en-US (default) | 28 added |
| pt-BR | 28 added |
| de | 28 added |
| es-419 | 28 added |

Translation keys use the `customer.auditChain.*` prefix. Body copy that
embeds a CLI invocation (`corelink audit verify-ndjson --chain-head-anchor
<hex>`) renders the command literal verbatim in all locales so search-engine
matching still surfaces the command from translated pages.

## 5. Quality gates

| Gate | Status | Notes |
|---|---|---|
| `pnpm typecheck` (`apps/docs`) | green | TS strict, no `any` |
| `pnpm build` (`apps/docs`) | green | DEBT-015-BUILD closed wave-25 + wave-29 |
| `python3 scripts/validate_specs.py` | green | 457 docs OK |
| `python3 scripts/validate_references.py` | green | no dangling refs |
| `pnpm test` (`apps/docs`) | 4 pre-existing failures untouched | sidebars/i18n-locales/REAPI/cross-functional — all baseline on main, unrelated to this stream |

Pre-existing test failures inventory (verified equal to main `365dd38`):

- `tests/sidebars.test.ts` — wave-26 Trust Center category mismatch
- `tests/i18n.test.ts` — `de` locale added in `1fb2dc4` but assertion not updated
- `tests/reapi-gen.test.ts` — REAPI generator drift (separate stream)
- `tests/cross-functional.test.ts` — audit-chain.mdx + byok.mdx draft frontmatter (wave-26)

This stream did NOT introduce any new test failures.

## 6. Cross-references

- `specs/04_sprints/S09/work_items/WI-S09-008-customer-audit-export.md` —
  §14 closure-note pointing back to this UI deliverable.
- `apps/docs/docs/explanation/security/audit-chain.mdx` — the existing
  Diátaxis explanation page that already mentions the customer dashboard
  viz (wave-8 forward reference) and now anchors the verify-offline help
  deeplink from the new page.

## 7. Charter compliance

- Freeze clause: §3.b customer-value-add pre-GA enhancement ✓
- DCO sign-off + Co-Authored-By: in commit trailer ✓
- Worktree-only: `wt/r-prep-audit-chain-viz-ui` based on main `365dd38` ✓
- No server-side handlers added ✓ (verified — `git diff main -- apps/server` is empty)
- Synchronous bash only ✓
