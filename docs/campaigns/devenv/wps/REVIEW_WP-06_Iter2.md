# WP-06 REVIEW — Iteration 2 (DEEPER AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Previous Verdict:** ⚠️ PASS-WITH-CROSS-WP-BLOCKERS  
**New Verdict:** ❌ **FAIL — 5 NEW BLOCKING + 4 NEW HIGH + 4 NEW MEDIUM (13 total) — plus 4 cross-WP regressions**

---

## ✅ ITERATION 1 FIX RE-VERIFICATION

Checking that the fixes from Iteration 1 actually work (not just shipped):

| Iter-1 Fix | Verdict | Evidence in iter-2 doc |
|------------|---------|------------------------|
| B1: `containerFetch` to port 8080 (no `ctx.container.exec`) | ⚠️ **REGRESSED** | Wrong signature — see B17 below. `containerFetch` is not a 3-arg `(url, options, port)`; the call doesn't reach the exec-server. |
| B2: `onStop(params: StopParams)` | ✅ Fixed | Line 318: `override async onStop(params: StopParams)`; `params.reason` logged at line 320. |
| B3: `onError(error: unknown)` + defensive coercion | ✅ Fixed | Line 386: `override async onError(error: unknown)`; `error instanceof Error ? error.message : String(error)` at 389. |
| B4: `this.schedule()` (durable) not `setAlarm` | ✅ Fixed | Line 297 `await this.schedule(Date.now() + HEALTH_CHECK_INTERVAL_MS, HEALTH_TICK_SCHEDULE)`; line 552 reschedule. |
| B5: `port_wait`/`provisioning` folded out of union | ✅ Fixed | Line 184-190 `VALID_TRANSITIONS` 5-state; line 182-183 comment explains the fold. |
| B6: `transitionState` takes full target `DevenvState` | ✅ Fixed | Line 278-287 `transitionState({status: "running", createdAt, startedAt, ...})`. |
| B7: No bash, use `startAndWaitForPorts` + exec-server `/port-check/:p` | ⚠️ **PARTIAL** | `startAndWaitForPorts` used at line 271. Per-port check at line 464 uses `containerFetch` (broken — B17). |
| B8: `this.currentEnvVars` instance field | ❌ **REGRESSED** | Field declared at line 170-172 but `this.envVars` itself does not exist on the class — see B15. |
| B9/B10: `hydrateViaClw`/`snapshotViaClw`/lock stubs reference WP-04 | ✅ Fixed | Lines 561-579 — stubs that `throw new Error("NOT_IMPLEMENTED_HERE: see WP-04 §3.2 ...")`. |
| B11: `transitionState` idempotent on same-state | ⚠️ **PARTIAL** | Line 204-209 same-state no-op + log; but `extractMutableFields` referenced and never defined — see H15. |
| B12: `deleteSchedules(HEALTH_TICK_SCHEDULE)` on terminal | ✅ Fixed | Line 366, 423, 505. |
| B13: `clw exec mkdir` removed; `/mkdir` endpoint used | ✅ Fixed | Line 136-150 `containerMkdir` calls exec-server `/mkdir`. |
| B14: `recordUsage` stub to canonical WP-07 path | ✅ Fixed | Line 587-589 stub; §3.2.1 ownership note. |
| H1: `port_wait` removed from union | ✅ Fixed | Folded out per B5. |
| H2: 4th health probe doesn't throw | ✅ Fixed | Same-state idempotent in `transitionState`. |
| H3: Liveness via SDK `getState()` + `/ping` (no clw/bash) | ⚠️ **PARTIAL** | Line 452-456 — uses `getState()` then falls back to `/ping`. `/ping` call uses `containerFetch` with wrong signature (B17) — the actual call doesn't reach the exec-server. |
| H5: `/_health` lives in WP-01's `fetch()` (auth) | ✅ Fixed | §3.4 puts the branch in WP-01's `fetch()` with bearer-token gate. |
| H6: `wsConnections` ownership documented | ✅ Fixed | §3.6 + §9.2 contract table. |
| H7: Wall-clock budget + `AbortSignal.timeout` on every fetch | ✅ Fixed | Line 253 `withTimeout(ON_START_BUDGET_MS, ...)`; every `containerFetch` has `signal: AbortSignal.timeout(...)`. |
| H8: container-not-alive vs port-down distinction | ✅ Fixed | Line 510-512 `container_not_alive` log event. |
| H9: `healthCheckFailures` persisted BEFORE awaits | ⚠️ **PARTIAL** | Line 488-494 persists after the await. H9 said BEFORE; the current code persists after the `Promise.all` of port checks, which is fine (no eviction between port-check and persist because the persist follows immediately) but the comment "persist BEFORE returning" (line 487) is vague. |
| H10: `onError` calls `recordUsage` with `minimumBillableSeconds:60` | ✅ Fixed | Line 420. |
| H11: `onStop` no-op on `stopped`/`errored` | ✅ Fixed | Line 325-328 early-return. |
| M1: `DevenvState` imported from `../types/devenv` | ✅ Fixed | Line 75-78. |
| M2: `SnapshotMetadata` imported, not re-declared | ✅ Fixed | Line 78 import. |
| M3: `HealthCheckResponse` imported, not re-declared | ✅ Fixed | Line 78 import. |
| M4: Unit test skeleton | ✅ Fixed | §9.1 with state machine, healthCheck, onStop no-op, onError never-throws. |
| M5: `acquireSnapshotLock` in `onStop` (per WP-04) | ✅ Fixed | Line 346. |
| M6: `onStart` guards against re-entry | ✅ Fixed | Line 246-248 throw on non-starting. |
| M7: Single `log(level, event, fields)` | ✅ Fixed | Line 596-604. |
| M8: `fetch()` not re-declared; lives in WP-01 | ✅ Fixed | §3.4 puts the branch in WP-01. |
| M9: snapshot metadata side-table | ✅ Fixed | Line 632-633, 657-662. |

