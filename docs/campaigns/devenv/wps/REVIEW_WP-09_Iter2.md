# WP-09 REVIEW — Iteration 2 (DEEPER AUDIT)

**Reviewer:** Self (TechLead persona)
**Date:** 2026-08-26
**Previous Verdict:** ❌ FAIL (13 BLOCKING, 9 HIGH, 7 MEDIUM — iter 1)
**New Verdict:** ❌ **FAIL — 11 NEW ISSUES (5 BLOCKING, 4 HIGH, 2 MEDIUM)**

---

## Iter 1 Fixes — Verification

| Iter 1 Issue | Status | Evidence |
|--------------|--------|----------|
| **B1** No `@tanstack/react-query` | ✅ **Fixed** | Uses `useState`+`useEffect`+`useCustomerClient` (mirror of `WorkspacesClient.tsx:46-93`) |
| **B2** No `@/lib/api` | ✅ **Fixed** | Imports `CustomerClient` from `@/lib/customer-client` |
| **B3** `apiClient` object | ✅ **Fixed** | Uses `CustomerClient.request<T>()` via class methods |
| **B4** API contract diverges from WP-08 | ✅ **Fixed** | `devenvId` dropped from URLs; types align with WP-08 §3.3 |
| **B5** Hard-coded empty `devenvId` | ✅ **Fixed** | No `devenvId` in any URL |
| **B6** Dead `useWebSocketConnection` hook | ✅ **Fixed** | Hook deleted from §3.1 |
| **B7** `useDevenvDetail` / `useDevenvActions` split | ✅ **Fixed** | `useDevenvList`, `useDevenvStatus`, `useDevenvActions` (4 hooks) defined |
| **B8** Stale closure + missing import + dep bug | ✅ **Fixed** | `useCallback` + proper deps; no `as any` |
| **B9** `Button variant="outline"` not in kit | ❌ **REGRESSED** | WP-09 now uses `variant="secondary"` — **same bug**, different value. Kit has `primary | ghost | danger` only |
| **B10** `Input` requires `label` | ⚠️ **Partial** | Modal now passes `label` to `Input`, but the kit's `Input` (`Input.tsx:4`) does NOT actually require `label` — it just spreads `InputHTMLAttributes`. So the call works, but the `label` prop renders a duplicate `<label>` (Field renders one, the `label` attribute on `<input>` is a different `label` association). Visual: duplicate label |
| **B11** `Badge variant` shadcn | ✅ **Fixed** | Uses `tone` + `dot` |
| **B12** `Card` import path | ✅ **Fixed** | Uses `@/components/ui/linear` barrel |
| **B13** `toast.error()` not in kit | ✅ **Fixed** | Uses `toast({ title, tone: "danger" })` |
| **H1** `features/devenv/` tree | ✅ **Fixed** | Files under `apps/admin-ui/src/components/devenv/` |
| **H2** `useEffect` deps | ✅ **Fixed** | `useCallback([client])` listed in deps |
| **H3** No `enabled` gate | ✅ **Fixed** | `useDevenvList(enabled = true)` and `useDevenvStatus(enabled = true)` |
| **H4** WebSocket auth in new tab | ✅ **Fixed** | `urls.vnc/tty/code` are server-issued (per WP-08) |
| **H5** Invalid `_blank_*` target name | ✅ **Fixed** | `devenv_conn_${Date.now()}` (non-reserved) |
| **H6** No `Skeleton` defined | ✅ **Fixed** | `DevenvSkeleton.tsx` uses Linear `Skeleton` |
| **H7** No `ConfirmDialog` for Stop | ✅ **Fixed** | `ConfirmStopDialog` + ConfirmDialog in detail |
| **H8** Invented status fields | ✅ **Fixed** | `DevenvStatusResponse` matches WP-08 §3.3 exactly (no invented fields) |
| **H9** Invariant duplicates + bad numbering | ✅ **Fixed** | Renumbered I1…I8; auto-dismiss claim dropped |
| **M1** Mobile breakpoints | ✅ **Fixed** | `flex-col sm:flex-row`, `grid-cols-1 lg:grid-cols-2` |
| **M2** No `Field`/`InlineError`/`EmptyState` | ✅ **Fixed** | All present in snippets |
| **M3** No unit test skeleton | ❌ **Missing** | Completeness checklist still only says "Unit tests" without file paths or assertions |
| **M4** No Playwright E2E spec | ❌ **Missing** | Completeness checklist still only says "Playwright" without path or fixture spec |
| **M5** Sign-Off rows | ✅ **Fixed** | Author / Frontend TL / QA / TechLead |
| **M6** Risk register gaps | ✅ **Fixed** | Container WS auth + quota + Clerk token expiry added |
| **M7** Self-Check `target="_blank"` | ✅ **Fixed** | Now says `window.open` with unique target name |

