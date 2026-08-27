# WP-04 REVIEW — Iteration 2 (CRITICAL AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Previous Verdict (per prompt):** ⚠️ PASS-WITH-CROSS-WP-BLOCKERS (iter 1 found 9B/11H/6M; "exec-server on port 8080" fix claimed)  
**Verdict:** ❌ **FAIL — Iter 1 review on disk is much harsher than the prompt's narrative; current WP-04 has 0/9 iter 1 BLOCKING fixes applied. New iter 2 review finds 7 NEW BLOCKING, 6 NEW HIGH, 3 MEDIUM issues.**

> **CRITICAL META-FINDING:** The narrative in the task prompt — "iter 1 fix was to use an in-container exec-server on port 8080" — is **inverted** vs. the on-disk state. WP-06 (the consumer) is the WP that was CORRECTED to use `this.containerFetch("http://localhost:8080/clw", ...)` (WP-06 §3.1.1 exec-server contract). WP-04 (the producer of those calls) was **NOT corrected**: `WP-04:117` still calls `this.ctx.container.exec(...)` and `WP-04:124` still calls `execProcess.output()` — APIs that do **not exist** on the `@cloudflare/containers` SDK (verified against WP-06 iter 1 review B1). WP-04 is the WP that needs the port-8080 exec-server FIX, not the WP that has the fix. The cross-WP exec contract is documented in WP-06 §3.1.1 — WP-04 ignores it.

---

## 🔴 UNFIXED ITER 1 BLOCKING ISSUES (9/9 NOT APPLIED)

None of the 9 BLOCKING issues from `REVIEW_WP-04_Iter1.md` have been corrected in the on-disk `WP-04_clw_Integration.md`. Re-verification:

| Iter 1 ID | Issue | Status in current WP-04 |
|---|---|---|
| **B1** | `this.envVars` does not exist | ⚠️ PARTIALLY — WP-04 now reads `this.state.workspaceName` / `this.state.profileName` (good), but `this.env.VCPU_COUNT` (line 505) is also undeclared; **`this.state.clwTenant` is read on line 510 but NOT in WP-01's union** |
| **B2** | `clwTenant` not in `DevenvState` | ❌ STILL BROKEN — `this.state.clwTenant` referenced on line 510; WP-01's union has no such field; compile error |
| **B3** | Hydrate JSON had chunks_total/chunks_uploaded | ✅ FIXED — separate `HydrateMetadata` interface (lines 60-67); uses `bytesFromCache`/`bytesDownloaded` |
| **B4** | `clw exec` subcommand doesn't exist | ❌ STILL BROKEN — but for a DIFFERENT reason now: WP-04:117 still uses `this.ctx.container.exec()` (the SDK method), not `clw exec`. The mkdir-via-clw-exec line is gone, but the wrong-API exec call remains. |
| **B5** | Hard-coded `--ref-domain runner` | ❌ STILL HARDCODED — lines 160, 197 |
| **B6** | Hydrate exit 2 mapped to "not found" | ✅ FIXED — replaced with `clwRefExists` pre-check (line 153) |
| **B7** | State-machine bypass (`this.state = ...` direct) | ❌ STILL BROKEN — line 240 (`this.state = { ...this.state, snapshotInProgress: true }`) and line 245 directly mutate state; `onStart` line 286 calls `transitionState` but `onStop` (line 336) and `onError` (line 412) also call `transitionState`; the `snapshot()` RPC at line 459 calls `transitionState`. **But onError line 412 builds `next` with `status: "errored" as const` (line 377) which is the errored arm of WP-01's union — and that arm does NOT carry `lastProfileSnapshot` / `lastWorkspaceSnapshot` / `workspaceName` / `profileName` (WP-01:126-130).** TypeScript compile error. |
| **B8** | `SnapshotRequest`/`SnapshotResponse` not from shared types | ⚠️ PARTIALLY — WP-04 line 54 imports `SnapshotMetadata, SnapshotRequest, SnapshotResponse` from `../types/devenv.js` (good) but the implementations at lines 434-483 and 469-479 use a DIFFERENT shape than the WP-01 `SnapshotResponse` type (WP-01:156-160 says `ok: true` and `profileSnapshot: {root: string; bytesTotal: number}` — matches, but does not match WP-04:469 which has `ok: settled.every(s => s.status === "fulfilled")` returning `boolean`, not `true` literal). |
| **B9** | `acquireSnapshotLock` uses non-existent `snapshotInProgress` field | ❌ STILL BROKEN — `this.state.snapshotInProgress` (line 237) not in WP-01's union; will not typecheck. The crash-recovery in `initializeState` (line 534) reads the same non-existent field. |

**Iter 1 BLOCKING fix application rate: 2/9 (22%)** — B3 and B6 applied; B1 partial; B2, B4, B5, B7, B8, B9 NOT applied.

---

## 🔴 NEW BLOCKING ISSUES (Iter 2 discoveries)