**Regression summary:** 4 iter-1 fixes partially regressed (B1, B7, B8, H3, H9) — all on the *exec-server call signature* / `this.envVars` field. The state machine + lifecycle + alarm handling are genuinely correct, but the *transport* (how WP-06 reaches the in-container exec-server) is still wrong.

---

## 🔴 NEW BLOCKING ISSUES (Missed in Iteration 1)

### B15. **`this.envVars` Instance Field Does Not Exist on the Class — `this.currentEnvVars` Reads `undefined`**

- **Location:** Line 170-172: `private get currentEnvVars(): Record<string, string> { return this.envVars; }`. Called at lines 261, 265, 349, 352, 373, 394, 403, 404.
- **Problem:** Iter-1 B8 said "Persist the merged envVars on the instance: `private currentEnvVars: Record<string, string> = {};` populated in `start()` and read in lifecycle hooks." The iter-2 spec implements a *getter* that returns `this.envVars`, but `this.envVars` is never assigned. WP-01's `start()` (the canonical owner) builds `newEnvVars` and passes it to `super.start({envVars: newEnvVars})` (WP-01 line 354-357) — the SDK stores it on the Container instance, NOT on the DO instance. The DO class (WP-01 line 252) declares only `private state!: DevenvState;` — no `envVars` field.
- **Failure mode:** First call to `this.currentEnvVars.PROFILE_NAME` throws `TypeError: Cannot read properties of undefined (reading 'PROFILE_NAME')`. `onStart` aborts at the first `hydrateViaClw` call. Container is `errored` from a fresh boot.
- **Real fix (per B8 spec):** Either
  - (a) WP-01's `start()` must ALSO write `this.envVars = newEnvVars` BEFORE `super.start({...})`, and WP-06 reads `this.envVars` directly (the getter becomes redundant), OR
  - (b) WP-06 persists the values to `this.ctx.storage` keyed `envVars` on first read, and reads from there in lifecycle hooks, OR
  - (c) WP-06 reads from `this.state` for the names it needs (WP-01's union already has `workspaceName` + `profileName` in `starting|running|stopping`) and reads `CLW_TENANT` from a separate state field that WP-01 must add (cross-WP).
- **Recommendation:** Option (a) — the smallest change; WP-01 already builds the merged envVars; one extra line `this.envVars = newEnvVars` is enough. The getter at line 170-172 can be deleted in favor of direct `this.envVars` reads in WP-06.
- **Why this slipped through iter-1:** Iter-1 only verified that the call sites use `this.currentEnvVars` (the right surface) — it did NOT verify that the field is actually populated. The getter looks like it works in isolation.

### B16. **Cross-WP Drift — WP-07 Still References `port_wait` State That WP-06 Removed**

- **Location:** External to WP-06 but BLOCKING for the contract. WP-07 §3.3 line 253: `const isActive = status.status === "starting" || status.status === "running" || status.status === "port_wait"; // WP-06 added port_wait`. WP-07 §8 Self-Check 3 line 533: "Concurrency check uses WP-06 state union (starting/running/port_wait = active)".
- **Problem:** WP-06 iter-1 removed `port_wait` from the union (B5 fix). WP-07 still includes it. The `||` branch is **unreachable code** in TypeScript-strict mode because the `status.status === "port_wait"` comparison narrows to a never-narrowed variant of `DevenvStatus`. With `tsc --strict`, this either errors (`port_wait` is not assignable to `DevenvStatus`) or silently becomes a dead branch.
- **Downstream impact:** WP-07's `enforceDevenvQuota` quota guard cannot use `port_wait` (it's not in the union), so the active set is `starting | running` only. A DO that's transitioning through `starting` (the pre-port-wait phase that iter-2 defines as part of `starting` per B5) IS counted. But the `port_wait` literal in WP-07's source still causes a `tsc` error in the Worker build.
- **Fix:** WP-07 must remove `|| status.status === "port_wait"` (line 253) and the Self-Check 3 reference (line 533). This is a cross-WP edit; flag for WP-07 iter-2.
- **Why this is BLOCKING for WP-06:** The spec is internally inconsistent (says port_wait is folded out, but cross-WP contract still relies on it). The quota guard either mis-reports active state (if tsc passes) or fails to compile (if tsc --strict is on, which it is in this repo per CLAUDE.md `tsc --strict`).