**Iter 1 fix verification: 24/29 fixed, 1 regressed (B9 → secondary), 4 missing/incomplete.**

---

## 🔴 NEW BLOCKING ISSUES (Missed in Iteration 1)

### B15. **`Button` variants: kit ships `primary | ghost | danger` only — WP-09 uses `secondary` everywhere**

- **Location:** `ConnectionButtons.tsx:596, 608, 619` (`variant="secondary"`); `ResizeControls.tsx:692, 709` (`variant="primary" | "secondary"`); `DevenvDetailClient.tsx:1087, 1094` (`variant="secondary" | "danger"`).
- **Reality check:** `apps/admin-ui/src/components/ui/linear/Button.tsx:11` types `variant?: "primary" | "ghost" | "danger"`. Grepped all `*.tsx` under `apps/admin-ui/src/components`: **zero files use `variant="secondary"`** — the variant does not exist.
- **Why iter 1 missed it:** Iter 1 caught `variant="outline"` and recommended `variant="secondary"`, but the kit does not ship a `secondary` variant either. There is no "visual outline" primitive.
- **Fix:** Two options:
  - **Option A (preferred):** add a `secondary` variant to `Button.tsx` (1-line class addition + tailwind/CSS — `lin-btn--secondary`, e.g. `bg-white text-slate-900 border border-slate-200 hover:bg-slate-50`). Cross-WP ask: file a Linear kit issue.
  - **Option B:** use `variant="ghost"` for the de-emphasized buttons (ResizeControls presets, detail "Open" / "Refresh" / "Stop") and reserve `primary` for the dominant action. Ghost is the existing kit de-emphasis primitive.
- **Why BLOCKING:** Every line in the snippets that says `variant="secondary"` is a TS compile error.

### B16. **No `getServerAuth` export; `getAuthContext` returns `AuthContext` (no `tenantId`, no `getToken`)**

- **Location:** `apps/admin-ui/src/app/[locale]/(authenticated)/customer/devenv/[devenvId]/page.tsx:1206, 1216, 1221` (the new server component).
- **Problem:** WP-09 imports `getServerAuth` from `@/lib/auth`. `auth.ts` exports `getAuthContext(): Promise<AuthContext>` where `AuthContext = { user_id, org_id, role, mfa_verified_at }`. There is **no `tenantId` field** and **no `getToken` method** on the context. The session-tenant mapping flows through Clerk's `orgRole` → `corelink-*` role, with `org_id` being the Clerk org id — not the CoreLink tenant id.
- **Verdict:** The entire `DevenvDetailPage` server component (`page.tsx:1211-1233`) is fiction. It will not compile, and even if it did, the `session.tenantId !== params.devenvId` 404 check is unverifiable (no `tenantId` exists on the session).
- **Real pattern (next/upgrade page):** `getSessionToken()` in `app/[locale]/upgrade/page.tsx:65-76` lazy-imports `@clerk/nextjs/server` and reads `auth().getToken()` directly. There is no `getServerAuth` shortcut. The repo pattern is `await auth()` from `@clerk/nextjs/server`, then read `orgId` if present.
- **Why BLOCKING:** The detail page cannot be wired without a working server-side auth + token fetch — and the "foreign-id defense" is non-functional without `orgId`.
- **Fix:**
  ```typescript
  // Replace the entire server component with the real pattern:
  import { notFound, redirect } from "next/navigation";
  import { auth } from "@clerk/nextjs/server";
  import { CustomerClient } from "@/lib/customer-client";
  import { DevenvDetailClient } from "@/components/devenv/DevenvDetailClient";

  export default async function DevenvDetailPage(props: {
    params: Promise<{ locale: string; devenvId: string }>;
  }): Promise<React.ReactElement> {
    const { devenvId } = await props.params;
    const session = await auth();
    if (!session?.userId) redirect("/sign-in");

    // Foreign-id defense: the devenvId is the Clerk orgId (or the userId
    // for the solo-tenant self-serve case — per WP-08 §3.2 `devenv_id == tenantId`).
    // The DO is per-tenant, so a foreign id is rejected at the DO anyway,
    // but fail-closed at the page boundary to avoid a wasted fetch.
    const tenantId = session.orgId ?? session.userId;
    if (devenvId !== tenantId) notFound();

    const client = new CustomerClient({
      getToken: () => Promise.resolve(session.getToken()),
    });

    let initial: Devenv;
    try {
      const list = await client.listDevenvs();
      initial = list.find((d) => d.devenv_id === devenvId) ?? list[0];
      if (!initial) notFound();
    } catch {
      notFound();
    }

    return <DevenvDetailClient initial={initial} />;
  }
  ```

