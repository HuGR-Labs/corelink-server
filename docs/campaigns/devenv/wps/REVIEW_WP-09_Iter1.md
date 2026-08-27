# WP-09 REVIEW — Iteration 1 (CRITICAL AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Verdict:** ❌ **FAIL — 13 BLOCKING ISSUES, 9 HIGH SEVERITY ISSUES, 7 MEDIUM SEVERITY ISSUES**

---

## Scope & Reality Check

WP-09 claims to build a DevEnv management UI in `apps/admin-ui`. Verified against
the actual repo:

- **No `features/devenv/` folder exists.** Real layout is `src/components/<area>/<Name>Client.tsx`
  (e.g. `RunnersClient.tsx`, `WorkspacesClient.tsx`). WP-09 invents an alien
  structure with a `features/` tree that does not exist anywhere in the codebase.
- **No `@tanstack/react-query` in `package.json`.** Repo uses `swr@2.4.2` + manual
  `useEffect` + `useState` + `useCallback` reload (see `WorkspacesClient.tsx:46-93`).
  WP-09 imports `useQuery`/`useMutation`/`useQueryClient` that do not exist in the
  dependency graph.
- **No `lucide-react` in `package.json` and zero `from "lucide-react"` imports in
  `src/`.** Icons are inline SVGs via the `I()` helper in `CustomerNav.tsx:23-26`.
  The Linear kit (`src/components/ui/linear/`) ships no icon set. WP-09 imports
  `ExternalLink, Monitor, Terminal, Code, Maximize2, Minimize2, RotateCcw, Play,
  Square, Loader2, AlertTriangle, Pause, Plus` — twelve missing icon modules.
- **No `apiClient` object with `.get`/`.post`/`.delete`.** Repo has:
  - `apiGet<T>(path, opts)` / `apiPost<T>(path, body, opts)` (named functions,
    `src/lib/api-client.ts:82-110`)
  - `CustomerClient` class (`src/lib/customer-client.ts`) with domain methods
    (`listWorkspaces`, `createPat`, `getRunnerEntitlement`, …) backed by
    `useCustomerClient()` (`src/lib/use-customer-client.ts:40-54`).
  WP-09 imports `apiClient` as a default import from `@/lib/api` — that module
  does not exist.
- **No `pages/` and no `routes/` folders.** Real layout is Next 15 App Router under
  `src/app/[locale]/`. WP-09 references `apps/admin-ui/src/pages/DevenvPage.tsx`
  and `src/routes/devenvRoutes.tsx` — neither path exists.
- **No `Button` with `variant="outline"` or `size="sm"`.** Linear kit `Button`
  (`src/components/ui/linear/Button.tsx:7-15`) variants are
  `primary | secondary | ghost | danger` and sizes `sm | md | lg`. WP-09 uses
  `variant="outline"`, `size="sm"`, and the wrong import path
  `@/components/ui/button` (singular, lowercase) — should be
  `@/components/ui/linear`.
- **No `Card` from `@/components/ui/card`.** Real path: `@/components/ui/linear/Card`.
- **No `Input` without a `label` prop.** `Input` (`src/components/ui/Input.tsx:5-11`)
  requires `label: string`. `ResizeControls.tsx` uses raw `<label>` + `<Input>`
  pair that will fail TypeScript strict mode (`Input` requires `label` not
  optional).
- **`useToast()` shape is wrong.** Real shape: `toast({ title, tone: "success" |
  "danger" | "neutral" })` (`ToastProvider.tsx:14-18`). WP-09 uses
  `toast.error("string")` (sonner-style API that does not exist in the repo).
- **`Badge` API wrong.** Linear `Badge` (`Badge.tsx:3-7`) takes
  `tone: "neutral" | "success" | "warn" | "danger"` + `dot?: boolean`. WP-09 uses
  `variant: "secondary" | "default" | "outline" | "destructive"` (shadcn API) and
  nests an `Icon` component. Will not compile.
- **No `@/components/ui/badge` and no `@/components/ui/button`.** Real path:
  `@/components/ui/linear` (barrel, see `linear/index.ts`).

**Bottom line:** every single code block in WP-09 imports modules, functions, and
types that do not exist in the repo. The WP is **non-buildable as written**.

---

## 🔴 BLOCKING ISSUES (Must fix before sign-off)

### B1. **Wrong package — no `@tanstack/react-query`**
- **Location:** `useDevenvList.ts:1`, `useDevenvDetail.ts:1`, `DevenvPage.tsx`
  (via implicit hook deps), `ResizeControls.tsx:6` (via `useDevenvActions`).
- **Problem:** Imports `useQuery`, `useMutation`, `useQueryClient` from
  `@tanstack/react-query`. `package.json` does not list it. The repo standardized
  on `swr@2.4.2` for data fetching and `useState + useEffect` for mutations
  (see `WorkspacesClient.tsx:46-93`, `KeysClient.tsx`).