### B17. **`containerFetch` Signature Is Wrong — `(url, options, port)` Does Not Match the SDK**

- **Location:** Lines 115, 137, 158, 464: `this.containerFetch("http://localhost:8080/clw", {method, headers, body, signal}, 8080)`. Line 158: `this.containerFetch("http://localhost:8080/ping", {signal: AbortSignal.timeout(2_000)}, 8080)`. Line 464: `this.containerFetch("http://localhost:8080/port-check/${port}", {signal: AbortSignal.timeout(2_000)}, 8080)`.
- **Problem:** The `@cloudflare/containers` SDK's `containerFetch` is `containerFetch(request: Request, port: number)` — a `Request` object (or string URL that gets coerced to a `Request`) and a port number. The 3-argument form `(url, options, port)` is **not a valid signature** in any version of the SDK. The middle argument is the **port number**; passing `{method, headers, body, signal}` as the port number coerces to `NaN` (port becomes `NaN`, the fetch errors out as an invalid port).
- **Real failure mode:**
  1. The `Request` body is never constructed from `{method, headers, body, signal}` — those are passed as the port, not the request options.
  2. Without a `Request` object, the body (`JSON.stringify({argv})`) is never sent.
  3. The exec-server receives either no request or a malformed one, returns 4xx, and the call throws `EXEC_RPC_FAILED`.
  4. `containerPing` (line 158) silently returns `false` because the catch at line 165 swallows.
  5. The per-port `port-check` at line 464 silently returns `{port, healthy: false}` for every tick.
  6. The health check fails 3 times → container transitions to `errored` within 90s of every healthy start.
- **Evidence the canonical pattern is different:** WP-05 line 149 uses the documented `this.ctx.container.getTcpPort(port).fetch(containerRequest)` (a `Fetcher` returned by the SDK that takes a `Request`). WP-06 should use the same pattern.
- **Fix (canonical):** Build a `Request` object and call `this.ctx.container.getTcpPort(port).fetch(request)`:
  ```typescript
  const req = new Request("http://localhost/clw", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ argv }),
    signal: AbortSignal.timeout(timeoutMs),
  });
  const resp = await this.ctx.container.getTcpPort(8080).fetch(req);
  ```
  Same pattern for `/mkdir`, `/ping`, `/port-check/:p`.
- **Why this is BLOCKING:** Every in-container call from WP-06 (exec, mkdir, ping, port-check) is broken. The container cannot hydrate (mkdir fails), cannot pass the health check (ping fails), cannot detect port readiness (port-check fails), and goes to `errored` from a fresh boot.

### B18. **Port 8080 Conflict — code-server AND Exec-Server Cannot Both Bind 8080**

- **Location:** §3.1.1 line 65-66: "The server runs on port `8080` alongside code-server." WP-03 line 528: `code-server --bind-addr 0.0.0.0:8080 --auth password --disable-telemetry --disable-workspace-trust /data/workspace`.
- **Problem:** code-server binds port 8080 first (supervisord starts it; the spec §3.1.1 line 65-66 says "alongside code-server" which is structurally impossible — only one process can bind a port). The exec-server then either (a) fails to bind (init error, container goes `errored` from boot), or (b) if it binds first, code-server fails to bind (no editor for the user).
- **The two viable options:**
  1. **Move exec-server to a different port (e.g., 9090).** Adds port 9090 to `requiredPorts` (or makes it optional). WP-02/WP-03 Dockerfile + supervisord must include the new port.
  2. **Put exec-server on the supervisord control socket (Unix socket).** More fragile; the WS path is the same.