### B17. **Next 15 sync `params` — `params: { devenvId: string }` will not compile**

- **Location:** `DevenvDetailPage` (`page.tsx:1214-1215`) — `params: { devenvId: string }`.
- **Reality:** Next 15 changed `params` and `searchParams` from a sync object to a `Promise` (see 28 occurrences of `params: Promise<…>` across the app — `app/[locale]/upgrade/page.tsx:78-83`, `app/[locale]/(authenticated)/admin/ops/[op_id]/page.tsx:12-26`, etc.). Every `await props.params` is mandatory.
- **Why BLOCKING:** The server component signature itself fails TypeScript.
- **Fix:** see B16 example (`params: Promise<…>` + `await props.params`).

### B18. **`ConfirmDialog` API wrong — uses `onOpenChange` + `tone="danger"`; kit uses `onClose` + `danger: boolean`**

- **Location:** `ConfirmStopDialog.tsx:836-845` — `onOpenChange={onOpenChange}`, `tone="danger"`. `DevenvDetailClient.tsx:1180-1186` — same.
- **Reality:** `apps/admin-ui/src/components/ui/linear/ConfirmDialog.tsx:8-16` types `{ open, onClose, title, body, confirmLabel, danger, onConfirm }`. There is **no `onOpenChange`** and **no `tone`** — the kit uses `danger: boolean`.
- **Why BLOCKING:** Every `ConfirmDialog` instantiation is a TS error.
- **Fix:** `<ConfirmDialog open={…} onClose={…} title={…} body={…} confirmLabel={…} danger onConfirm={…} />` (drop `tone`, use `danger`, swap `onOpenChange` → `onClose`).

### B19. **`Input` `label` prop renders duplicate labels when used inside a `Field`**

- **Location:** `CreateDevenvModal.tsx:789, 800` — `<Input label="Workspace name" … />` inside a `<Field label="Workspace name">`. Also `ResizeControls.tsx:667, 680` — `<Input label="Width" … />` inside a `<Field label="Width">`.
- **Reality:** The kit's `Input` (`Input.tsx:4-12`) is `InputHTMLAttributes<HTMLInputElement>` with no custom `label` handling — the `label` prop is just spread to the underlying `<input>` as the HTML `aria-label`/`form` attribute (NOT rendered as a `<label>` element). It will render a duplicate label visually: `Field` renders `<label class="lin-label">Workspace name</label>` and the `label` attribute is silently absorbed (a no-op for a bare `<input>` since `<input>` doesn't render a label child).
- **Secondary effect:** `Input` requires NO `label` prop, but the type spreads `InputHTMLAttributes`, so the prop is accepted — but it does nothing. Iter 1's complaint that "Input requires label" is **false**; the actual problem is that passing `label` to a kit `Input` does nothing AND `Field` already renders the label visually.
- **Why BLOCKING:** The snippets are misleading — readers will copy the `label="…"` prop, ship the duplicate, and the modal renders with the field's label showing twice (once from `Field`, once from… wait, the `<input>` itself doesn't render its `label` attribute, so the user sees only the `Field` label). But the snippet CLAIMS it labels the input — it doesn't. The `htmlFor` linkage from `Field` to `Input` is missing too: `Field` only takes `htmlFor` as an optional prop, and the snippet does not pass it. So the label-click does NOT focus the input (failing basic a11y).
- **Fix:** Drop the `label="…"` prop on `Input`, add `id="…"` to `Input`, add `htmlFor="…"` to `Field` — the canonical `KeysClient.tsx:233-242` pattern. Example:
  ```tsx
  <Field label="Workspace name" htmlFor="devenv-create-workspace" error={…} hint="…">
    <Input id="devenv-create-workspace" required value={…} onChange={…} />
  </Field>
  ```