- **Real pattern:** `const client = useCustomerClient(); const [data, setData] =
  useState<T>(); const [loading, setLoading] = useState(true); const [error,
  setError] = useState<unknown|null>(null); const reload = useCallback(async () =>
  { ... }, [client]); useEffect(() => { void reload(); }, [reload]);`
- **Fix:** Rewrite all hooks to match the `useCustomerClient` + manual `reload`
  pattern. Delete React Query scaffolding. Replace `useQuery` with the `useEffect
  + reload` template, replace `useMutation` with `useState` flags + try/finally.

### B2. **Wrong import — `@/lib/api` does not exist**
- **Location:** `api/devenv.ts:1` — `import { apiClient } from "@/lib/api"`.
- **Problem:** `@/lib/api` is not a path in the repo. The real options are:
  - `apiGet<T>(path, opts)` / `apiPost<T>(path, body, opts)` from
    `@/lib/api-client` (token-agnostic — caller passes `token` per call).
  - `CustomerClient` class from `@/lib/customer-client` (handles auth via
    `useCustomerClient`).
  - Domain-specific client (e.g. `CustomerClient.listWorkspaces()`) for
    "live" routes; for new surfaces the pattern is to add a method to
    `CustomerClient`.
- **Fix:** Add `devenv` methods to `CustomerClient` (mirroring
  `listWorkspaces`/`createWorkspace`) and consume via `useCustomerClient()`.
  Update the WP file structure: `src/lib/customer-client.ts` (add methods),
  `src/lib/customer-types.ts` (add types), `src/components/devenv/DevenvClient.tsx`
  (sibling of `RunnersClient.tsx`).

### B3. **`apiClient.get()`/`.post()`/`.delete()` are not functions**
- **Location:** `api/devenv.ts:155-189` (entire file).
- **Problem:** Uses a non-existent `apiClient` object with `.get`/`.post`/
  `.delete` methods. The real `apiGet`/`apiPost` are standalone async functions
  that take `(path, opts)` or `(path, body, opts)`. There is no `apiDelete`
  helper at all — DELETE in `CustomerClient` is done inline with
  `this.request(path, { method: "DELETE" })`.
- **Fix:** Rewrite to add `CustomerClient.listDevenvs()`, `createDevenv(input)`,
  `getDevenvStatus()`, `stopDevenv()`, `snapshotDevenv(force)`, `resizeDevenv(w,h)`,
  and `getDevenvConnectionUrls()`. Each goes through `this.request<T>()` which
  already handles auth + base URL + E2E bypass.

### B4. **API contract diverges from WP-08**
- **Location:** `types/devenv.ts:80-143` and `api/devenv.ts:165-181`.
- **Problem:** Multiple contract mismatches with `WP-08_Worker_Ingress_Routes.md`:
  - WP-08 says list returns `Devenv` with `devenv_id`, `workspace_name`,
    `profile_name`, `status`, `created_at`, `started_at`, `ports: number[]`.
    WP-09 type matches but `getStatus()` ignores `devenvId` and hard-codes the
    URL.
  - WP-08 says `POST /v1/customer/devenv` returns `201 { devenv_id, workspace_name,
    profile_name, status, vnc_url, tty_url, code_url }` (no `started_at`).
    WP-09 type omits `vnc_url`/`tty_url`/`code_url` from the LIST item but
    includes them only on create. **The list response shape differs from the
    create response shape, but the WP-09 `Devenv` type has no connection URLs.**
    A list-card then has no way to render "Open noVNC" without re-fetching.
  - WP-08 says `POST /v1/customer/devenv/resize` body is `{ width, height }`
    and is tenant-scoped (no `devenvId` in URL). WP-09's
    `resize(devenvId, w, h)` passes `devenvId` but never sends it — the
    signature is misleading.
  - WP-08 says `DELETE /v1/customer/devenv` (no id). WP-09's
    `stop(devenvId)` takes an ID and then drops it.
  - WP-08 says `GET /v1/customer/devenv/status` (no id). WP-09 passes `devenvId`
    then ignores it. The hard-coded `""` in `useDevenvDetail.ts:259` is a bug
    even within the WP-09 fiction.
- **Fix:** Drop the `devenvId` parameters (one DevEnv per tenant — `devenv_id`
  is always the tenant id, server-inferred). Align types byte-for-byte with
  WP-08 §3.2/§3.3. Note this as a cross-WP coordination finding.

### B5. **`getStatus()` hard-codes empty `devenvId` and ignores the param**
- **Location:** `useDevenvDetail.ts:259` — `const data = await devenvApi.getStatus("")`.
- **Problem:** Function signature `getStatus(devenvId: string)` accepts an id,
  URL never includes it, and the call site passes `""`. Either the function
  shouldn't take an id (per WP-08) or the URL should include it. Today it's both:
  wrong and silent.