- **Fix:** Pick option 1 (port 9090). Update §3.1.1 line 65-66, all `containerFetch` call sites, the per-port health check loop, and §9.2 cross-WP contract table. The `requiredPorts` literal in WP-01 (`[6080, 7681, 8080] as const`) is unchanged; 9090 is a separate *control* port, not a user-facing port. The `port-check` per-tick loop in `healthCheck` checks the user-facing ports (6080/7681/8080) — those are still healthy even if 9090 is down. The exec-server is a control plane, not user plane.
- **Why this is BLOCKING:** Even if B17's signature is fixed, the request still goes to port 8080 which is now code-server, not the exec-server. The exec-server is unreachable. The 3-failure threshold fires.

### B19. **State Machine Idempotent Path Spreads `newState` Over `this.state` But Calls an Undefined Helper `extractMutableFields`**

- **Location:** Line 205: `this.state = { ...this.state, ...extractMutableFields(newState) };`. The function `extractMutableFields` is referenced but never declared anywhere in WP-06.
- **Problem:** The same-state idempotent path spreads `newState` (the *full* target `DevenvState`, which includes `status`) over `this.state`. After the spread, `status` becomes whatever `newState.status` is — but the early-return at line 204 already confirmed `oldStatus === newStatus`, so this is fine. The issue is `extractMutableFields` is undefined — `ReferenceError` at runtime when an idempotent transition is attempted (e.g. the 4th health probe after the 3rd already errored).
- **Trace:** The 4th health probe runs `healthCheck()`. `newFailureCount >= 3` → `transitionState({status: "errored", ...})` → `oldStatus === "errored" === newStatus` → line 205 evaluates `extractMutableFields(...)` → `ReferenceError: extractMutableFields is not defined`. The exception propagates to the alarm handler, which the SDK treats as a failure. The container's `lastError` is overwritten with the helper-missing message.
- **Fix:** Either
  - (a) Inline the helper: `const { status: _ignored, ...mutable } = newState; this.state = { ...this.state, ...mutable } as DevenvState;` (discard `status` explicitly), or
  - (b) Define `extractMutableFields` at the top of the file as a static helper.
- **Recommendation:** Option (a) — explicit and inline; no chance of drift. The cast to `DevenvState` is safe because `status` is preserved from `oldStatus` (which equals `newStatus`).
- **Why this is BLOCKING:** The H2 fix (4th probe doesn't throw) is documented as ✅ in §10 but is actually broken — the new code path throws a different error (ReferenceError instead of `INVALID_STATE_TRANSITION`). The "idempotent transition" doesn't work as advertised.

---

## 🟠 NEW HIGH SEVERITY ISSUES

### H12. **Per-Port `port-check` Path Uses the Same Broken `containerFetch` Signature as the Exec Path**

- **Location:** Line 461-474 — `healthCheck()` port check loop.
- **Problem:** B17's signature bug applies here too. Every port check returns `{port, healthy: false}` because the `containerFetch` call is malformed. The health check fires 3 failures within 90s, transitions the container to `errored`, and the user's session is dead before they can connect.
- **Fix:** Same as B17 — use `this.ctx.container.getTcpPort(8080).fetch(new Request(...))` (or 9090 per B18).

### H13. **`onStart` Re-Entry Guard (M6) Is Defensive Theater — SDK Never Re-Invokes `onStart`**

- **Location:** Line 246-248: `if (this.state.status !== "starting") { throw new Error("onStart called in non-starting state: ..."); }`.
- **Problem:** The Container SDK calls `onStart` exactly once per `start()` call (verified at `container.js:677-688`). The "second `start()` from Worker" scenario is impossible — the second `start()` call goes through WP-01's `start()` RPC, which calls `super.start(...)` and the SDK's `start()` is idempotent (rejects the second invocation if the container is already running). The guard is dead code in production.
- **However:** the guard is not harmful — it adds an invariant assertion that's cheap. Keep it as defense-in-depth but downgrade the failure mode from `throw` to `this.log("error", "onStart_reentry_attempt", ...)` + return. A throw here masks the real "container is already running" error from the SDK, which then fires `onError` (cascade).
- **Fix:** Change the guard to a log-and-return:
  ```typescript
  if (this.state.status !== "starting") {
    this.log("warn", "onStart_reentry_attempt", { currentStatus: this.state.status });
    return;
  }
  ```

### H14. **`as { startedAt: number }` Cast Is Unsafe — Breaks Type Narrowing of the Union**

- **Location:** Line 515: `now - (this.state as { startedAt: number }).startedAt` and line 525: `(this.state as { wsConnections?: number }).wsConnections ?? 0`.
- **Problem:** `DevenvState` is a discriminated union (WP-01 §3.2). The `running` and `stopping` variants have `startedAt: number`; the `stopped`/`errored` variants do not. The `as` cast forces the type checker to treat the state as having the field even if it doesn't. The guard at line 514 `if (... === "running" || === "stopping")` is correct, but the cast defeats the type system: a future maintainer who changes the guard forgets the cast, and tsc passes (because the cast is wrong but valid syntax). Result: a silent runtime error when the guard is loosened.
- **Fix:** Use `Extract<DevenvState, { status: "running" | "stopping" }>` to narrow, then read `.startedAt` directly:
  ```typescript
  const uptimeMs = this.state.status === "running" || this.state.status === "stopping"
    ? Date.now() - (this.state as Extract<DevenvState, { status: "running" | "stopping" }>).startedAt
    : 0;
  ```
  Same pattern for `wsConnections` — but `wsConnections` is NOT in any variant of WP-01's union, so the cast is fabricating a field. Add `wsConnections: number` to the `running | starting | stopping` variants of WP-01's `DevenvState` (WP-05 owns the increment; WP-06 only reads). Cross-WP.
- **Why HIGH not BLOCKING:** The current guards are correct; the unsafe cast is a maintenance bomb that fires when someone changes the guards.

### H15. **`extractMutableFields` Function Undefined — Same Bug as B19 (Restated for Cross-Reference)**

- See B19 — same finding, listed here under HIGH for severity-by-impact on tests. The iter-1 M4 unit test skeleton at §9.1 doesn't test the idempotent path (only tests "is idempotent on same-state: running → running" line 845 which constructs the call with a `running` state — but doesn't actually invoke the line 205 spread). The test would fail with ReferenceError at runtime.

