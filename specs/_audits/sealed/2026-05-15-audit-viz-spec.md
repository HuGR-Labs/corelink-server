# Audit-Chain Visualization Spec — Customer Dashboard

- **Date:** 2026-05-15
- **Branch:** `wt/r-prep-audit-chain-viz`
- **Author:** swarm agent (r-prep)
- **Status:** DRAFT (UI skeleton landed; real wire-up deferred to handler-crate work items)
- **Crates touched (logic):** `corelink-audit-chain`, `corelink-wasm`, `corelink-client-verify`
- **App surface:** `apps/admin-ui/src/app/[locale]/customer/audit/visualization/page.tsx`
- **Cross-refs:**
  - `crates/corelink-audit-chain/src/{chain,verifier,event}.rs` — canonical chain + JCS hashing primitives
  - `specs/03_architecture/security_model.md` §Merkle proof — CTRL-AC-001 / CTRL-AC-002
  - `apps/docs/docs/explanation/security/audit-chain.mdx` — customer-facing trust-center explainer
  - `marketing/sales/PROOF-POINTS.md` — auditable-by-design talking points
  - (forward) `apps/admin-ui/src/app/[locale]/customer/audit/page.tsx` — wave-7 customer audit landing (referenced; expected sibling page)

---

## 1. Why

CoreLink already publishes a CloudEvents 1.0 audit chain hashed with BLAKE3 +
RFC 8785 JCS canonicalization (see `corelink-audit-chain` crate). Today
customers can prove inclusion only via the CLI (`corelink audit export
--proof`). Audit-friendly enterprise prospects (FSI, healthcare, GRC) want to
*grok the architecture in a browser*, not read NDJSON. This spec defines a
customer-facing visualization that:

1. Makes the chain head **visible at a glance** (single card with digest,
   total events, last update).
2. Lists **my tenant's leaves** in a paginated table (no other tenants leak in
   — CTRL-TENANT-ISOLATION at the query layer).
3. Renders the **inclusion proof for any leaf** as an animated path from leaf
   → sibling hashes → root, with **each step verifiable in the browser** via
   the `@corelink/client` WASM bundle (which already wraps the canonical
   `corelink-client-verify` Rust crate per ADR-0016 single-truth rule).
4. Shows a **chain integrity timeline** sparkline so trust violations (chain
   head rolling back) are detectable in 1 glance.
5. Links out to the **CLI bundle export** for offline audit submission.

This is *visualization on top of existing primitives*. Zero new server-side
chain logic. Real wire-up to `/v1/customer/audit/*` handlers ships with the
customer audit handler-crate WI (deferred; tracked in the implementation
roadmap).

## 2. Non-Goals

- Cross-tenant chain (customers see ONLY their tenant's leaves; admin UI keeps
  the cross-tenant viewer at `/[locale]/admin/audit`).
- Mutating the chain from the UI (read-only by spec; CTRL-AUDIT-APPEND-ONLY).
- Showing CMK / KEK material (BYOK redacted in the audit envelope already).
- Realtime push (page polls; SSE/WebSocket out-of-scope for this wave).

## 3. Component Spec

### Component A — chain head card

- **Inputs:** `{ head_digest: hex64, total_events: u64, last_updated: iso8601,
  algorithm: "blake3" }` from `GET /v1/customer/audit/chain/head`.
- **Layout:** card with three rows; digest rendered in monospace, truncated
  middle (`abc…def`) with full value in `<details>` and copy-to-clipboard
  button.
- **"Verify head" button:** fetches the most recent N=8 leaves + sibling
  paths, recomputes the head in-browser via the WASM verifier, and shows a
  green check + "Verified locally" badge on match, or a red "Mismatch — see
  trust-center incident page" link on failure.
- **Empty state:** if chain is brand-new (0 events) renders "No audit events
  yet — emit your first one with `corelink put …`".
- **A11y:** card is a `<section aria-labelledby="chain-head-heading">`; the
  verify button is keyboard-reachable and announces status via
  `aria-live="polite"`.

### Component B — leaf table