- **Fix:** Remove the parameter. Call `client.getDevenvStatus()`.

### B6. **`useWebSocketConnection` hook declared but never used + missing export**
- **Location:** `hooks/useWebSocketConnection.ts` listed in §3.1; absent from
  §3.4 hook definitions. `ConnectionButtons.tsx` opens URLs via
  `window.open(url, "_blank_<name>", "noopener,noreferrer")` — it does NOT
  open WebSocket connections from the parent. The hook is dead code.
- **Problem:** Listed as a file in the file tree (3.1) but never specified
  in 3.4; downstream `ConnectionButtons.tsx` doesn't use it. Either specify it
  (and use it) or delete it.
- **Fix:** Delete the file entry from §3.1. If a WebSocket hook is actually
  needed (e.g. for a live status stream instead of polling), specify its full
  signature, lifecycle, and `close()` semantics.

### B7. **No `useDevenvDetail` in §3.4 — `useDevenvActions` reuses it**
- **Location:** `ResizeControls.tsx:6` imports `useDevenvResize` from
  `../hooks/useDevenvActions`; §3.1 lists `useDevenvDetail.ts` and
  `useDevenvActions.ts` as separate files but §3.4 specifies `useDevenvDetail`
  as a hand-rolled `useState + setInterval` hook (not React Query) and never
  defines `useDevenvActions`. There is a file/name mismatch.
- **Problem:** `useDevenvActions` is referenced but not defined; `useDevenvDetail`
  is defined but never imported by any component shown.
- **Fix:** Pick one file for mutation hooks: `useDevenvActions.ts` exports
  `useCreateDevenv`, `useStopDevenv`, `useSnapshotDevenv`, `useResizeDevenv`
  (all manual `useState` patterns). Move `useDevenvDetail` (now using
  `useCustomerClient` + `useEffect`) to its own file and have `DevenvDetail.tsx`
  actually use it.

### B8. **`useDevenvDetail` has stale closure + missing deps + dep array bug**
- **Location:** `useDevenvDetail.ts:267-279`.
- **Problem:** Three issues stacked:
  1. `useEffect(..., [enabled])` — `fetchStatus` is recreated on every render
     but the effect doesn't depend on it. The interval captures the first
     `fetchStatus` forever; if `apiClient` token changes, the polling call
     still uses the old closure.
  2. `useState` is imported from `"react"` but the import statement at line
     249 imports only `useQuery, useEffect, useRef` — `useState` is referenced
     on line 253 and 254 without import. **Compile error.**
  3. Effect cleanup `clearInterval(intervalRef.current)` — `intervalRef.current`
     is set AFTER `setInterval` returns, fine, but if the effect re-runs (e.g.
     `enabled` flips), the OLD interval is cleared by the cleanup of the
     previous run, but only if React hasn't already torn down the component.
     The interplay with `refetch` returned to the caller (which is the same
     function reference as `fetchStatus`) is fragile.
- **Fix:** Rewrite as the standard pattern: `const [status, setStatus] =
  useState(null); const [error, setError] = useState<Error|null>(null); const
  reload = useCallback(async () => { ... }, [client]); useEffect(() => {
  if (!enabled) return; void reload(); const id = setInterval(() => void
  reload(), 10_000); return () => clearInterval(id); }, [enabled, reload]);`

### B9. **Linear kit `Button` has no `variant="outline"` / `size="sm"`**
- **Location:** `ResizeControls.tsx:399,412,417` (`variant="outline"`,
  `size="sm"`).
- **Problem:** `Button.tsx:7-15` types `variant` as `"primary" | "secondary" |
  "ghost" | "danger"`. `size="sm"` actually exists, but the variant doesn't.
  The five preset buttons (Mobile/4K/etc.) will fail TS.
- **Fix:** Use `variant="secondary"` (closest visual to outline in the Linear
  kit) or define an outline variant. Add to the kit if needed.

### B10. **`Input` requires `label: string` — `ResizeControls` uses raw `<label>`**
- **Location:** `ResizeControls.tsx:372-393`.
- **Problem:** `Input` (`src/components/ui/Input.tsx:5-7`) requires a `label`
  prop. WP-09 wraps the Input in a raw `<label>` element. Either:
  - the Input would emit a duplicate label (the kit renders its own
    `<label htmlFor>` with the `label` prop).
  - the Input would render with no label (failing required prop).
  Both are wrong.
- **Fix:** Use `Input label="Width"` / `Input label="Height"`. Drop the manual
  `<label>` blocks. Or use `Field` (label + control + hint + error wrapper) for
  richer composition.