### B10. **`this.ctx.container.exec()` does NOT exist on the SDK — fatal compile/runt ime error**
- **Location:** Line 117, `execClw` method
- **Code:** `const execProcess = await this.ctx.container.exec({cmd: [CLW_BIN, ...args], timeoutMs, stdout: capture ? "pipe" : "ignore", stderr: capture ? "pipe" : "ignore"});`
- **Problem:** Per WP-06 iter 1 B1 (`REVIEW_WP-06_Iter1.md` would document this) and the canonical cross-WP contract in **WP-06 §3.1.1**: the `@cloudflare/containers` SDK exposes `this.containerFetch(port, req)` (HTTP fetcher to a container port) and `super.fetch(req)` (proxy through), but **NOT** `this.ctx.container.exec(cmd)` and **NOT** `execProcess.output()`. WP-04 uses both. The SDK source confirms `ctx.container` is not a Container instance with `.exec()` — it is the parent `Container` class itself.
- **Real API (per WP-06 §3.1.1):** Call the in-container exec-server over HTTP: `await this.containerFetch("http://localhost:8080/clw", { method: "POST", headers: {"Content-Type": "application/json"}, body: JSON.stringify({argv: [CLW_BIN, ...args]}) }, 8080)`. The response is `{"exit_code": number, "stdout": string, "stderr": string}` (note: the server's `stdout`/`stderr` are already strings, NOT ArrayBuffers — there is NO `output()` to await).
- **Cascade:** The `output()` call on line 124 is doubly wrong: the SDK doesn't have it, AND the exec-server response shape returns pre-decoded strings.
- **Fix:** Replace `execClw` body to call `this.containerFetch("http://localhost:8080/clw", ...)` per WP-06 §3.1.1 contract; decode the `{exit_code, stdout, stderr}` JSON response. The exec-server is a WP-02/03 deliverable (port 8080, alongside code-server).
- **Cross-WP:** This fix is the WHOLE POINT of WP-06 §3.1.1 — the exec-server was designed so WPs that need to run commands inside the container (WP-04, WP-06) have a clean, non-bash-dependent path. WP-02/03 must implement the server; WP-04/06 must consume it.

### B11. **`clwTenant` and `snapshotInProgress` not in `DevenvState` discriminated union — compile error**
- **Location:** `computeUsageEvent` line 510 (`(this.state as { clwTenant: string }).clwTenant`); `acquireSnapshotLock` line 237 (`this.state.snapshotInProgress`); `releaseSnapshotLock` line 245; `initializeState` line 534; `onError` line 392-397 (`lastProfileSnapshot: SnapshotMetadata | null` cast); `snapshot()` RPC lines 452-457.
- **Problem:** WP-01's `DevenvState` (WP-01:96-130) is a strict 5-arm discriminated union with no `clwTenant`, no `snapshotInProgress`, no `lastProfileSnapshot`, no `lastWorkspaceSnapshot` fields. WP-04 reads all four and uses cast-through `as { ... }` to bypass the type system. Per WP-06 iter 1 M9: snapshot metadata was moved to **side-table keys** (`lastProfileSnapshot`, `lastWorkspaceSnapshot` are separate `ctx.storage.put` keys, NOT state fields). WP-04 contradicts WP-06 by putting them back in state.
- **Real API:** Per WP-06 §3.3: `STATE_KEY`, `PROFILE_SNAPSHOT_KEY`, `WORKSPACE_SNAPSHOT_KEY` are three distinct `ctx.storage` keys. WP-04 must (a) move `lastProfileSnapshot` / `lastWorkspaceSnapshot` to side-tables and use `ctx.storage.get/put` for them, (b) ADD `clwTenant` to the union (since billing needs it, per WP-07), and (c) put `snapshotInProgress` in its OWN side-table key, not in `DevenvState` (the union was designed to be portable across WP-01/04/06/07; injecting a transient flag would re-pollute the state shape).
- **Cross-WP:** This is the WP-01 ↔ WP-04 ↔ WP-06 ↔ WP-07 cross-cutting contract. WP-06 §3.3 already defines the key naming convention; WP-04 must align.
- **Fix:** Add `clwTenant: string` to WP-01's union (the "starting" / "running" / "stopping" arms — same as `workspaceName`); introduce a `SnapshotLockKey = "snapshotInProgress"` side-table boolean; read/write `lastProfileSnapshot` / `lastWorkspaceSnapshot` via `ctx.storage.get/put` (not state).

### B12. **`onError` writes `lastProfileSnapshot` / `lastWorkspaceSnapshot` into state, but the `errored` arm of the union has neither — compile error AND runtime invariant violation**
- **Location:** `onError` lines 391-398
- **Code:**
  ```typescript
  if (settled[0].status === "fulfilled") {
      (next as { lastProfileSnapshot: SnapshotMetadata | null }).lastProfileSnapshot = settled[0].value;
  }
  ```
- **Problem:** WP-01's `errored` arm (WP-01:126-130) is `{status: "errored", createdAt, lastError, lastWorkspaceName}` — no `lastProfileSnapshot`, no `lastWorkspaceSnapshot`, no `workspaceName`, no `profileName`. WP-04 constructs a `next` object (lines 375-379) with `status: "errored" as const` and TRIES to mutate it with the snapshot metadata. TypeScript would reject this, but WP-04's `as { lastProfileSnapshot: ... }` cast masks the error. At runtime, the persisted state is a JSON blob that violates the union contract — and a subsequent read via `this.state.workspaceName` on a (supposedly) `errored` state would return `undefined`, crashing any `computeUsageEvent` call (line 506: `(this.state as { startedAt: number | null }).startedAt`).
- **Real API:** Persist the snapshot metadata to the side-table keys (WP-06 §3.3). The `errored` arm keeps `lastWorkspaceName` (WP-01:129) so the operator dashboard can show "last seen workspace" — the snapshot metadata is structural observability, not state-machine data.
- **Fix:** In `onError`, do NOT add `lastProfileSnapshot` / `lastWorkspaceSnapshot` to the `next` state object. Instead, after `transitionState({ status: "errored", ... })` succeeds, call `await this.ctx.storage.put("lastProfileSnapshot", profileMeta)` / `await this.ctx.storage.put("lastWorkspaceSnapshot", workspaceMeta)` conditionally (matching WP-06 §3.1 `onStop` lines 358-359).

### B13. **WP-04's `recordUsage` (lines 501-516) is REPLACED by WP-07 §3.2 — duplicate, conflicting definitions**
- **Location:** WP-04 §3.2 lines 501-516 (`computeUsageEvent` + `recordUsageForDevEnv`); WP-07 §3.2 lines 111-186.
- **Problem:** WP-07 was iter 1 reviewed and explicitly REJECTED the WP-04 /v1/usage endpoint and the `recordUsage` body. The canonical path is `/internal/v1/billing/usage` with `X-Corelink-Internal-Auth`, `idem_key = BLAKE3-256(tenant|container|startedAt)`, `event_kind: "devenv_vcpu_seconds"`, and `qty: vcpuSeconds` (NOT a custom `{tenant_id, container_id, vcpu_seconds, period_start, period_end}` shape). WP-04's `computeUsageEvent` is now orphaned — WP-06 §3.6 line 587 declares `recordUsage()` as a STUB that calls the canonical billing path (WP-07 owned); WP-04 should NOT define `recordUsageForDevEnv` or `computeUsageEvent` at all.
- **Cascade:** The two definitions disagree on:
  1. **URL:** WP-04's `recordUsageForDevEnv` is undefined but `computeUsageEvent` returns `{tenantId, containerId, vcpuSeconds, periodStart, periodEnd}` — none of which are in the WP-07 canonical schema. WP-07 expects `[{tenant_id, event_kind, qty, billing_period, region, source, time_ms, idem_key}]`.
  2. **Auth:** WP-04 leaves auth to the caller; WP-07 requires `X-Corelink-Internal-Auth: BILLING_INGEST_AUTH_KEY`.
  3. **Idempotency:** WP-04 has none; WP-07 has BLAKE3-256 → 64-hex `idem_key`. Without it, a double-fire (onStop + onError race) double-bills the tenant.
  4. **vcpuCount:** WP-04 line 505 hard-codes `Number(this.env.VCPU_COUNT) || 4`; WP-07 line 140 hard-codes `vcpuSeconds = wallSeconds * 4` (no env override). WP-04's env override is correct in spirit (configurable per-environment), but WP-07's literal is what's deployed. Pick one and document.
- **Real API:** Per WP-06 §3.2.1 ("The `recordUsage` body is owned by **WP-07 §3**"): WP-04's `computeUsageEvent` and `recordUsageForDevEnv` must be REMOVED. WP-04 should expose NOTHING related to billing. The `UsageEvent` type and `recordUsageForDevEnv` import on line 54 should also be removed.
- **Fix:** Delete WP-04 lines 493-516 (the `computeUsageEvent` and `recordUsageForDevEnv` helpers). Delete the `UsageEvent` and `recordUsageForDevEnv` references in the import on line 54. Add a comment cross-referencing WP-07 §3.2 as the canonical billing path.

### B14. **`onError`'s `lastWorkspaceName` field — workspace name not captured before `errored` transition**
- **Location:** WP-04 line 379 (`next = { ...this.state, status: "errored" as const, lastError: err.message }`)
- **Problem:** WP-01's `errored` arm requires `lastWorkspaceName: string` (WP-01:129) so a subsequent `buildStatusResponse` (WP-01:626-634) can show the operator "this DevEnv last ran workspace X". But WP-04's `onError` reads `this.state.workspaceName` (line 372) AFTER the spread — which works for the `running` arm but is also reached on the `starting` arm (if the crash happens during hydrate). On the `starting` arm, `workspaceName` IS present (WP-01:105) so this is fine. **BUT** if the DO is restored from storage in the `errored` state and a subsequent crash happens, `this.state.status === "errored"` and the spread `{ ...this.state }` carries `lastWorkspaceName` not `workspaceName` — the `next.lastWorkspaceName = ?? ??` fallback is missing. WP-04's TypeScript will likely compile (the spread is permissive), but the operator dashboard will show `lastWorkspaceName = <previous value>` instead of the current one, masking data.
- **Fix:** Capture `workspaceName = this.state.status === "errored" ? this.state.lastWorkspaceName : this.state.workspaceName` into a local BEFORE building `next`; then `next = { ...next, lastWorkspaceName: workspaceName }`. Same pattern as WP-07 §3.2 (B2 fix).

### B15. **`onStart` calls `transitionState` with `status: "running"` from `port_wait` — but the union has no `port_wait`**
- **Location:** `onStart` line 286-292
- **Code:**
  ```typescript
  this.transitionState({
      ...this.state,
      status: "running",
      lastActivityAt: Date.now(),
      lastProfileSnapshot: profileMeta,
      lastWorkspaceSnapshot: workspaceMeta,
  });
  ```
- **Problem:** Per WP-06 §1 ("`port_wait` is NOT a top-level state — it is a sub-phase of `starting`") and WP-06 iter 1 B5: the `running` arm of the union (WP-01:108-117) requires `containerHandle: string`, `lastHealthCheckAt: number`, `healthCheckFailures: number` — none of which are set in the spread. The spread from `this.state` would carry whatever the prior state had, but `this.state` is the `starting` arm (no `containerHandle`), so the spread produces a `running` object missing required fields. **TypeScript compile error.**
- **Cascade:** Also, `lastActivityAt` is not in the union (only WP-04 §3.3 line 524 calls `persistState` for it). `lastProfileSnapshot` / `lastWorkspaceSnapshot` (lines 290-291) belong on side-table keys, not in state. The build shape is fundamentally wrong.
- **Real API:** `onStart` must build the full `running` arm explicitly:
  ```typescript
  this.transitionState({
      status: "running",
      createdAt: this.state.createdAt,
      startedAt: this.state.startedAt ?? Date.now(),
      workspaceName: this.state.workspaceName,
      profileName: this.state.profileName,
      containerHandle: this.ctx.id.toString(), // per WP-06 §3.1 line 284
      lastHealthCheckAt: Date.now(),
      healthCheckFailures: 0,
  }, traceId);
  ```
  And `lastActivityAt` is a side-table key (per WP-01 §3.3 line 202 `ACTIVITY_KEY = "lastActivityAt"`).
- **Fix:** Replace the `onStart` `transitionState` call with the canonical `running` arm. After the transition, call `await this.ctx.storage.put("lastProfileSnapshot", profileMeta)` and `await this.ctx.storage.put("lastWorkspaceSnapshot", workspaceMeta)` (side-tables) if they are non-null.

### B16. **`onStart` does NOT call `clwTenant` from anywhere; the state-union migration has no consumer**
- **Location:** `onStart` line 267-292; the union's `running` arm does not include `clwTenant`.
- **Problem:** WP-07 §3.2 (line 135: `const tenantId = this.envVars.CLW_TENANT;`) reads tenant from `envVars`, but the DO persists tenant to state so that a DO restart can still bill correctly. The current `onStart` does not capture tenant. After fix B11 (add `clwTenant` to union), `onStart` must include `clwTenant: this.envVars.CLW_TENANT` in BOTH the `starting` (if start() injects it) and `running` arms.
- **Fix:** Add `clwTenant: string` to the `starting` / `running` / `stopping` arms of `DevenvState` (cross-WP change to WP-01). WP-04's `onStart` must read `this.envVars.CLW_TENANT` and pass it into the `starting` → `running` transition.
- **Cross-WP dependency:** Requires WP-01 iter 5+ to accept the `clwTenant` field; alternatively, store it in a side-table `TENANT_KEY = "clwTenant"` and read it from `ctx.storage.get` everywhere. The side-table approach avoids touching WP-01.

---

## 🟠 NEW HIGH SEVERITY ISSUES (Iter 2)

### H12. **`this.envVars.CLW_TENANT` vs `this.env.VCPU_COUNT` — inconsistent env access**
- **Location:** Line 510 (`(this.state as { clwTenant: string }).clwTenant` after the B11 fix; `this.env.VCPU_COUNT` on line 505)
- **Problem:** The DO's tenant-scoped env vars live in `this.envVars` (per WP-01 §3.3 line 336-342: mutable envVars are set via `super.start({envVars: newEnvVars, ...})`). The wrangler-level secrets/vars live in `this.env` (per WP-01 §3.3 line 256: `env: Env`). WP-04 reads `this.env.VCPU_COUNT` (correct — wrangler var) but also `this.state.clwTenant` (after B11 fix — DO state). This is consistent IF the `clwTenant` is also persisted to state in `start()`. WP-01's `start()` at line 338 sets `CLW_TENANT` in `envVars` but does NOT add it to the persisted `DevenvState`. The current WP-04 §3.2 line 510 (after B11 fix) would need a state field that WP-01 does not set.
- **Fix:** Either (a) add `clwTenant: string` to the state and have WP-01's `start()` set it in the `starting` transition, or (b) read it from `this.envVars.CLW_TENANT` directly (per WP-06 line 282: `this.currentEnvVars.WORKSPACE_NAME`). Option (b) is safer because it doesn't require touching WP-01.

### H13. **`onStart` does NOT call `clwTenant` validation; entrypoint.sh validates but the DO doesn't**
- **Location:** `onStart` line 259-295
- **Problem:** Per WP-03 §3.2 `validate_env()` line 99-163, the entrypoint validates `CLW_TENANT` against the canonical `^[a-zA-Z0-9._-]{1,128}$` regex and rejects `.`/`..`/leading-or-trailing `.`. The DO has `ClwTokenSchema` and `WorkspaceNameSchema` Zod validators (WP-01 §3.2 line 86-93) but no `ClwTenantSchema`. If a malformed tenant slips past entrypoint (env-var injection, container restart with stale state, a future refactor that removes entrypoint validation), the DO would happily run with a `clwTenant` that the billing endpoint rejects with `bad_tenant_id` (per WP-07 §3.2 I4).
- **Fix:** Add a `ClwTenantSchema` Zod validator to `src/types/devenv.ts` (mirroring WP-03's regex) and call it in `start()` (WP-01) AND in `onStart` (WP-04) as a defense-in-depth check.

### H14. **`Promise.allSettled` in `onError` is correct, but the cast-through-`as` breaks the type guarantee**
- **Location:** `onError` lines 391-398
- **Problem:** `(next as { lastProfileSnapshot: SnapshotMetadata | null }).lastProfileSnapshot = settled[0].value;` — even after moving to side-tables, the `settled[0].value` is `SnapshotMetadata | undefined` (when rejected); the code only writes when fulfilled, so the type is correct, BUT the cast pattern is the same anti-pattern flagged in WP-01 iter 2 B12 ("uses `as` to bypass type checking"). WP-04 perpetuates it.
- **Fix:** Use a type guard: `if (settled[0].status === "fulfilled" && settled[0].value) { await this.ctx.storage.put("lastProfileSnapshot", settled[0].value); }` — no `as`, no spread-mutation.

### H15. **`hydrateViaClw` returns `null` on first run, but `onStart` (line 272, 276) doesn't differentiate `null` from `SnapshotMetadata` for the state field**
- **Location:** `onStart` lines 272-291
- **Problem:** `hydrateViaClw` returns `null` when no ref exists (line 154-156). WP-04 then tries to store `null` into `lastProfileSnapshot` / `lastWorkspaceSnapshot` state fields. After B12 fix (side-tables), `ctx.storage.put("lastProfileSnapshot", null)` would store a literal `null` (not a missing key) — subsequent `get` returns `null`, indistinguishable from "snapshot failed and we recorded null". Operators looking at logs would see "lastProfileSnapshot = null" and not know if the hydrate succeeded.
- **Fix:** Either (a) don't call `ctx.storage.put` when `hydrateViaClw` returns `null` (the key is absent, signaling "no snapshot yet"), or (b) write a `HydrateMetadata` with a `firstRun: true` flag. Option (a) is simpler.

### H16. **`onError`'s `acquireSnapshotLock` runs inside a try, but the catch swallows the lock-release error too**
- **Location:** `onError` lines 382-408
- **Problem:** The structure is `try { acquireSnapshotLock; ...; } catch (snapshotErr) { log; } finally { releaseSnapshotLock; }`. If `acquireSnapshotLock` itself throws (e.g. `SNAPSHOT_IN_PROGRESS`), the `finally` runs and tries to `releaseSnapshotLock` — but the lock was never acquired, so `releaseSnapshotLock` sets `snapshotInProgress: false` (which is a no-op or a wrong-state write if the field doesn't exist). More importantly, the OUTER `transitionState(next)` on line 412 runs UNCONDITIONALLY after the `try/catch/finally` — even when the snapshot lock throw means we never attempted any snapshot. The state is set to `errored` for a lock collision, masking the original crash reason.
- **Fix:** In `onError`, when `acquireSnapshotLock` throws `SNAPSHOT_IN_PROGRESS`, log a warning and SKIP the emergency snapshot (the lock holder will finish or timeout), then proceed to `transitionState({status: "errored", ...})` with `lastError: "concurrent snapshot in progress"`. Do not call `releaseSnapshotLock` in this case (the OTHER holder is responsible).

### H17. **`waitForPort` is referenced but not imported (line 281)**
- **Location:** `onStart` lines 281-283
- **Code:** `await waitForPort(6080, { timeoutMs: 60_000 }); await waitForPort(7681, { timeoutMs: 60_000 }); await waitForPort(8080, { timeoutMs: 60_000 });`
- **Problem:** Per WP-06 §3.1 line 110-130 (and iter 1 B7), the port-wait is done by the SDK's `this.startAndWaitForPorts(this.requiredPorts, { portReadyTimeoutMS: 60_000, waitInterval: 1_000 })` — a single kernel-level call. WP-04 invents `waitForPort(port, opts)` as an unimported helper. WP-06 owns the canonical port-wait; WP-04 should call `this.startAndWaitForPorts` or `await this.waitForPort(8080, ...)` where `waitForPort` is a method on the inherited `Container` class (not an unimported free function).
- **Fix:** Replace the three `waitForPort` calls with a single `await this.startAndWaitForPorts([6080, 7681, 8080], { portReadyTimeoutMS: 60_000, waitInterval: 1_000 })` call — but ONLY if it makes sense in WP-04's context. Per WP-06 §3.1, the port-wait is inside WP-06's `onStart` (which is the ACTUAL `onStart` that runs in the SDK's lifecycle). WP-04's `onStart` is the SKELETON for WP-06 to import. **If WP-04's `onStart` is the literal method body that WP-06's `onStart` will call, then this is a different concern; if WP-04 is supposed to BE the lifecycle hook, it should use the SDK's method.**
- **Cross-WP ambiguity:** WP-06 §3.1 owns `onStart` (it's a stub that calls `hydrateViaClw` etc. as imports from WP-04). WP-04 §3.2 owns the `hydrateViaClw` body. The `waitForPort` call on WP-04 line 281 is therefore inside the `hydrateViaClw` (or post-hydrate) flow — but `waitForPort` is NOT a WP-04 concern; it's WP-06's. WP-04's `onStart` is at best an EXAMPLE; the actual lifecycle is WP-06.
- **Fix (better):** Remove the `onStart` / `onStop` / `onError` definitions from WP-04 entirely. WP-04 should be a LIBRARY (exporting `hydrateViaClw`, `snapshotViaClw`, `acquireSnapshotLock`, `releaseSnapshotLock`) imported by WP-06's lifecycle hooks. The current WP-04 conflates LIBRARY + LIFECYCLE — a separation-of-concerns violation that crosses WP-04 with WP-06.

---

## 🟡 NEW MEDIUM SEVERITY ISSUES (Iter 2)

### M7. **`clwRefExists` does NOT pass `--ref-domain runner` — first-run pre-check hits the wrong keyspace**
- **Location:** `clwRefExists` line 141-147
- **Code:** `await this.execClw(["ls", "--name", name], {captureOutput: false, timeoutMs: 30_000})`
- **Problem:** `hydrateViaClw` and `snapshotViaClw` pass `--ref-domain runner` (lines 160, 197), but `clwRefExists` does NOT. The ref keyspace is `BLAKE3(ref_domain || name)` (per WP-03 §3.2 comment line 173-175 and `clw-cli/src/subcmds/ls.rs:55` `ref_key_in(cfg.ref_domain, name)`). Without `--ref-domain runner`, `clw ls --name <name>` looks up the ref in the DEFAULT ref domain (whatever clw's default is — likely `user` for the user-facing CLI). The first-run detection would always succeed (ref absent in the wrong keyspace) and proceed to call `clw hydrate ... --ref-domain runner`, which would then either find a ref (if the user happens to have one in the runner domain) or fail with a misleading "no ref" error.
- **Fix:** Add `--ref-domain runner` to the `clwRefExists` arg list: `["ls", "--name", name, "--ref-domain", "runner"]`. Also, more robustly, read `--ref-domain` from a single source constant (per the iter 1 M2 "lock the arg vector in a test" guidance).

### M8. **`force: false` in the `snapshot()` RPC still adds `--force` (B11 from iter 1, partial regression)**
- **Location:** `snapshotViaClw` line 201 (`if (options.force) args.push("--force")`)
- **Problem:** Iter 1 H11 flagged that `payload.force` was never threaded. The current code DOES thread it (good), BUT `onStop` (line 322-323) and `onError` (line 387-388) ALWAYS pass `{ force: true }`. The manual `snapshot()` RPC at line 434-447 also passes `{ force: payload.force }`, which can be `false`. When the operator triggers a manual snapshot on a healthy running DevEnv with `force: false`, clw's `clw snapshot <DIR> --name <NAME>` (no `--force`) on an existing ref with DIFFERENT content will return HTTP 409 (per `clw-cli/src/subcmds/snapshot.rs:178-183` — "workspace already exists with different content and the server rejected the in-place update (HTTP 409) — the ref store is create-only"). The `snapshotViaClw` then throws (line 222), the manual RPC returns a 500-style error, and the operator has to retry with `force: true`. The RPC should document this: `force: false` is a NO-OP on divergent content; `force: true` is destructive. The shape is fine; the documentation gap is the issue.
- **Fix:** Add a JSDoc on the `snapshot` RPC: `Manual snapshot. force: false is a no-op when the existing ref points at different content (clw returns HTTP 409 → this method throws). force: true re-snapshots destructively. Default for /api/snapshot is force: true (matches WP-01's `fetch` default on line 518).`

### M9. **JSON parse fallback silently sets `root: null` — should at least log a `warn` for observability**
- **Location:** `hydrateViaClw` lines 178-183, `snapshotViaClw` lines 215-220
- **Problem:** When `JSON.parse` throws, both methods return `{root: null, ...zeros...}`. The `null` root propagates to `lastProfileSnapshot.root`, which downstream observability reads as "parse failed". But the failure reason (stderr, parse error message) is discarded. If clw's JSON shape ever changes (e.g. wrapping in `{data: {root, ...}}`), WP-04 silently swallows the change and reports zeros.
- **Fix:** In the `catch` block, log a structured warning: `this.log("warn", "clw_json_parse_failed", { subcmd, err: String(e), rawStdoutPreview: result.stdout.slice(0, 256) })`. Then return the zero-metadata fallback.

---

## 📋 CROSS-WP COORDINATION FINDINGS (Iter 2)

| # | Cross-WP surface | Owner of canonical | WP-04 deviation | Resolution |
|---|---|---|---|---|
| 1 | In-container exec transport | WP-02/03 (port 8080 exec-server) | WP-04 uses non-existent `this.ctx.container.exec()` | **REPLACE** with `this.containerFetch("http://localhost:8080/clw", ...)` per WP-06 §3.1.1 |
| 2 | `DevenvState` shape | WP-01 (5-arm union) | WP-04 adds 4 fields (`clwTenant`, `snapshotInProgress`, `lastProfileSnapshot`, `lastWorkspaceSnapshot`) not in the union | **REMOVE** from state; use side-table keys (WP-06 §3.3 pattern: `STATE_KEY`, `PROFILE_SNAPSHOT_KEY`, `WORKSPACE_SNAPSHOT_KEY`, `TENANT_KEY`, `SNAPSHOT_LOCK_KEY`); add `clwTenant` to the union's `starting`/`running`/`stopping` arms (or use a side-table) |
| 3 | `transitionState` + state-machine transitions | WP-01 (`transitionState` on line 291) | WP-04 spreads `...this.state` into `transitionState` calls, producing objects missing required fields | **BUILD EXPLICIT FULL STATES** for each transition (per WP-06 §3.1 `onStart` line 278-287) |
| 4 | `waitForPort` / `startAndWaitForPorts` | WP-06 (SDK call) | WP-04 references free-function `waitForPort` not in scope | **REPLACE** with `this.startAndWaitForPorts([6080, 7681, 8080], {portReadyTimeoutMS: 60_000, waitInterval: 1_000})` (SDK method) OR remove the call from WP-04 (it's a WP-06 concern) |
| 5 | `recordUsage` body | WP-07 (canonical ASK-2 endpoint) | WP-04 defines `computeUsageEvent` and `recordUsageForDevEnv` — duplicates WP-07 §3.2 and disagrees on URL/auth/schema/idempotency | **DELETE** WP-04's billing helpers entirely; document the WP-07 §3.2 contract as the single source |
| 6 | `UsageEvent` type import | WP-07 (canonical wire shape) | WP-04 imports `UsageEvent` from `../types/devenv.js` and uses non-canonical fields | **DELETE** the import; WP-04 does not deal with billing types |
| 7 | `containerHandle` field | WP-06 (sets in `onStart` line 284: `containerHandle: this.ctx.id.toString()`) | WP-04 does not set `containerHandle` anywhere | **ADD** to the `running` arm in WP-04's `onStart` (B15 fix) |
| 8 | `snapshotInProgress` lock storage | WP-06 / WP-04 (cross-owned) | WP-04 stores in `this.state.snapshotInProgress` (not in the union) | **MOVE** to side-table key `"snapshotInProgress": boolean` (DO storage, not state field) |
| 9 | `lastActivityAt` field | WP-01 (`ACTIVITY_KEY` line 202) | WP-04 line 289 uses `lastActivityAt` in state; WP-01 has it as a separate `ctx.storage.put` key | **REMOVE** from the `transitionState` spread; call `this.noteActivity()` (WP-01 line 494) instead |
| 10 | `ClwTenantSchema` Zod validator | WP-01 / WP-03 (entrypoint validates) | WP-04 does not validate; relies on entrypoint | **ADD** `ClwTenantSchema` to `src/types/devenv.ts`; validate in `start()` (WP-01) AND in `onStart` (WP-04 defense-in-depth) |
| 11 | Exec-server contract (port 8080 endpoints) | WP-02/03 (server) + WP-06 (consumer documentation) | WP-04 ignores the contract | **ALIGN** with WP-06 §3.1.1: `POST /clw {argv: string[]}` → `{exit_code, stdout, stderr}` |
| 12 | `force` flag semantics on `snapshot` RPC | WP-04 (RPC owner) | WP-04 documents but doesn't surface in `SnapshotRequest` | **DOCUMENT** in RPC JSDoc (M8 fix) |
| 13 | `clwRefExists` keyspace (`--ref-domain runner`) | WP-04 (helper) | Helper omits the flag, looks up wrong keyspace | **ADD** the flag (M7 fix) |
| 14 | `onError` `lastWorkspaceName` field | WP-01 (`errored` arm) | WP-04 spreads but doesn't capture the local before transition | **CAPTURE** locals before transition (B14 fix) |
| 15 | Library vs lifecycle separation | WP-04 (library) vs WP-06 (lifecycle) | WP-04 defines BOTH library functions AND `onStart`/`onStop`/`onError` lifecycle hooks | **STRIP** lifecycle hooks from WP-04; export only the 4 library functions; WP-06 owns the hooks (H17 fix) |

---

## 📋 DoD Re-Verification

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | onStart calls clw hydrate for /data/chrome and /data/workspace (with first-run skip) | ❌ **FAIL** | B11 (compile error on `snapshotInProgress` field); B15 (compile error on `running` arm missing fields); B10 (`this.ctx.container.exec()` does not exist) |
| 2 | onStart first-run detection uses `clw ls --name X` pre-check, not exit 2 | ⚠️ **PARTIAL** | Pre-check exists (line 153) but `clwRefExists` does NOT pass `--ref-domain runner` (M7), so the pre-check hits the wrong keyspace |
| 3 | onStop calls clw snapshot --force for both dirs | ❌ **FAIL** | B10 (exec API does not exist); B11 (compile error on lock field) |
| 4 | onStop persists snapshot metadata to DO state via transitionState | ❌ **FAIL** | B12 (compile error: errored arm has no lastProfileSnapshot); B15 (running arm missing fields) |
| 5 | onError attempts emergency snapshot before dying (with allSettled) | ❌ **FAIL** | B10 (exec API); B12 (compile error on state cast); H16 (lock collision masks original error) |
| 6 | Snapshot lock prevents overlapping snapshots; recoverable on DO restart | ❌ **FAIL** | B11 (lock field not in union); H16 (lock collision handling wrong) |
| 7 | Usage event handed off to WP-07 via recordUsageForDevEnv | ❌ **FAIL** | B13 (WP-04 must NOT define recordUsage; WP-07 owns the body) |
| 8 | JSON output parsing works for clw --json (hydrate AND snapshot) | ✅ **PASS** | Correct hydrate JSON shape (root/files/bytes_total/bytes_from_cache/bytes_downloaded); correct snapshot shape (root/files/bytes_total/chunks_total/chunks_uploaded/unchanged/skipped_external_symlinks) |
| 9 | Snapshot timeout (10 min) prevents hanging; null stdout/stderr handled | ⚠️ **PARTIAL** | Timeout is 600_000ms (good); but B10 means the `output()` call won't even compile against the real SDK |
| 10 | State persists across DO restarts; snapshotInProgress recovery | ❌ **FAIL** | B11 (snapshotInProgress not in state); the recovery path on line 534 also broken |

**DoD Score: 1/10 PASS, 1/10 PARTIAL, 8/10 FAIL** (down from iter 1's 1/10 PASS, 5/10 PARTIAL, 4/10 FAIL — **REGRESSION** because B10 is a new BLOCKING issue that didn't exist in iter 1's analysis).

---

## 📋 Invariants Re-Verification

| Invariant | Enforced? | Verdict |
|-----------|-----------|---------|
| I1: snapshotInProgress = true only during active snapshot; recovered on init | ❌ | Field doesn't exist in union (B11) |
| I2: lastProfileSnapshot/lastWorkspaceSnapshot only on success | ⚠️ | Side-table move would fix this (B12) |
| I3: onStop and onError both call recordUsage exactly once | ❌ | WP-04 shouldn't define recordUsage (B13) |
| I4: acquireSnapshotLock/releaseSnapshotLock always paired (try/finally) | ✅ | Both use try/finally |
| I5: clwRefExists(name)===false ⇒ hydrateViaClw returns null without invoking clw hydrate | ⚠️ | Helper exists but uses wrong keyspace (M7) |
| I6: clw snapshot/hydrate exit code != 0 always throws | ✅ | Both throw on non-zero |
| I7: SnapshotMetadata includes root/bytesTotal/chunksTotal/chunksUploaded/timestamp | ✅ | Type matches snapshot.rs JSON |
| I8: HydrateMetadata includes root/bytesTotal/bytesFromCache/bytesDownloaded/timestamp | ✅ | Type matches hydrate.rs JSON |
| I9: State transitions go through transitionState | ⚠️ | WP-04 uses transitionState but builds states with wrong fields (B11, B12, B15) |
| I10: Billing event handoff via recordUsageForDevEnv, never fetch directly | ❌ | WP-04 defines recordUsage — VIOLATES this invariant (B13) |

**Invariants Enforced: 4/10 (40%)** — REGRESSION from iter 1's 3/7 (43%, with different scale).

---

## 📊 UPDATED SCORECARD

| Category | Iter 1 | Iter 2 | Trend |
|----------|--------|--------|-------|
| Blocking Issues | 9 | 7 (1 fixed + 0 regressed + 6 net new) + 9 UNFIXED | ❌ NO progress on iter 1 fixes; 7 net new |
| High Issues | 11 | 6 NEW | ❌ net new |
| Medium Issues | 6 | 3 NEW | ❌ net new |
| DoD Pass Rate | 10% | 10% | → (no progress) |
| Invariants Enforced | 43% | 40% | ↓ regression |

**OVERALL VERDICT: ❌ FAIL — Current WP-04 is the same as iter 1 (B3 + B6 fixed only) plus 7 net new BLOCKING issues discovered by reading the cross-WP contracts more carefully.**

---

## 🔧 FIX PRIORITY FOR ITER 2

### Must Fix (Blockers — all 7 + all 9 unfixed iter 1)
1. **B10** — Replace `this.ctx.container.exec()` with `this.containerFetch("http://localhost:8080/clw", ...)` per WP-06 §3.1.1 exec-server contract
2. **B11** — Move `lastProfileSnapshot`, `lastWorkspaceSnapshot`, `snapshotInProgress` to side-table keys; add `clwTenant` to a side-table or the union
3. **B12** — `onError` must NOT mutate the `errored` arm with `lastProfileSnapshot` / `lastWorkspaceSnapshot`; use side-table writes
4. **B13** — DELETE WP-04's `computeUsageEvent` and `recordUsageForDevEnv` entirely; WP-07 §3.2 owns billing
5. **B14** — Capture `workspaceName` into a local before `onError`'s `transitionState` to errored
6. **B15** — `onStart` `transitionState` to `running` must build the full `running` arm with `containerHandle`, `lastHealthCheckAt`, `healthCheckFailures`
7. **B16** — `onStart` must read and persist `clwTenant`
8. **B1, B2, B4, B5, B7, B8, B9** — original iter 1 BLOCKING (B4 partially addressed by B10; B5 hard-coded flag; B7 transitionState patterns; B8 type imports; B9 lock storage)

### Should Fix (High)
1. **H12** — Consistent env access (`this.envVars` vs `this.env`)
2. **H13** — Add `ClwTenantSchema` Zod validator
3. **H14** — Remove `as { ... }` casts in onError
4. **H15** — `onStart` differentiates null vs metadata for side-table writes
5. **H16** — `onError` handles `SNAPSHOT_IN_PROGRESS` lock collision (skip snapshot, still transition to errored)
6. **H17** — Strip lifecycle hooks from WP-04; export only library functions

### Nice to Fix (Medium)
1. **M7** — `clwRefExists` must pass `--ref-domain runner`
2. **M8** — Document `force` flag semantics on `snapshot` RPC
3. **M9** — Log a warning on JSON parse failure (with stdout preview)

---

## NEXT STEPS

1. **Apply all 7 new BLOCKING fixes** (B10-B16) plus all 9 unfixed iter 1 BLOCKING fixes (B1, B2, B4, B5, B7, B8, B9)
2. **Apply all 6 new HIGH fixes** (H12-H17)
3. **Apply all 3 new MEDIUM fixes** (M7-M9)
4. **Coordinate cross-WP:**
   - **WP-01:** Add `clwTenant` to the union OR document the `TENANT_KEY` side-table convention
   - **WP-02/03:** Implement the exec-server on port 8080 (per WP-06 §3.1.1 contract); WP-04 will be the heaviest consumer
   - **WP-06:** Strip its `hydrateViaClw` / `snapshotViaClw` / `acquireSnapshotLock` / `releaseSnapshotLock` STUBS (lines 561-579) — they should be IMPORTS from WP-04, not local stubs
   - **WP-07:** Already owns the billing; no change needed
   - **WP-08:** Must add the `BILLING_INGEST_URL` and `BILLING_INGEST_AUTH_KEY` wrangler vars (per WP-07 §3.5)
5. **Re-verify all 10 DoD items pass** (post-fix)
6. **Re-verify all 10 invariants enforced** (post-fix)
7. **Proceed to Iteration 3 review** with a tightened cross-WP contract check (especially WP-01 union shape, WP-06 library import vs re-declaration)

**Do NOT declare PASS until DoD ≥ 9/10 AND all invariants enforced AND cross-WP contracts aligned.**

---

## CONVERGENCE TRAJECTORY

| Iteration | Blocking | High | Medium | DoD Pass | Verdict |
|-----------|----------|------|--------|----------|---------|
| Iter 1 | 9 | 11 | 6 | 10% | FAIL |
| Iter 2 | **7 net new + 9 unfixed = 16** | **6** | **3** | **10%** | **FAIL** |
| Iter 3 (projected) | TBD | TBD | TBD | TBD | TBD |

**Convergence:** No progress on iter 1 fixes; iter 2 found more issues by reading the cross-WP contracts more carefully. **NOT CONVERGING** — the WP-04 needs a structural rewrite (library-only; no lifecycle hooks) before iter 3 can pass.

---

**END OF WP-04 ITERATION 2 REVIEW**