- **Inputs:** paginated rows from `GET /v1/customer/audit/events?cursor=…`.
  Cursor-based; default page size 25; cursor opaque (already encoded by
  backend).
- **Columns:** `event_id` (monospace, click → modal), `timestamp` (ISO + relative
  "3h ago"), `event_type` (chip; CNCF taxonomy color-coded), `audit_chain_seq`
  (monotonic integer), `leaf_digest` (hex truncated), action ("show proof"
  button).
- **Sorting:** server-side; chain seq descending default; toggle to ascending.
- **Filtering:** event_type multiselect; date range; correlation_id text input.
- **A11y:** semantic `<table>` with `<caption>` ("My tenant's audit events,
  most recent first"); row click also surfaces `Enter` key handler; `tabindex`
  on header cells for sort toggle.

### Component C — proof modal

- **Trigger:** "show proof" button on any leaf row, or focusing a leaf and
  pressing `Enter`.
- **Content:**
  1. **Leaf banner:** event_id + chain_seq + leaf digest.
  2. **Proof path:** ordered list of `{ sibling_hash, position }` pairs from
     leaf to root.
  3. **Step verifier:** each step shows the running hash before / after; a
     `Verify step` button computes `BLAKE3(left || right)` via WASM and
     shows ✓ if it matches the spec's next step.
  4. **Final check:** computed root vs claimed `expected_root`; green check
     or red mismatch.
  5. **Export step:** "Download proof JSON" — bundles `leaf`, `siblings`,
     `expected_root`, `algorithm`, `chain_seq` as a JSON-LD envelope.
- **Animation:** the path renders as a vertical tree (CSS grid, no SVG
  required for v1); animation is `prefers-reduced-motion` aware.
- **Verifier source:** `apps/admin-ui/src/lib/audit/verify-proof.ts` —
  imports `@corelink/client` (the published WASM npm package wrapping
  `corelink-wasm`) and uses the canonical BLAKE3 from `corelink-client-verify`
  (S-02 SEAL). Existing SHA-256 path in `apps/admin-ui/src/lib/merkle.ts`
  remains for the admin UI; this is the BLAKE3 sibling.
- **Failure UX:** if WASM fails to load (offline / CSP block) the modal
  surfaces a banner "Browser verifier unavailable — use `corelink audit
  verify` CLI" with a link to the CLI doc; the proof bytes remain
  downloadable so the customer can verify offline.
- **A11y:** Radix `<Dialog>` (already a dep) with `aria-labelledby` /
  `aria-describedby`; close via `Esc`; trap focus inside; restore focus to
  the trigger on close.

### Component D — chain integrity timeline

- **Inputs:** `GET /v1/customer/audit/chain/history?window=30d` → an array of
  `{ timestamp, head_digest, total_events }` snapshots (one per UTC hour).
- **Visual:** sparkline of `total_events` over the window (CSS-only bars;
  no chart library needed); each tick is keyboard-focusable and announces
  the timestamp + digest via `aria-label`.
- **Anomaly markers:** a red dot is rendered on any tick where the snapshot's
  `total_events` is **lower** than the prior tick (chain rollback — should be
  impossible per CTRL-AUDIT-APPEND-ONLY). Clicking a marker links to the
  trust-center incident response page.
- **Empty state:** "Insufficient history — chain less than 24h old."

### Component E — export bundle button

- **Action:** triggers a client-side download of a JSON-LD audit bundle
  containing the chain head, the visible leaves' inclusion proofs, and the
  list of `corelink audit verify` invocations needed to verify offline.
- **CLI doc link:** opens `/docs/reference/cli/audit` in a new tab.
- **Audit-of-audit:** the download is *client-side only* (no server emit),
  but the export click logs a `customer.audit.export_clicked` analytics
  event via the existing analytics pipeline (PII-free per CTRL-PRIV-002).

## 4. Accessibility (WCAG 2.2 AA)

- **Color contrast:** all text + interactive states ≥ 4.5:1 on light AND dark
  surfaces; verify with `pnpm test:a11y` axe rules + Lighthouse a11y ≥ 95.
- **Keyboard:** every interactive element reachable via Tab; focus rings
  visible (default `:focus-visible` style; no `outline: none` overrides).
- **Screen reader:** semantic landmarks (`<main>`, `<nav>`, `<section>`);
  `aria-live="polite"` on dynamic verification results; never use *only*
  color for verification status (always paired ✓/✗ glyph + text).
- **Reduced motion:** all animations gated on
  `@media (prefers-reduced-motion: no-preference)`.
- **i18n:** all strings pulled from `next-intl` message catalogs under
  `messages/{en,pt-BR}/customer-audit.json` (catalog stub created; copy
  shipped EN-only in this wave, pt-BR translation tracked).

## 5. Data flow + APIs (server stubs)

Endpoint surface (handler crate work TBD; mocked here under
`apps/admin-ui/src/lib/e2e-mock-fixtures.ts`):

| Endpoint | Method | Returns |
|---|---|---|
| `/v1/customer/audit/chain/head` | GET | `{ head_digest, total_events, last_updated, algorithm }` |
| `/v1/customer/audit/events?cursor=…` | GET | `{ rows: AuditLeaf[], next_cursor: string \| null }` |
| `/v1/customer/audit/events/{event_id}/proof` | GET | `{ leaf_hash, siblings, expected_root, algorithm, chain_seq }` |
| `/v1/customer/audit/chain/history?window=30d` | GET | `{ snapshots: ChainSnapshot[] }` |

All endpoints scope by `tenant_id` derived from the session JWT — never from
a query parameter. CTRL-TENANT-ISOLATION enforced at the handler boundary.

## 6. Security review checklist

- [x] No PII rendered (event payloads already redacted per
      `corelink-audit-chain` JCS canonicalizer).
- [x] No CMK/KEK material in any endpoint response.
- [x] WASM verifier loaded via SRI-pinned `@corelink/client` package; CSP
      allows `wasm-unsafe-eval` only on the visualization page (scoped via
      route-level CSP header).
- [x] Export bundle is `application/json` (no executable content).
- [x] All endpoints session-bound; no PAT bypass.
- [x] Chain rollback anomaly markers surface trust-center incident page
      (not silent).

## 7. Test plan

- Unit: `verify-proof.ts` happy path + tamper-rejection + WASM-load-failure
  fallback (existing `apps/admin-ui/src/lib/__tests__` pattern).
- E2E (Playwright, this PR): `audit-viz-chain-head.spec.ts` + `audit-viz-
  proof-modal.spec.ts` covering chain head load + proof verification flow.
- A11y: axe via existing `test:a11y` Vitest harness (to be added in the
  follow-up wave when full A11y harness covers `customer/` routes; CI gate
  parity).
- Visual regression: not in scope for v1.

## 8. Open questions / followups

- Should the export bundle include the *entire* visible page or only the
  selected leaf? Default v1: visible page. Track in follow-up after lighthouse
  customer feedback.
- WASM bundle is ~480 KB gzipped (S-02 baseline). Acceptable for the
  visualization page; lazy-loaded only on first proof-modal open to keep the
  table interactive at TTI.
- Animation for the proof path tree could move to SVG with smooth path
  drawing in a v2; v1 keeps CSS for bundle-size discipline.

## 9. Cross-link manifest

This visualization should be linked from:

1. `apps/docs/docs/explanation/security/audit-chain.mdx` — "See it in your
   browser: /customer/audit/visualization".
2. `marketing/sales/PROOF-POINTS.md` — "Audit chain visualizable in your
   browser — no SOC 2 auditor laptop required."
3. (forward) `apps/admin-ui/src/app/[locale]/customer/audit/page.tsx` — when
   wave-7 customer audit landing ships, the page gains a tab/link to
   `./visualization`.

---

*This spec lives in `specs/_audits/` (per validator skip-list); when the
handler-crate WI ships the production endpoints, a sibling normative spec
under `specs/02_product/customer-audit-visualization.md` with full front-matter
will replace this doc.*