### B11. **`Badge` API is wrong — `variant` is shadcn, not Linear**
- **Location:** `StatusBadge.tsx:469-472` and throughout the file.
- **Problem:** `Badge` (`Badge.tsx:3-7`) takes `tone: "neutral" | "success" |
  "warn" | "danger"`. WP-09 uses `variant: "secondary" | "default" | "outline" |
  "destructive"` and renders an `<Icon>` component. Will not compile.
- **Fix:** Use `tone` mapping: starting→`warn`, running→`success`,
  stopping→`warn`, stopped→`neutral`, errored→`danger`. Use the `dot` prop
  instead of an icon (the kit only ships `<span class="lin-dot" />`).
  Re-implement with Linear's primitives: `<Badge tone="success" dot>Running</Badge>`.

### B12. **`Card` import path is wrong**
- **Location:** `DevenvPage.tsx:3` — `import { Card } from "@/components/ui/card"`.
- **Problem:** Path does not exist. Real path: `@/components/ui/linear` (barrel
  re-exports `Card`).
- **Fix:** `import { Card } from "@/components/ui/linear"`.

### B13. **`toast.error("string")` API doesn't exist**
- **Location:** `ConnectionButtons.tsx:302`, implicit everywhere.
- **Problem:** `useToast()` returns `{ toast: (t: ToastInput) => void }` where
  `ToastInput = { title: string; description?: string; tone?: "neutral" |
  "success" | "danger" }` (`ToastProvider.tsx:14-18`). There is no
  `toast.error()`, `toast.success()`, etc.
- **Fix:** `toast({ title: `DevEnv is not running (status: ${status})`, tone:
  "danger" })`. Apply to every site.

---

## 🟠 HIGH SEVERITY ISSUES

### H1. **WP invents an `apps/admin-ui/src/features/devenv/` tree that does not exist**
- **Location:** §3.1 file structure.
- **Problem:** The repo organizes features by `src/components/<area>/<Name>Client.tsx`
  (e.g. `RunnersClient.tsx`, `WorkspacesClient.tsx`) and shares types in
  `src/lib/customer-types.ts`. The proposed `features/devenv/{components,hooks,
  api,types}` tree is foreign to the codebase. Adopting it would create a
  parallel structure that future contributors must learn.
- **Fix:** Conform to the existing layout. Files:
  - `src/components/devenv/DevenvList.tsx` + `DevenvListClient.tsx` (consumer)
  - `src/components/devenv/CreateDevenvModal.tsx`
  - `src/components/devenv/DevenvDetail.tsx` + `DevenvDetailClient.tsx`
  - `src/components/devenv/ConnectionButtons.tsx`
  - `src/components/devenv/StatusBadge.tsx`
  - `src/components/devenv/ResizeControls.tsx`
  - `src/components/devenv/DevenvSkeleton.tsx` (use `Skeleton` from linear)
  - `src/lib/customer-types.ts` — add `Devenv`, `DevenvStatus`, `CreateDevenvInput`, `DevenvSnapshot`, `DevenvResize` (the existing `customer-types.ts` is the canonical home).
  - `src/lib/customer-client.ts` — add `listDevenvs()`, `createDevenv(input)`,
    `getDevenvStatus()`, `stopDevenv()`, `snapshotDevenv(force)`, `resizeDevenv(w,h)`,
    `getDevenvConnectionUrls()`.
  - `src/app/[locale]/(authenticated)/devenv/page.tsx` (Next App Router — the
    repo uses `[locale]`, not bare `app/`).

### H2. **`useEffect` deps violation in `useDevenvDetail` (line 276)**
- **Location:** `useDevenvDetail.ts:267-276`.
- **Problem:** `useEffect(() => { ... }, [enabled])` calls `fetchStatus` and
  `setInterval(fetchStatus, ...)` without listing it in deps. ESLint
  `react-hooks/exhaustive-deps` will fail the build.
- **Fix:** Memoize `fetchStatus` via `useCallback([client])`, list it in deps.

### H3. **No `enabled` gate for poll when not on detail page**
- **Location:** `useDevenvDetail.ts:267-279`, `useDevenvList.ts:201-208`.
- **Problem:** The list hook polls every 30s unconditionally even when the
  user is on a different page. `useDevenvList` is called from `DevenvList.tsx`
  which is itself called from `DevenvPage.tsx` — if `DevenvPage` unmounts
  (user navigates away), the query refetch stops only if React Query is in
  use. With the manual pattern, the `setInterval` lives in the hook and
  does NOT auto-pause. **Real risk:** 10s polling × N tenants = constant
  background fetch load on the Worker.
- **Fix:** Add `enabled: boolean` to `useDevenvList` (default true). Mount
  the polling only when the DevenvPage is active. When refactored to SWR, use
  `useSWR(..., { refreshInterval: ..., revalidateOnFocus: true })` which
  auto-pauses on unmount.