### H16. **`transitionState` Same-State Path Spreads `status` Through the Merge — Loses the `as const` Discriminator**

- **Location:** Line 205: `this.state = { ...this.state, ...extractMutableFields(newState) };`. Even after B19 is fixed, the spread overwrites `status` to `newState.status` (which equals `oldStatus`), but TS no longer narrows the resulting type to the correct discriminated-union variant. `this.state` is then typed as the **widened** `DevenvState` (no specific variant), and downstream reads like `this.state.workspaceName` return `string | undefined` instead of the expected `string`.
- **Fix:** After the spread, re-cast to the correct variant: `this.state = { ...this.state, ...mutable } as DevenvState;` — the cast is safe because `status` is preserved through the equality check.

---

## 🟡 NEW MEDIUM SEVERITY ISSUES

### M10. **`recordUsage` Stub Throws — `onError` Catch Path Will Throw If WP-04 Not Yet Implemented**

- **Location:** Line 587-589: `private async recordUsage(): Promise<void> { throw new Error("NOT_IMPLEMENTED_HERE: see WP-07 §3 recordUsage (canonical)"); }`. Called at line 356 (in `onStop`, after snapshot) and line 420 (in `onError`, after emergency snapshot).
- **Problem:** The stub throws unconditionally. If WP-04's actual `recordUsage` and WP-07's billing path aren't ready when WP-06 ships, the throw propagates through `onStop` and `onError`. The outer try/catch at line 369-379 (`onStop`) and line 424-432 (`onError`) catches the throw — `onError` is fine. But `onStop` re-throws (line 376) which the SDK treats as a stop failure. The user's last request gets a 5xx even though the snapshot succeeded.
- **Fix:** Make `recordUsage` fire-and-forget: wrap the body in try/catch and log. Document that the canonical impl (WP-04 §3.2) is also fire-and-forget (per WP-07 §3.2 I9). The stub:
  ```typescript
  private async recordUsage(opts?: { minimumBillableSeconds?: number }): Promise<void> {
    // Canonical impl lives in WP-04 §3.2 (rewritten in WP-07 §3.2). The stub
    // is fire-and-forget: failures MUST NOT block the lifecycle hook (WP-07 I9).
    try {
      throw new Error("NOT_IMPLEMENTED_HERE: see WP-04 §3.2 recordUsage (canonical)");
    } catch (err) {
      this.log("error", "recordUsage_stub_threw", { error: String(err) });
    }
  }
  ```

### M11. **`acquireSnapshotLock` Stub Throws on First Call — `onStop` and `onError` Both Fail**

- **Location:** Line 572-574. Called at line 346 (`onStop`) and line 399 (`onError`).
- **Problem:** Same as M10. If WP-04 §3.2 hasn't shipped `acquireSnapshotLock`/`releaseSnapshotLock`, both lifecycle hooks throw. `onError` is wrapped in try/catch (line 424) — emergency snapshot never happens. `onStop` re-throws — `super.stop()` sees a failure and the SDK calls `onError` after the partial-stop, which also throws.
- **Fix:** Document the dependency on WP-04 §3.2 being shipped FIRST, OR add a fallback inline implementation in WP-06 that doesn't depend on WP-04 (use a `private snapshotInProgress: boolean` flag on the instance, not on state).