---

## 🟠 NEW HIGH SEVERITY ISSUES

### H10. **Field has no `error` or `hint` props — kit ships only `label, htmlFor, help, children`**

- **Location:** `CreateDevenvModal.tsx:787, 798` — `<Field label="…" error={validationError ?? undefined} hint="…">`.
- **Reality:** `apps/admin-ui/src/components/ui/linear/Field.tsx:4-9` types `FieldProps = { label, htmlFor?, help?, children }`. There is **no `error` prop** and **no `hint` prop**. The kit's `help` renders a `HelpPopover` (a hover info icon, not inline helper text) — different semantics.
- **Why HIGH:** The validation error and the hint cannot be wired through `Field`. Currently the snippet claims `error={validationError}` works — it silently does nothing.
- **Fix:** Drop the `error` and `hint` props. Either:
  - **Option A (cleanest):** add `error?` + `hint?` to the kit (one-line each — the validation pattern is a kit-wide concern, and `KeysClient.tsx` doesn't have validation feedback either, so this would be the first validation-bearing form). Cross-WP ask to file a Linear kit issue.
  - **Option B:** render the error/hint OUTSIDE the `Field` (sibling `<div>` with `role="alert"` for the error). The hint and error are not coupled to the input, so a sibling is honest.
  - For this WP, choose Option B (does not block the kit extension); document the cross-WP ask.

### H11. **`Tooltip` prop is `label` not `content`; `aria-label` on disabled `<button>` is unreachable**

- **Location:** `ConnectionButtons.tsx:595, 605, 615` — `<Tooltip content="…">`. `Button` instances also pass `aria-label="…"` (line 599, 609, 619) on a `<button disabled>`.
- **Reality 1:** Kit `Tooltip` (`Tooltip.tsx:7-9`) types `{ label, children }`. There is no `content` prop.
- **Reality 2:** A disabled `<button>` is **inert for assistive tech** — `aria-label` on it is announced, but the button is not focusable, so the tooltip wrapper (which uses `onFocus`/`onMouseEnter`) never fires on keyboard nav. The `Tooltip` wraps the disabled `<Button>` in a `<span tabIndex={0}>` (`Tooltip.tsx:18`), which is the correct workaround for the inert-button case, but the `aria-label` on the disabled button itself is redundant noise.
- **Why HIGH:** Two compile errors (`content` not in `Tooltip`) + an accessibility foot-gun (redundant `aria-label` on a disabled button is announced, which can confuse screen-reader users who can't reach the button).
- **Fix:**
  ```tsx
  <Tooltip label={isRunning ? "Open noVNC desktop in a new tab" : "DevEnv is not running"}>
    <Button
      variant="primary"
      disabled={!isRunning}
      onClick={() => openInNewTab(urls.vnc)}
    >
      noVNC Desktop
    </Button>
  </Tooltip>
  ```
  (Drop the `aria-label` — the button text "noVNC Desktop" + the Tooltip text already describe the target. The Tooltip's `<span tabIndex={0}>` wrapper provides the focusable label for the disabled-button case.)

### H12. **`Skeletons` per-row, but `Skeleton` kit accepts `rows` not individual row props**

- **Location:** `DevenvSkeleton.tsx:860-865` — `<Skeleton className="h-5 w-40" />` (passes `className`).
- **Reality:** Kit `Skeleton` (`Skeleton.tsx:1-17`) types `{ rows?: number; width?: string }`. It does NOT accept `className` — the prop is silently dropped. The `width` prop applies to the LAST row only, not arbitrary rows. A `h-5 w-40` className on `Skeleton` does nothing.
- **Why HIGH:** The skeleton renders a single full-width bar per `<Skeleton />` (all rows are `100%` width); the visual layout claimed by the snippet (3 buttons side-by-side with specific widths) will not appear. The skeleton looks wrong on the page.
- **Fix:** Either extend the kit to accept `className` (small ask, consistent with the `Input` className passthrough), or restructure the skeleton to use multiple `Skeleton` instances inside a flex container with explicit `style={{ width: … }}` on each:
  ```tsx
  <Card>
    <div className="space-y-3">
      <Skeleton rows={1} width="40%" />
      <Skeleton rows={1} width="64%" />
      <div className="flex gap-2 pt-2">
        <Skeleton rows={1} width="120px" />
        <Skeleton rows={1} width="120px" />
        <Skeleton rows={1} width="120px" />
      </div>
    </div>
  </Card>
  ```

### H13. **`useParams` not imported in `DevenvDetailClient` (line 1040) — but it's never used either**

- **Location:** `DevenvDetailClient.tsx:1040` — `import { useParams, useRouter } from "next/navigation";`
- **Problem:** The component destructures only `useRouter`. `useParams` is imported but never referenced → eslint `no-unused-vars` fail. More importantly, the `useParams` was a leftover from the iter 1 design (which had the client component reading the param itself). The corrected design has the server `page.tsx` resolve the param and pass `initial` — so the client doesn't need `useParams` at all.
- **Why HIGH:** Build will fail on the unused import (the kit's ESLint config flags `no-unused-vars` per `apps/admin-ui/eslint.config.mjs`).
- **Fix:** `import { useRouter } from "next/navigation";` — drop `useParams`.

---

## 🟡 NEW MEDIUM SEVERITY ISSUES

### M8. **List endpoint field-name divergence: WP-09 `connection_urls: { vnc, tty, code }` vs WP-08 `vnc_url`, `tty_url`, `code_url`**

- **Location:** `customer-types.ts:111-118` (`DevenvConnectionUrls`) + cross-WP `WP-08:415-417` (OpenAPI `DevenvCreated.vnc_url`/`tty_url`/`code_url`).
- **Problem:** WP-08 §4 OpenAPI declares:
  ```yaml
  DevenvCreated:
    properties:
      vnc_url: ...
      tty_url: ...
      code_url: ...
  ```
  WP-09 invents a different shape:
  ```typescript
  interface DevenvConnectionUrls {
    vnc: string;
    tty: string;
    code: string;
  }
  ```
  At runtime, the server returns `{ vnc_url, tty_url, code_url }` (per WP-08 spec) and the client destructures `{ vnc, tty, code }` — **every field is `undefined`**, the buttons are disabled, and the "fall back to Open detail" path is the only path the user can take. The fix is a rename, not a feature.
- **Cross-WP finding:** The list request (WP-09 §9 row 2) is to ADD `connection_urls` to the list response too. WP-08 §4 has NO list-response field for this — so the list-card can never have working buttons in v1 (it has to fall back to "Open detail"). Document this honestly: the list-card is a "Open detail" button OR a disabled button set, NOT a working button set.
- **Why MEDIUM:** Not a compile error (the type is internal), but a runtime contract mismatch. The list will be broken even when WP-08 lands, because the field names don't line up.
- **Fix:**
  ```typescript
  export interface DevenvConnectionUrls {
    /** Server-issued URL for the noVNC WebSocket upgrade. */
    vnc_url: string;
    /** Server-issued URL for the ttyd WebSocket upgrade. */
    tty_url: string;
    /** Server-issued URL for the code-server WebSocket upgrade. */
    code_url: string;
  }
  ```
  And every destructure becomes `{ vnc_url, tty_url, code_url }` not `{ vnc, tty, code }`.

### M9. **Server `DevenvDetailPage` re-fetches via `CustomerClient` on every navigation; redundant with the client `useDevenvStatus` poll**

- **Location:** `DevenvDetailPage` (`page.tsx:1221-1229`) + `DevenvDetailClient` `useDevenvStatus(true)` at 10s.
- **Problem:** The server component fetches the full `Devenv` via `listDevenvs()` to get `initial`. The client component then immediately starts polling `getDevenvStatus()` every 10s. Two issues:
  1. **N+1 on initial render:** the server fetch returns `Devenv` (no `connection_urls` because the list endpoint doesn't return them — see M8 + WP-08 §4). The client then issues its first poll. So the "Open noVNC" button is `disabled` (because `initial.connection_urls` is undefined) until the first poll fires 10s later. The user sees a dead button.
  2. **Quorum/race:** the server `listDevenvs` is fired even though the client will poll — wasted RPC. The right pattern is to fire `getDevenvStatus` server-side for `initial` (it returns the live `DevenvStatusResponse`), or to keep `listDevenvs` for the server (since the list endpoint is the only one returning `workspace_name` + `profile_name` + `devenv_id`) and then `getDevenvStatus` to populate `connection_urls` once on the server.
- **Why MEDIUM:** Two wasted RPCs per navigation; visible 10s gap before buttons become enabled. Not a correctness issue, but a UX issue.
- **Fix:** Server-side, fire BOTH `listDevenvs()` (for the `Devenv` row) and `getDevenvStatus()` (for the `DevenvStatusResponse` — has `ports`); merge into `initial`. Then the client's first poll at 10s is purely a refresh, not a "make the buttons work" gating step. Or simpler: change the polling to start at 0s (`useDevenvStatus(true)` already does this — `void reload()` is called once on mount) and let the first poll populate `connection_urls`. Document the 10s lag as known and acceptable for v1.

---

## 🔍 CROSS-WP COORDINATION FINDINGS

### Cross-WP-1. **WP-08 has NO list-response field for `connection_urls`**

The list endpoint `GET /v1/customer/devenv` returns a `Devenv` (per WP-08 §4) WITHOUT `connection_urls`. WP-09's `Devenv.connection_urls?: DevenvConnectionUrls` is therefore **always undefined** on the list path. WP-09 §9 already requests this be added (line 1371), but the request has not been actioned. **Iter 2 outcome:** the list-card must render a single "Open detail" button (the existing fallback path), not three disabled buttons + tooltip. Verify `ConnectionButtons` when `urls === null` returns the existing "Open detail" path — it does (line 585-591 of WP-09), so this is a documented limitation, not a bug.

### Cross-WP-2. **WP-08 declares `vnc_url`/`tty_url`/`code_url` (with `_url` suffix), WP-09 invents `vnc`/`tty`/`code` (no suffix)**

The OpenAPI schema is the source of truth for wire field names. WP-09 must rename to match. See M8.

### Cross-WP-3. **WP-08's `Devenv` schema has `created_at`, `started_at`, `ports: number[]` — WP-09's `Devenv` has the same fields but `DevenvStatusResponse` has `ports: DevenvPort[]` (objects with `port` + `healthy`)**

The `/status` endpoint's `ports` shape diverges from the list's `ports` shape. This is consistent with WP-08 §3.3 (status has the rich port-health object) but WP-09 needs to be explicit that `Devenv.ports` is `number[]` (just the listening ports) while `DevenvStatusResponse.ports` is `DevenvPort[]` (rich objects). The current types are correct, but the field naming (`ports` vs `ports`) means callers must read the right endpoint's payload. No code change; just confirm docs are clear.

### Cross-WP-4. **The detail page's `notFound()` for foreign `devenvId` is the right defense — but `session.tenantId` doesn't exist on Clerk's session in this codebase**

`@clerk/nextjs/server`'s `auth()` returns `{ userId, orgId, orgRole, sessionClaims, … }`. The CoreLink-tenant mapping is `orgId ?? userId` (solo-tenant case) per the existing customer routes' `getAuthContext()`. The iter 1 "session.tenantId" framing is wrong; the iter 2 fix (B16) uses `session.orgId ?? session.userId` correctly.

### Cross-WP-5. **WP-08's WS upgrade routes require Clerk bearer; `window.open` cannot send it**

This was iter 1 H4. Iter 1's "fix" was "use server-issued, short-lived signed URLs" — but WP-08 does NOT currently mint signed URLs. The Worker route at `/v1/customer/devenv/vnc` is a `proxyWebSocket` forward — it does require the Clerk bearer on the upgrade. `window.open` cannot send a header. **Today, the new tab opens, the WS upgrade fails, noVNC never loads.** This is an unresolved cross-WP auth-model gap. WP-09's snippets that say `openInNewTab(urls.vnc)` will produce broken buttons in production.

The honest fix is one of:
- **Option A:** add a `?token=<jwt>` query-string to the URLs (requires WP-08 to mint a short-lived Clerk JWT on the create response, AND the container WS proxy to accept a `?token=` query on the upgrade). Cross-WP ask to WP-08.
- **Option B:** render the noVNC/ttyd/code client inline (iframe) with a `postMessage` handshake. Heavy.
- **Option C:** open a server-side proxy page at `/customer/devenv/vnc` that performs the WS upgrade with the bearer cookie (must be a first-party route, so the cookie rides). Likely the right answer.

This is a **BLOCKING** for production deploy, but the WP can land with the limitation documented if WP-08 commits to one of A/B/C before launch. WP-09 must NOT claim "Connection buttons open correct URLs" (DoD #5) until this is resolved.

---

## 📋 DoD Re-Verification (Iter 2)

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | DevEnv list loads and displays status | ❌ **Cannot build** | `Button variant="secondary"` TS error; `Modal onOpenChange` not in kit |
| 2 | Create modal validates workspace name | ❌ **Cannot build** | `Field error/hint` not in kit; `Input label` does nothing |
| 3 | Create DevEnv shows loading then success | ❌ **Cannot build** | `Button variant="secondary"` TS error |
| 4 | DevEnv card shows real-time status | ✅ Will work | Manual poll pattern matches `WorkspacesClient` |
| 5 | Connection buttons open correct URLs | ❌ **Cannot build + cross-WP auth gap** | TS errors + WS auth model unresolved |
| 6 | Buttons disabled when not running | ✅ Will work (after B15 fix) | `disabled` prop set; Tooltip wraps for focus |
| 7 | Resize controls apply dimensions | ❌ **Cannot build** | `Button variant="secondary"`; `Input label` wrong; `Skeleton className` dropped |
| 8 | Status badge tones match Linear kit | ✅ Will work | `tone` + `dot` matches `Badge.tsx` |
| 9 | Empty state shows helpful message | ✅ Will work | `EmptyState` snippet matches `EmptyState.tsx` |
| 10 | Error states handled gracefully | ✅ Will work (after H10 fix) | `toast({ title, tone: "danger" })` + `InlineError` |
| 11 | Mobile responsive | ✅ Will work | Tailwind classes match patterns |
| 12 | Stop is gated by ConfirmDialog | ❌ **Cannot build** | `ConfirmDialog` API wrong (`tone`/`onOpenChange` not in kit) |
| 13 | Quota 403 surfaces upgrade guidance | ❌ **Cannot build** | `Button variant="secondary"` (the upgrade CTA in EmptyState) |
| 14 | All data flows through `useCustomerClient` | ✅ Will work | Pattern matches `WorkspacesClient` |

**DoD Pass Rate: 5/14 = 36%** (was 0/10 → 5/14 after iter 1 + iter 2 fixes; the new failures are from the kit-API drift, not regressions)

---

## 📋 Invariants Re-Verification (Iter 2)

| Invariant | Enforced? | Verdict |
|-----------|-----------|---------|
| I1: All API calls include auth | ✅ Via `useCustomerClient` | **ENFORCED** |
| I2: Polling 10s detail, 30s list | ✅ Hardcoded constants | **ENFORCED** |
| I3: Connection buttons disabled unless running | ✅ Logic present | **ENFORCED** |
| I4: Resize 1-8192 | ✅ `clamp()` helper | **ENFORCED** |
| I5: Modal closes on successful create | ✅ `onOpenChange(false)` | **ENFORCED** |
| I6: Errors as `toast({ tone: "danger" })` + `InlineError` with `onRetry` | ✅ Pattern matches | **ENFORCED** |
| I7: Stop is gated by `ConfirmDialog` | ❌ Snippet has wrong `ConfirmDialog` API | **NOT ENFORCED** until B18 fix |
| I8: `connection_urls` may be absent; UI falls back to "Open detail" | ❌ Field names don't match server (M8); buttons will be `undefined` | **PARTIALLY ENFORCED** — fallback works, but the "non-null" branch never fires because of the field-name mismatch |

**Invariants Enforced: 6/8 = 75%** (was 25% → 75%)

---

## 📊 SCORECARD

| Category | Iter 1 | Iter 2 | Trend |
|----------|--------|--------|-------|
| Blocking Issues | 13 → 0 | 5 NEW | ❌ New failures |
| High Issues | 9 → 0 | 4 NEW | ❌ New failures |
| Medium Issues | 7 → ~3 fixed | 2 NEW | → |
| DoD Pass Rate | 0% | 36% (5/14) | ↑ |
| Invariants Enforced | 25% | 75% (6/8) | ↑ |
| Quality Standards | 0% | ~60% | ↑ |

**OVERALL VERDICT: ❌ FAIL — Iter 2 found 11 new issues; the snippets are MUCH closer to reality but still don't compile against the actual kit.**

---

## 🔧 FIXES NEEDED FOR ITERATION 2

### Must Fix (Blockers — code samples still do not compile)
1. **B15** — Either add `secondary` variant to the kit OR use `ghost`/`primary` consistently. Pick `ghost` for de-emphasis; reserve `primary` for the dominant action.
2. **B16** — Replace the `getServerAuth` server component with the real `auth()` + `orgId ?? userId` pattern from `app/[locale]/upgrade/page.tsx:65-76`.
3. **B17** — `params: Promise<{ locale: string; devenvId: string }>` + `await props.params` (Next 15).
4. **B18** — `ConfirmDialog` API: `danger` (boolean) not `tone="danger"`; `onClose` not `onOpenChange`.
5. **B19** — `Field` + `Input` pairing: drop the `label` prop on `Input` (Field renders the label); add `id` to `Input` + `htmlFor` to `Field`.

### Should Fix (High)
1. **H10** — `Field` has no `error`/`hint`; render error/hint outside the Field (sibling div) OR file a kit issue to add the props.
2. **H11** — `Tooltip content=` → `Tooltip label=`; drop redundant `aria-label` on disabled `<Button>` (the Tooltip wraps with a focusable span).
3. **H12** — `Skeleton` doesn't take `className`; restructure `DevenvSkeleton` to use multiple `Skeleton` instances with `width` props in a flex container.
4. **H13** — Drop unused `useParams` import in `DevenvDetailClient`.

### Nice to Fix (Medium)
1. **M8** — Rename `DevenvConnectionUrls.{vnc, tty, code}` → `{vnc_url, tty_url, code_url}` to match WP-08 OpenAPI. Update every destructure.
2. **M9** — Server component fetches `listDevenvs` only; client poll fetches `getDevenvStatus`. Document the 10s gap as known v1 limitation, or do a second server-side `getDevenvStatus` call to populate `initial.connection_urls`.

---

## NEXT STEPS

1. **Apply all 5 BLOCKING fixes (B15-B19)** — non-negotiable; the snippets still don't compile.
2. **Apply all 4 HIGH fixes (H10-H13)** — required for the WP to be buildable and lint-clean.
3. **Apply both MEDIUM fixes (M8, M9)** — M8 is a wire-contract correctness fix; M9 is a UX nit.
4. **Resolve Cross-WP-5** — the WS auth model in `window.open` is unresolved. WP-09 can land with the limitation documented, but DoD #5 cannot pass until one of Option A/B/C is committed.
5. **Add a unit-test file path + a Playwright spec path** (iter 1 M3 + M4 are still missing). Minimal:
   - `apps/admin-ui/src/components/devenv/DevenvList.test.tsx` — renders empty + 1-card cases
   - `apps/admin-ui/src/components/devenv/StatusBadge.test.tsx` — tone per status
   - `apps/admin-ui/src/lib/customer-client.test.ts` — `listDevenvs` unwraps `{ devenvs: [...] }`
   - `apps/admin-ui/e2e/devenv.spec.ts` — create → detail → stop
6. **Proceed to Iter 3 review** with the same rigor.
7. **Do NOT declare PASS until 2 consecutive iterations pass without new BLOCKING issues.** (This is the WP-01 bar.)

---

## 📊 ITERATION HISTORY

| Iter | Blocking | High | Medium | DoD Pass | Trend |
|------|----------|------|--------|----------|-------|
| 1    | 13       | 9    | 7      | 0/10     | baseline |
| 2    | 5 NEW    | 4 NEW | 2 NEW | 5/14     | ↓ (still failing, but gap closed from "every line broken" to "5 specific kit-API mismatches") |

**Convergence trajectory:** Two more iterations to reach PASS expected (iter 3 fixes the kit-API mismatches; iter 4 catches whatever the kit extension pulls in). 0/10 → 36/100 is real progress; the new issues are smaller and more focused.

---

**END OF WP-09 ITERATION 2 REVIEW**