### H4. **`ConnectionButtons` opens WS URLs in new tabs but does not pass auth**
- **Location:** `ConnectionButtons.tsx:300-306` — `window.open(url, ...)`.
- **Problem:** The noVNC/ttyd/code-server WebSocket connections live behind
  the same Clerk auth as the REST API. Opening a `blob:` URL or a raw
  `/v1/customer/devenv/vnc` in a new tab will hit a CORS/auth wall. The
  Worker proxy expects an `Authorization: Bearer …` header on the upgrade
  request, which `window.open` cannot supply. The new tab will show
  "Unauthorized" or the WS will fail.
- **Fix:** Options:
  - Mint short-lived signed upgrade tokens server-side (WP-08 must add a
    `GET /v1/customer/devenv/upgrade-token?kind=vnc` route) and pass
    `?token=…` on the URL.
  - Or render the noVNC/ttyd/code client inline (iframe with `sandbox` and
    `postMessage` auth handshake) — heavier but more secure.
  - Document which the WP picks and call out the auth-model gap to WP-08.

### H5. **`window.open` `name` arg starts with `_` — invalid**
- **Location:** `ConnectionButtons.tsx:305` — ``window.open(url, `_blank_${name}`, …)``.
- **Problem:** `window.open`'s second arg is the target name. Targets starting
  with `_` are reserved (`_blank`, `_self`, `_parent`, `_top`). Browsers will
  silently treat `` `_blank_vnc` `` as `_blank` — three new tabs will land
  in the same single new tab if clicked in sequence, **overwriting each other**.
- **Fix:** `window.open(url, "devenv_vnc", "noopener,noreferrer")` (unique
  per channel) OR just `window.open(url, "_blank", "noopener,noreferrer")`
  to always open a fresh tab.

### H6. **No skeleton loading component defined + Linear `Skeleton` exists**
- **Location:** §3.1 lists `DevenvSkeleton.tsx`; the file is never specified
  in §3.5.
- **Problem:** Completeness checklist line 564 requires a skeleton, but no
  implementation is given. The repo has `Skeleton` in
  `@/components/ui/linear/Skeleton`.
- **Fix:** Add a `DevenvSkeleton` snippet using `<Skeleton />` from the Linear
  kit. Show 3 placeholder rows matching the DevenvCard layout.

### H7. **No `ConfirmDialog` for the destructive Stop action**
- **Location:** `ConnectionButtons.tsx`, missing entirely.
- **Problem:** Stopping a DevEnv is destructive (kills the container, may
  lose unsaved work). Repo pattern: `WorkspacesClient.tsx` gates delete behind
  `ConfirmDialog`. WP-09 has no confirmation step.
- **Fix:** Add a Stop button to `DevenvDetail.tsx` (or a `DevenvCard.tsx`
  menu) that opens a `ConfirmDialog` with `tone="danger"` before calling
  `stopDevenv()`.

### H8. **Cross-WP contract drift: `started_at` field name + semantics**
- **Location:** `types/devenv.ts:94` and `DevenvStatusResponse` shape.
- **Problem:** WP-08 §3.3 schema says `started_at: integer, nullable`.
  WP-09 type matches. But `DevenvStatusResponse` (the detail payload) has
  `uptime_ms: number | null` which WP-08 does NOT define. Where does
  `uptime_ms` come from? `container_alive` is also not in WP-08's
  `DevenvStatus` schema. **WP-09 invents server fields.**
- **Fix:** Remove `uptime_ms`, `container_alive`, `ws_connections`,
  `health_check_failures`, `last_check_at` from `DevenvStatusResponse` OR
  flag them as `[not-wired]` and add a TODO referencing the WP that will
  close them. **Cross-WP coordination finding.**

### H9. **Invariant duplicates + bad numbering**
- **Location:** §5 (Invariants) — the table has `I3` and `I4` twice.
- **Problem:** The table has `I1, I2, I3, I4, I3, I4` — invariant IDs are
  not unique. The first `I3` is "Connection buttons only enabled when
  status === running" and the second `I3` is "Modal closes on successful
  create". The first `I4` is "Resize inputs clamped to 1-8192" and the
  second `I4` is "Error toasts auto-dismiss after 5s". The Linear `Toast`
  does NOT auto-dismiss at a fixed 5s; it auto-dismisses on user action
  or when the caller calls `dismiss()` (or after a default duration the
  caller can set via `duration`). **The invariant claims behavior the
  primitive doesn't ship.**
- **Fix:** Renumber to `I1…I6`. Either drop the "auto-dismiss 5s" claim
  or document that `ToastProvider` must be enhanced to honor it. Verify
  current behavior in `ToastProvider.tsx` (lines 38-44 setToasts after
  5s) — if so, the invariant holds; if not, the invariant is false.

---

## 🟡 MEDIUM SEVERITY ISSUES