### M12. **`containerPing` 2s Timeout Is Too Short for Cold Container**

- **Location:** Line 160: `signal: AbortSignal.timeout(2_000)`. Container cold-start is up to 60s (per `startAndWaitForPorts` `portReadyTimeoutMS: 60_000`).
- **Problem:** If the first health check fires before the container is fully warmed, the 2s `/ping` times out, `containerAlive = false`, the counter increments. After 3 cold-start probes (within 90s, but the first 60s of which the container may still be warming), the container transitions to `errored`.
- **Fix:** Either (a) skip the first health check until `startedAt + 30s` (grace period), or (b) bump the timeout to 5s for the first probe. Add a `firstCheck: boolean` flag on the state that gates the threshold counter.

### M13. **`healthCheck` Duplicated State-Status Guards**

- **Location:** Lines 443-445, 482, 487, 498, 510, 514, 524 — `this.state.status === "running" || === "stopping"` appears 5+ times in `healthCheck`.
- **Problem:** The repeated guard obscures the logic. If WP-01 adds a new active variant (e.g. `migrating`), all 5 sites need updates. The maintenance surface is wide.
- **Fix:** Hoist a `const isActive = this.state.status === "running" || this.state.status === "stopping";` at the top of `healthCheck`. Use `isActive` for all subsequent checks.

### M14. **WP-06 §3.1.1 Reference to "supervisord's other services" Is a Misnomer**

- **Location:** §3.1.1 line 66-67: "The server runs on port `8080` alongside code-server. It MUST be ready BEFORE supervisord's other services (so `onStart` can reach it after `startAndWaitForPorts`)."
- **Problem:** "supervisord's other services" is a category error — supervisord is the process manager; the services are the supervised programs (code-server, noVNC, ttyd, etc.). The sentence confuses two layers. Minor documentation hygiene.
- **Fix:** Rewrite as: "The exec-server runs on port 9090 (or 8080 if the WP-03 supervisord config is updated to move code-server). It MUST be ready BEFORE `onStart` calls `startAndWaitForPorts`, so port-wait doesn't time out."

---

## 🔄 CROSS-WP REGRESSIONS (Caused by WP-06 changes; need to be flagged to other WP reviews)

| WP | Regression | Detail |
|----|-----------|--------|
| **WP-07** | Quota guard still references `port_wait` | §3.3 line 253 `|| status.status === "port_wait"` and §8 line 533 — `port_wait` was removed from the union by WP-06 iter-1. WP-07 must drop the clause or extend WP-06's union. |
| **WP-01** | `envVars` instance field never added | WP-06's `this.currentEnvVars` getter reads `this.envVars` (line 170-172). WP-01's class (line 252) has no `envVars` field. WP-01 must either (a) add `private envVars: Record<string, string> = {};` and assign in `start()`, or (b) document that lifecycle hooks read from `ctx.storage` (cross-WP). |
| **WP-01** | `wsConnections` not in `DevenvState` union | WP-06 reads `this.state.wsConnections` (line 525, 525 cast). WP-01's union has no `wsConnections` field. WP-05 increments a `wsConnections` field (line 467-471) — but the field is not in the union; it must be. Cross-WP: WP-01 to add `wsConnections: number` to `running | starting | stopping` variants. |
| **WP-02 / WP-03** | Exec-server port 8080 conflicts with code-server | WP-06 §3.1.1 says exec-server runs on 8080; WP-03 line 528 has code-server on 8080. WP-02 (Dockerfile) and WP-03 (supervisord.conf) need to either move code-server to 9090 or move the exec-server to 9090. |
| **WP-04** | `acquireSnapshotLock` / `releaseSnapshotLock` are stubs | WP-06 §3.2.1 says "implementation in WP-04 §3.2". WP-04 §3.2 line 236-247 has the impl. If WP-04 doesn't ship first, WP-06 throws on first lifecycle call. WP-04 must be merged before WP-06 lands. |

---

## 🔍 ITER-1 DOD RE-VERIFICATION