### M1. **Quality Standards claim "Mobile-first; breakpoints at 640px, 1024px" but no Tailwind config referenced**
- **Location:** §5 Quality Standards table.
- **Problem:** Tailwind v4 (per `package.json`) uses arbitrary breakpoints.
  "640px, 1024px" maps to `sm:` and `lg:` by default — fine, but the WP
  never uses `sm:`/`lg:` prefixes in the example code. No `flex-col sm:flex-row`
  or `grid-cols-1 lg:grid-cols-2`. Mobile responsiveness is claimed but not
  implemented in the snippets.
- **Fix:** Add `flex-col sm:flex-row` to `ConnectionButtons` wrapper, and
  `grid-cols-1 lg:grid-cols-2` to ResizeControls grid. Document the
  breakpoints used.

### M2. **No `Field`/`InlineError`/`EmptyState` in scope**
- **Location:** `CreateDevenvModal`, `DevenvList`, `DevenvDetail` (referenced
  by checklist but not shown).
- **Problem:** Form validation should use `Field` (label + input + hint +
  error wrapper) and surface `InlineError` on API failure. Empty list should
  use `EmptyState`. WP-09 mentions none of these.
- **Fix:** Add `Field` + `InlineError` to the `CreateDevenvModal` snippet.
  Add `EmptyState` (with `cta` slot for the "New DevEnv" button) to the list
  empty branch.

### M3. **No unit test skeleton**
- **Location:** §6 Completeness Checklist line 573.
- **Problem:** Says "Unit tests: components, hooks, API" but no test file
  is specified. Repo convention: `<Name>.test.tsx` next to source
  (`Button.test.tsx`, `WorkspacesClient` is tested via Playwright E2E).
- **Fix:** Add at least:
  - `DevenvList.test.tsx` — renders 3 cards from mock, renders empty state
    when zero, renders skeleton on loading.
  - `StatusBadge.test.tsx` — tone per status.
  - `useDevenvList.test.ts` — calls `client.listDevenvs` once on mount,
    re-fetches on interval, surfaces `CustomerClientError` to `error`.
  - `customer-client.test.ts` — verifies `createDevenv` unwraps the envelope
    and sends auth header (mirroring `createPat` envelope-unwrap pattern at
    `customer-client.ts:172-178`).

### M4. **No Playwright E2E spec**
- **Location:** §6 Completeness Checklist line 574.
- **Problem:** Lists "E2E tests: create → connect → resize → stop" but
  doesn't give a path or a mock fixture. The repo has
  `src/lib/e2e-mock-fixtures.ts` to back the catch-all `/api/v1/...` route.
- **Fix:** Specify `apps/admin-ui/e2e/devenv.spec.ts` with steps and
  document the mock fixtures required (`mockDevenvs`, `mockDevenvStatus`).

### M5. **Sign-Off table has no "Code Owner" or "QA" rows**
- **Location:** §7 Sign-Off.
- **Problem:** Other WP reviews (e.g. REVIEW_WP-01_Iter1.md) sign off
  through TechLead. WP-09 has Frontend TL + TechLead only. Add a QA row
  if E2E is part of the DoD.
- **Fix:** Add `QA (E2E owner)` row.

### M6. **Risk register misses critical risks**
- **Location:** §8 Risk Register.
- **Problem:** Missing risks:
  - **Container WS auth (HIGH):** the connection buttons can't open the
    WS without a token (see H4).
  - **Quota race:** WP-08 enforces `enforceDevenvQuota` but UI doesn't
    display remaining quota; user gets 403 with no UX guidance.
  - **Clerk token expiry mid-poll:** `useCustomerClient` always mints
    fresh tokens, OK, but if the user is idle for >60 min the WS upgrade
    will fail. Document re-auth flow.
- **Fix:** Add the three risks with mitigation pointers.

### M7. **Self-Check 2 claims `target="_blank"` but the code uses `window.open`**
- **Location:** §7 Self-Check 2, line 595.
- **Problem:** Verification says "Buttons open in new tab
  (`target="_blank"`)". The implementation uses `window.open` with a
  dynamic target name. The two are not equivalent (`window.open` can be
  popup-blocked, `<a target="_blank">` is not; conversely, `window.open`
  can pass window features the link cannot).
- **Fix:** Either render an `<a href={url} target="_blank" rel="noopener
  noreferrer">` (preferred for CSP popup policy) or correct the
  self-check language.

---

## 📋 DoD Gap Analysis

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | DevEnv list loads and displays status | ❌ **Cannot build** | Wrong component lib, wrong data-fetching pkg |
| 2 | Create modal validates workspace name | ❌ **Cannot build** | `Input` API wrong, no `Field` snippet |
| 3 | Create DevEnv shows loading then success | ❌ **Cannot build** | `toast.error` doesn't exist; Linear toast signature wrong |
| 4 | DevEnv card shows real-time status | ❌ **Cannot build** | Hooks import non-existent `useQuery` |
| 5 | Connection buttons open correct URLs | ❌ **FAIL** | Auth model not solved (H4); target name invalid (H5) |
| 6 | Buttons disabled when not running | ⚠️ **Partial** | `disabled` attr present but Linear `Button` uses `aria-disabled` only when `asChild`; needs explicit `disabled` prop on `<button>` (kit does this OK at `Button.tsx:60-65`) |
| 7 | Resize controls apply dimensions | ❌ **FAIL** | `Button` variant="outline" doesn't exist; `Input` requires label |
| 7 (dup) | Status badge colors match spec | ❌ **FAIL** | `Badge` `variant` doesn't exist |
| 8 | Empty state shows helpful message | ❌ **FAIL** | No `EmptyState` snippet |
| 9 | Error states handled gracefully | ❌ **FAIL** | `toast.error` doesn't exist; no `InlineError` snippet |
| 10 | Mobile responsive | ❌ **FAIL** | No `sm:`/`lg:` prefixes in snippets |

**DoD Score: 0/10 PASS, 1/10 PARTIAL, 9/10 FAIL** — 0% achievable without a rewrite.

---

## 📋 Invariants Verification

| Invariant | Enforced in Code? | Verdict |
|-----------|-------------------|---------|
| I1: All API calls include auth | ⚠️ Real `CustomerClient.request` sets `authorization: Bearer <token>` (line 102-103). The WP-09 `apiClient` is invented and would NOT enforce this. | **ENFORCED only after rewrite to use CustomerClient** |
| I2: Polling interval 10s detail, 30s list | ⚠️ Hard-coded in hook snippets but the hooks don't compile | **NOT ENFORCED** (code can't run) |
| I3 (dup 1): Buttons only enabled when `status === "running"` | ✅ The check is there in the snippet | **ENFORCED** |
| I3 (dup 2): Modal closes on successful create | ❌ `CreateDevenvModal` is not specified; cannot verify | **NOT VERIFIED** |
| I4 (dup 1): Resize inputs clamped to 1-8192 | ✅ `Math.min(8192, Math.max(1, ...))` present | **ENFORCED** |
| I4 (dup 2): Error toasts auto-dismiss after 5s | ❌ Linear toast does not auto-dismiss; would need an `setTimeout` wrapper | **NOT ENFORCED** |
| I5: `vnc_url`/`tty_url`/`code_url` come from API | ❌ `getConnectionUrls()` returns hard-coded paths, not server-issued URLs | **NOT ENFORCED** |
| I6: `devenv_id` present in `Devenv` type | ✅ Present | **ENFORCED** |

**Invariants Enforced: 2/8 (25%)** — INSUFFICIENT. Plus 2 are duplicate IDs.

---

## 📋 Quality Standards Verification

| Standard | Met? | Evidence |
|----------|------|----------|
| Type Safety: Zero `any` in components | ❌ **Fail** | `useDevenvDetail.ts` uses `as Error` (cast), `ResizeControls.tsx` parses to `number` without NaN guard |
| Accessibility: ARIA labels; keyboard nav; focus management | ⚠️ **Partial** | `Modal` (Radix Dialog) handles focus trap; `Input` has label. But no `aria-label` on connection buttons describing target (`<Button>noVNC Desktop</Button>` has no `aria-label` saying "opens in new tab") |
| Performance: caching; no unnecessary re-renders | ⚠️ **Partial** | `useCustomerClient` already uses the latest-ref pattern. But the proposed hooks return fresh functions on every render (`refetch = fetchStatus` recreated) |
| Error Handling: toasts + retry + graceful degradation | ❌ **Fail** | `toast.error` API doesn't exist; no `InlineError` in `DevenvList`; no `onRetry` |
| Responsive: Mobile-first; breakpoints 640/1024 | ❌ **Fail** | No breakpoint prefixes in any snippet |

**Quality Standards: 0/5 MET, 2/5 PARTIAL** — INSUFFICIENT.

---

## 📋 Self-Check Points Analysis

### Self-Check 1: Real-Time Updates
- [ ] List polls every 30s; detail polls every 10s — ⚠️ Constants present, hooks don't compile
- [ ] Status badge updates without full page reload — ❌ Will require full page render since `useEffect`/`setInterval` doesn't update atomically
- [ ] Connection buttons enable/disable based on status — ✅ Logic present
- [ ] Manual refresh button available — ❌ No refresh button in any snippet

**Verdict: 1/4 PASS, 1/4 PARTIAL, 2/4 FAIL**