| DoD Item | Iter-1 Status | Iter-2 Status | Delta |
|----------|---------------|---------------|-------|
| 1. State machine idempotent on same-state | ✅ B11 fixed | ❌ B19 — `extractMutableFields` undefined; ReferenceError | **REGRESSED** |
| 2. `onStart` waits for 3 ports via SDK | ✅ B7 fixed | ❌ B17 — wrong `containerFetch` signature | **REGRESSED** |
| 3. `onStart` transitions: starting → running | ✅ B5/H1 fixed | ✅ | OK |
| 4. `onStop(params)` no-op on terminal | ✅ B2/H11 fixed | ✅ | OK |
| 5. `onError` emergency snapshot, never rethrows | ✅ B3 fixed | ⚠️ H15/M10/M11 — stubs throw, try/catch swallows but snapshot never happens | **REGRESSED** |
| 6. `/_health` returns correct shape + auth | ✅ H5 fixed | ⚠️ H14 — `wsConnections` cast unsound; also no auth in WP-01 spec (token check is in §3.4, not yet landed in WP-01's `fetch()` body) | **PARTIAL REGRESSION** |
| 7. Health check every 30s via `schedule()` | ✅ B4 fixed | ✅ | OK |
| 8. 3 failures → errored, idempotent | ✅ H2 fixed | ❌ B19 — same ReferenceError on 4th | **REGRESSED** |
| 9. Port check uses exec-server `/port-check/:p` | ✅ B7 fixed | ❌ B17 — wrong signature, B18 — wrong port | **REGRESSED** |
| 10. State persists across restarts; new fields defaulted | ✅ M9 fixed | ⚠️ M9 fix only defaults `lastProfileSnapshot`/`lastWorkspaceSnapshot`; does NOT default `wsConnections` (cross-WP) | **PARTIAL** |
| 11. `recordUsage` calls canonical WP-07 path | ✅ B14 fixed | ⚠️ M10 — stub throws; WP-07 canonical path is in WP-04 §3.2, not in WP-07 §3.3 (the latter was REJECTED in iter-1) | **PARTIAL** |

**DoD Score:** 11/11 claimed PASS in iter-1 → **3 OK, 5 REGRESSED, 3 PARTIAL** in iter-2. The iter-1 verification was surface-level (claimed "✅" based on text presence, not on signature/runtime correctness).

---

## 🔍 INVARIANTS RE-VERIFICATION

| Invariant | Iter-1 Status | Iter-2 Status |
|-----------|---------------|---------------|
| I1: Transitions only via `VALID_TRANSITIONS`; same-state idempotent | ✅ | ❌ B19 (ReferenceError on idempotent path) |
| I2: `healthCheckFailures` reset to 0 on "running" | ✅ | ✅ (passes through the full target state at line 286) |
| I3: `healthCheckFailures` increments only on failed check | ✅ | ✅ (line 483) |
| I4: 3 consecutive failures → errored | ✅ | ❌ (B19 — counter increments but transition throws on the 4th-call idempotent path) |
| I5: 60s port wait + 10 min `onStart` budget | ✅ | ✅ |
| I6: `onStop` + `onError` both call `recordUsage` once | ✅ | ⚠️ M10 (stub throws; outer try/catch hides it but actual billing doesn't happen) |
| I7: Schedule rescheduled only when not terminal; `deleteSchedules` on terminal | ✅ | ✅ |
| I8: Counter capped at 3, idempotent on errored | ✅ | ❌ B19 |
| I9: All exec via `containerFetch("http://localhost:8080/...")` | ✅ | ❌ B17 (wrong signature), B18 (wrong port) |
| I10: `onStop` no-op on terminal | ✅ | ✅ |
| I11: Cross-WP stubs reference WP-04/07, not duplicated | ✅ | ✅ (correct ownership delegation) |

**Invariants Enforced:** 11/11 claimed → **6/11 (54%)** in iter-2. The state-machine text is correct but the *implementation* has 5 broken invariants (I1, I4, I6, I8, I9).

---

## 🔍 QUALITY STANDARDS RE-VERIFICATION

| Standard | Iter-1 Status | Iter-2 Status |
|----------|---------------|---------------|
| State Machine: explicit transitions + audit log | ✅ | ❌ B19 (audit fires but the underlying transition crashes) |
| Observability: every transition logged with traceId | ✅ | ✅ |
| Health Checks: active probing (container exec) | ✅ | ❌ B17 (exec broken) |
| Failure Detection: 3 consecutive = hard failure | ✅ | ❌ (crashes on 4th instead of transitioning) |
| Crash Safety: `onError` fires on ANY container exit | ✅ | ⚠️ (signature correct, but the body throws on stub dependencies — M10/M11) |
| Idempotency: health check safe under concurrent traffic | ✅ | ✅ (DO is single-threaded; no race) |

**Quality Standards:** 6/6 claimed → **2/6 (33%)** in iter-2.

---

## 📊 UPDATED SCORECARD

| Category | Iter 1 | Iter 2 | Trend |
|----------|--------|--------|-------|
| Blocking Issues | 14 → 0 | 5 NEW | ❌ Regression |
| High Issues | 11 → 0 | 4 NEW | ❌ Regression |
| Medium Issues | 9 → 0 | 4 NEW | ❌ Regression |
| DoD Pass Rate | 100% | 27% (3/11) | ↓↓ |
| Invariants Enforced | 100% | 54% (6/11) | ↓ |
| Quality Standards | 100% | 33% (2/6) | ↓↓ |

**OVERALL VERDICT: ❌ FAIL — Iteration 2 found critical regressions in the exec-server call signature (B17), envVars field plumbing (B15), same-state idempotency (B19), and port allocation (B18). Plus 4 cross-WP coordination gaps. The lifecycle + state machine + alarm-scheduling fixes from iter-1 are sound, but the *transport layer* is still broken. 2-3 more iterations likely needed.**

---

## 🔧 FIXES NEEDED FOR ITERATION 2

### Must Fix (Blockers) — applied below in the doc
1. **B15**: WP-01 must add `private envVars: Record<string, string> = {};` and assign in `start()`. WP-06 deletes the redundant getter.
2. **B16**: Cross-WP — flag WP-07 to remove `port_wait` references. (Applied in WP-07 not WP-06.)
3. **B17**: Replace all 4 `containerFetch(url, options, port)` calls with `this.ctx.container.getTcpPort(9090).fetch(new Request(url, {method, headers, body, signal}))`.
4. **B18**: Move exec-server to port 9090 (not 8080). Update §3.1.1 + every call site.
5. **B19**: Inline the `extractMutableFields` helper — explicit `{status: _ignored, ...mutable} = newState` pattern.

### Should Fix (High)
1. **H12**: Same fix as B17.
2. **H13**: Change M6 re-entry guard from `throw` to `log + return`.
3. **H14**: Use `Extract<DevenvState, ...>` to narrow instead of `as` cast. Cross-WP with WP-01 for `wsConnections`.
4. **H15**: Same as B19.
5. **H16**: Add `as DevenvState` cast after the mutable merge.

### Nice to Fix (Medium)
1. **M10**: Fire-and-forget wrapper in `recordUsage` stub.
2. **M11**: Inline snapshot lock in WP-06 (don't depend on WP-04 §3.2 stub) — OR document hard ordering requirement.
3. **M12**: 5s first-probe grace period for cold container.
4. **M13**: Hoist `isActive` local in `healthCheck`.
5. **M14**: Rewrite §3.1.1 wording.

---

## 🔗 CROSS-WP COORDINATION NEEDS (Updated for Iter 2)

| WP | Coordination Need |
|----|-------------------|
| **WP-01** | Add `private envVars: Record<string, string> = {};` instance field; assign in `start()` from `newEnvVars`. Add `wsConnections: number` to `DevenvState` union's `running | starting | stopping` variants. Add HEALTH_TOKEN to `Env` type. |
| **WP-02** | Dockerfile must move code-server to a different port (or move the exec-server to 9090) — pick one. Exec-server binary must be installed by WP-02 (currently not in WP-02's binary list). |
| **WP-03** | supervisord.conf must NOT bind code-server to 8080 if exec-server is on 8080. Add the exec-server to the `[program:exec-server]` block on the chosen port. Ensure exec-server starts BEFORE other services so `startAndWaitForPorts` succeeds. |
| **WP-04** | `acquireSnapshotLock`/`releaseSnapshotLock`/`hydrateViaClw`/`snapshotViaClw` must be SHIPPED before WP-06 (M11 hard ordering requirement). |
| **WP-05** | `wsConnections` field name must match exactly between WP-05 (writes) and WP-06 (reads). Currently WP-05 writes to `this.state.wsConnections` (cast — see H14) but WP-01's union doesn't have the field. |
| **WP-07** | Remove `|| status.status === "port_wait"` clause (B16). Cross-WP edit in next iter. |
| **WP-08** | Add `HEALTH_TOKEN` secret to wrangler config + Worker ingress. WP-08 reads the token to call `/_health` (currently WP-06 §3.4 references it but the secret isn't declared anywhere). |

---

## NEXT STEPS

1. **Apply all 5 BLOCKING fixes (B15-B19)** to WP-06
2. **Apply all 4 HIGH fixes (H12-H16)** to WP-06
3. **Apply 3 of 5 MEDIUM fixes (M10, M12, M13, M14)** to WP-06 (skip M11 — keep as cross-WP ordering requirement)
4. **Flag cross-WP fixes to WP-01, WP-02, WP-03, WP-04, WP-05, WP-07, WP-08** for coordinated update
5. **Re-verify all 11 DoD items, 11 invariants, 6 quality standards**
6. **Proceed to Iteration 3 review**

**Estimated convergence: 2-3 more iterations.** The exec-server transport (B17, B18, H12) is the dominant gap; once that's right, the state machine and lifecycle are sound. The cross-WP `envVars` field (B15) and `wsConnections` field (H14) are second-order but block compilation.

---

**END OF WP-06 ITERATION 2 REVIEW**