### Self-Check 2: WebSocket Connection UX
- [ ] Buttons open in new tab (`target="_blank"`) — ❌ Uses `window.open` with invalid name
- [ ] `rel="noopener noreferrer"` for security — ⚠️ `window.open` features string includes `noopener,noreferrer` but that's a different mechanism
- [ ] Disabled state with tooltip when not running — ❌ No Tooltip wired (kit has Tooltip)
- [ ] Loading state while connecting — ❌ Not shown; button doesn't reflect "Connecting…"

**Verdict: 0/4 PASS, 1/4 PARTIAL, 3/4 FAIL**

### Self-Check 3: Error Handling Completeness
- [ ] API errors → toast with message + retry button — ❌ `toast.error` doesn't exist; no retry button
- [ ] Network errors → "Connection lost" banner + auto-retry — ❌ No banner component
- [ ] Validation errors → inline field messages — ⚠️ Possible via `Field` but not shown
- [ ] 403 quota → modal with upgrade link — ❌ No upgrade modal

**Verdict: 0/4 PASS, 1/4 PARTIAL, 3/4 FAIL**

---

## 📊 SCORECARD

| Category | Score | Required | Gap |
|----------|-------|----------|-----|
| Blocking Issues | 13 | 0 | **-13** |
| High Issues | 9 | 0 | **-9** |
| Medium Issues | 7 | 0 | **-7** |
| DoD Pass Rate | 0% | 100% | **-100%** |
| Invariants Enforced | 25% | 100% | **-75%** |
| Quality Standards | 0% | 100% | **-100%** |
| Self-Check Pass | 8% | 100% | **-92%** |

**OVERALL VERDICT: ❌ FAIL — Requires complete rewrite of code samples + data layer.**

---

## 🔧 FIX PRIORITY

### Must Fix (Blockers — code samples do not compile)
1. **B1, B2, B3** — Replace `@tanstack/react-query` + invented `apiClient` with
   the real `useCustomerClient` + `CustomerClient` methods.
2. **B4, B5** — Conform to WP-08's actual contract (one DevEnv per tenant;
   `devenvId` removed).
3. **B9, B10, B11, B12, B13** — Switch to Linear kit API (`tone`, no
   `variant`; `Input` requires `label`; `toast({ title, tone })`).
4. **B6, B7, B8** — Delete dead hooks, fix the missing `useState` import,
   memoize `fetchStatus`.

### Should Fix (High)
1. **H1** — File structure must match the repo (`components/devenv/`, types
   in `lib/customer-types.ts`, client in `lib/customer-client.ts`).
2. **H4, H5** — WebSocket auth model + valid `window.open` target name.
3. **H7, H8** — Add `ConfirmDialog` for Stop; reconcile invented status
   fields with WP-08.
4. **H9** — Renumber invariants; verify the "auto-dismiss 5s" claim.

### Nice to Fix (Medium)
1. **M1, M2, M3, M4** — Responsive classes, `Field`/`EmptyState`/`InlineError`,
   unit + E2E test scaffolds.
2. **M5, M6, M7** — Sign-off rows, additional risks, self-check language.

---

## NEXT STEPS

1. **Apply all BLOCKING fixes** (B1–B13) — non-negotiable.
2. **Apply all HIGH fixes** (H1–H9) — required for the WP to be buildable.
3. **Apply at least 4 of 7 MEDIUM fixes** (M1, M2, M3, M4 minimum).
4. **Coordinate with WP-08** on the `devenvId` removal + connection-URL
   auth model (cross-WP finding).
5. **Coordinate with WP-07** on the 403 quota UX (cross-WP finding).
6. **Re-verify all 10 DoD items pass after the rewrite.**
7. **Proceed to Iteration 2 review only when the snippets compile and
   the data layer is the real `CustomerClient`.**

**Do NOT mark WP-09 sign-off-ready until the rewrite lands.**

---

## CROSS-WP COORDINATION NEEDS

| Need | Direction | Impact |
|------|-----------|--------|
| Drop `devenvId` from URL paths | WP-08 → WP-09 | One DevEnv per tenant; ID is the tenantId and is server-inferred |
| Connection-URL auth model | WP-08 → WP-09 | Either signed upgrade tokens OR inline iframe; today neither works |
| DevenvStatusResponse field set | WP-08 → WP-09 | WP-09 invents `uptime_ms`, `container_alive`, `ws_connections`, `health_check_failures`, `last_check_at`; align with WP-01/06 status payload |
| Quota 403 → upgrade modal | WP-07 → WP-09 | UI should show a quota-exhausted empty state linking to `/upgrade?plan=…` |
| Resize + snapshot in DevenvDetail | WP-06 → WP-09 | Confirm whether the Worker has a single `getDevenvStatus` that returns both `running`-time ports AND `stopped`-time state, or two separate endpoints |
| Toast auto-dismiss | Linear kit owner → WP-09 | Confirm or extend the kit to auto-dismiss at 5s |

---

**END OF WP-09 ITERATION 1 REVIEW**
