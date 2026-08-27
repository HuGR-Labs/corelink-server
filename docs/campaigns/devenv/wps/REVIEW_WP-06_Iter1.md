# WP-06 REVIEW — Iteration 1 (CRITICAL AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Verdict:** ❌ **FAIL — 14 BLOCKING ISSUES, 11 HIGH SEVERITY ISSUES, 9 MEDIUM SEVERITY ISSUES**

---

## 🔴 BLOCKING ISSUES (Must fix before sign-off)

### B1. **`ctx.container.exec()` Does Not Exist on `@cloudflare/containers`**
- **Location:** `execClw()` line 81, `ensureDataDirectories()` line 178, `waitForRequiredPorts()` line 219, `healthCheck()` line 328, and throughout `WP-04 §3.2`
- **Code:** `await this.ctx.container.exec({ cmd: [...], timeoutMs, stdout, stderr })`
- **Problem:** The `Container` class has NO public `exec()` method. `ctx.container` is a private field in `@cloudflare/containers` SDK (`container.js:374: this.container = ctx.container`) used internally for port checks and `containerFetch`. The public API offers only `containerFetch()` (HTTP) and `startAndWaitForPorts()`. There is no `ExecProcess` shape and no `output()` stream.
- **Real API (verified in `corelink-runners/deploy/cloudflare/node_modules/@cloudflare/containers/dist/lib/container.d.ts`):** zero `exec` declarations. The `RunnerContainer` in `index.ts:395` never execs; the existing `/v1/exec` route (`index.ts:3228`) implements exec by dialing an in-container exec-server over HTTP on port 8080 (`containerFetch(req, 8080)`).
- **Fix:** Two viable paths, both must be picked explicitly:
  1. **Add an exec-server in the container** (port 8080 already exists — extend it to accept `POST /clw {argv:[…]}` returning `{exit_code, stdout, stderr}`), then call via `this.containerFetch("http://localhost:8080/clw", {method:"POST", body:JSON.stringify({argv})})`. Mirrors the existing `/v1/exec` (`index.ts:3228`).
  2. **Use `containerFetch` against supervisord's control socket** (only works if supervisord is configured to accept a Unix-socket control API). Reject — too fragile.
- **Do not** ship a spec that calls a non-existent method. Either path requires WP-02 / WP-03 to add the exec-server, and WP-04/06 to use it.

### B2. **`onStop(params)` Signature Wrong — Takes `StopParams`, Not `void`**
- **Location:** `onStop()` line 234: `override async onStop(): Promise<void>`
- **Problem:** `@cloudflare/containers` declares `onStop(params: StopParams): void | Promise<void>` (`container.d.ts:220`). `StopParams` is `{ exitCode: number; reason: string }`. The WP stubs `() =>` which is a *wider* type so TypeScript will accept, but the override silently loses the `exitCode` and `reason` that the SDK passes (especially the `reason` which discriminates graceful-stop from crash, critical for the WP-06 "did this `onStop` follow a real stop or a crash?" decision).
- **Fix:** `override async onStop(params: StopParams): Promise<void>` and use `params.reason` in the audit log.

### B3. **`onError(error: unknown)` Signature Wrong — Not `Error`**
- **Location:** `onError()` line 277: `override async onError(err: Error): Promise<void>`
- **Problem:** SDK declares `onError(error: unknown): unknown` (`container.d.ts:236`). The WP narrows to `Error` and discards the `unknown` return. Two concrete bugs:
  1. The SDK can pass non-Error values (strings, plain objects) — `error.message` will throw.
  2. The WP signature's return type is `Promise<void>`; SDK allows `unknown` (the value is logged, not consumed). Returning `void` is OK as a narrowing but `unknown` is the parent's contract.
- **Fix:** `override async onError(error: unknown): Promise<void>` and defensively coerce: `const msg = error instanceof Error ? error.message : String(error);`

### B4. **Re-Arming the Alarm via `this.ctx.storage.setAlarm()` Fights the SDK's Own Reschedule**
- **Location:** `alarm()` line 400 + `onStart()` line 423
- **Code:** `this.ctx.storage.setAlarm(Date.now() + 30_000);`
- **Problem:** The SDK's own `Container.alarm()` (verified in `container.js:1502-1518`) does:
  1. `await this.ctx.storage.setAlarm(prevAlarm)` — re-arms with `Date.now()` (i.e. *immediately* fires next tick)
  2. Processes any registered `container_schedules` table rows
  3. Schedules the next alarm via `scheduleNextAlarm()` from any pending schedules
  The SDK comment is explicit: "do not remove this, container DOs ALWAYS need an alarm right now" (`container.js:1513`).
- If WP-06 overrides `alarm()` and the implementation forgets to call `super.alarm(alarmProps)`, the SDK's internal `container_schedules` machinery breaks. If it does call `super.alarm(alarmProps)`, the SDK **immediately rearms** with `Date.now()` and overwrites the 30s delay WP-06 just set. The "every 30s" cadence is therefore impossible to honor via the existing `alarm()` path. The SDK's recommendation (verified in `container.d.ts:252-253`): use `this.schedule(when, "callbackName", payload)` — `schedule()` is durable (SQL table `container_schedules`) and survives eviction.
- **Fix:** Replace the alarm-based loop with `this.schedule()`:
  ```ts
  await this.schedule(Date.now() + 30_000, "healthCheckTick");
  ```
  And implement:
  ```ts
  async healthCheckTick(): Promise<void> {
    if (this.state.status === "running") await this.healthCheck();
    if (this.state.status !== "stopped" && this.state.status !== "errored") {
      await this.schedule(Date.now() + 30_000, "healthCheckTick");
    }
  }
  ```
  Cancel via `this.deleteSchedules("healthCheckTick")` in `requestStop` / `onStop`.

### B5. **State Machine: `port_wait` Not in WP-01's `DevenvState` Discriminated Union**
- **Location:** `VALID_TRANSITIONS` line 85; `transitionState("port_wait", ...)` line 145; `onStart` line 188 (`port_wait` referenced in transitions)
- **Problem:** WP-01 §3.2 (final, after Iter 4) ships a `DevenvState` discriminated union with 5 variants: `stopped | starting | running | stopping | errored`. WP-06 introduces a 6th `port_wait` and a 7th `provisioning` state without adding them to the union. The result: `transitionState(newStatus: DevEnvStatus, ...)` is called with `"port_wait"` but the helper then *manually mutates `this.state = { ...this.state, status: newStatus, ... }`* (line 108) which:
  1. Violates WP-01's `I1` ("state.status always one of 5 values" — `tsc --strict` cannot enforce it any more).
  2. Produces a state with the wrong shape for `running` (no `containerHandle`, no `lastHealthCheckAt`), breaking the WP-01 `buildStatusResponse()` narrowing in `index.ts:613-647`.
- **Fix:** Either (a) update `DevenvState` in `src/types/devenv.ts` to add `provisioning` and `port_wait` variants with their own payload, or (b) fold `provisioning` into `stopped` (current) and replace `port_wait` with an internal boolean (since the port check is a synchronous sub-phase of `starting`, not a top-level state). Option (b) is the smaller change: `port_wait` becomes a private flag on the `starting` variant, not a state.

### B6. **Transition Helper Mutates State Without Going Through the Discriminated Union's Required Fields**
- **Location:** `transitionState()` line 99-119
- **Code:** `this.state = { ...this.state, status: newStatus, ...(newStatus === "starting" && { startedAt: Date.now() }), ...(newStatus === "running" && { healthCheckFailures: 0 }), ...(newStatus === "errored" && { lastError: metadata?.error as string }) }`
- **Problem:** This is a *flat* shape. WP-01 ships a *discriminated union* where `running` requires `containerHandle`, `lastHealthCheckAt`, `healthCheckFailures`, `workspaceName`, `profileName`, `startedAt`, `createdAt`. The spread leaves those undefined for the `running` transition (e.g. `containerHandle` only gets set in `onStart` line 162 *after* `transitionState("running")` ran on line 151 — a window where the state is `running` with `containerHandle: undefined`). WP-01's `buildStatusResponse()` (`index.ts:645: containerHandle: this.state.status === "running" ? this.state.containerHandle : null`) will then return `null` and the invariant `I3: containerHandle non-null iff running` is broken.
- **Fix:** Re-design `transitionState` to take the full target state (not just a status), exactly like WP-01 Iter 4 (`runner_dev_env.ts:291-312`). Drop the `metadata.error as string` cast; require a full `DevenvState` value.

### B7. **`/dev/tcp/localhost/PORT` Port Check Uses Bash TCP-Redirect — Doesn't Work on Alpine Containers**
- **Location:** `checkPort()` line 216-228
- **Code:** `bash -c "</dev/tcp/localhost/${port}"` via `this.execClw(["exec", "bash", "-c", ...])`
- **Problem:** This is a *bash extension* (bash opens `/dev/tcp/...` as a pseudo-TTY and reads from it). It is NOT portable:
  1. The base image is **Alpine** (WP-02, busybox+ash by default; `bash` is a separate apk and *not present unless installed by WP-02's Dockerfile*). WP-02 must install bash explicitly.
  2. WP-06 assumes `bash` is on `$PATH` (line 220: `cmd: ["bash", "-c", ...]`) but WP-02 (not yet reviewed) hasn't committed to installing bash. If WP-02 ships busybox-only Alpine, every port check throws `exec: "bash": executable file not found` and the container can never reach `running`.
- **Fix:** Use the SDK's built-in `startAndWaitForPorts()` (verified `container.d.ts:185-187`) which polls internally and uses the same TCP probe. If the exec approach is required, replace bash with `nc -z localhost PORT` (Alpine has it via busybox) or install bash in WP-02 and *document the contract in WP-02's DoD*.

### B8. **Container Execs from DO to Container Use `this.envVars` But It Doesn't Exist on the Class**
- **Location:** Lines 138, 142, 192, 246, 250, 290, 291
- **Code:** `await this.hydrateViaClw("/data/chrome", this.envVars.PROFILE_NAME);`
- **Problem:** WP-01 ships `STATIC_ENV_VARS` as a `private static readonly` (constant) and a *mutable* per-instance envVars map assembled in `start()` (`runner_dev_env.ts:336-342`). The merged result is **passed to `super.start({ envVars: newEnvVars })`** and not stored on `this`. The class has no `envVars` instance field. The WP-06 code reads `this.envVars.PROFILE_NAME` → `undefined`, then passes `undefined` to `clw hydrate --name` → clw errors with `--name requires a value` and `onStart` crashes.
- **Fix:** Persist the merged envVars on the instance: `private currentEnvVars: Record<string, string> = {};` populated in `start()` and read in lifecycle hooks. Or read the values from `this.state` (workspaceName/profileName are already there; tenant/token are redacted and must be re-fetched from `this.ctx.storage.get("envVars")`).

### B9. **`acquireSnapshotLock()` / `releaseSnapshotLock()` Defined and Called But Never Declared**
- **Location:** Lines 242, 269, 284, 312
- **Problem:** The class never declares these methods. WP-04 §3.2 has them but WP-06's spec shows the lifecycle hooks as if they live in `RunnerDevEnvDO` directly, and there is no `private async acquireSnapshotLock` definition in WP-06's code listing. The file does not compile.
- **Fix:** Inline the lock primitives in WP-06 (treat as a WP-04 primitive that the DO *calls* but is owned by WP-04): state `this is provided by WP-04 §3.2` and add a one-line stub: `private acquireSnapshotLock() { /* see WP-04 §3.2 */ throw new Error("NOT_IMPLEMENTED"); }` — OR, more rigorously, lift the lock into a private `inflightSnapshot: Promise<void>` field on the DO and document the contract.

### B10. **`hydrateViaClw` / `snapshotViaClw` / `recordUsage` Referenced But Not Defined**
- **Location:** Lines 138, 142, 246, 250, 290, 291, 253, 307
- **Problem:** Same as B9 — WP-04 owns the definitions, but WP-06 calls them as if they are class methods. There is no `import` statement, no `private` declaration. The spec is internally inconsistent: it claims WP-06 implements the lifecycle hooks but its hooks call methods that are not in this file.
- **Fix:** Either (a) add the `import { /* see WP-04 */ }` block at the top of WP-06's code listing with the `// implementation: WP-04 §3.2` comment per method, or (b) inline the 3 clw helpers + `recordUsage` in WP-06 §3.2 and remove the WP-04 §3.2 duplicates. Option (a) is cleaner and respects the WP-04 / WP-06 split.

### B11. **`transitionState` Throws Inside the Audit Log Path — Liveness Bug**
- **Location:** `transitionState()` line 99-119
- **Code:** `this.log(err); throw new Error(err);` when an invalid transition is detected
- **Problem:** `onStart` line 169 calls `this.transitionState("errored", { traceId, error: String(err) })` from a `catch (err)` block. If the *previous* transition was already `errored` (e.g. a port-check failure throws, we transition `port_wait → errored`, then the surrounding catch sees an error and tries `transitionState("errored")` again), the helper throws `INVALID_STATE_TRANSITION: errored → errored` and **the original error is lost** — the container is now in `errored` state but the DO has thrown out of `onStart` without the `onError` hook ever firing (since the SDK swallows `onStart` errors and falls back to `onError`, per `container.js:677-688`). The DO's `errored` state's `lastError` will then be the bogus "INVALID_STATE_TRANSITION" string, not the real port-timeout cause.
- **Fix:** Make `transitionState` *log* invalid transitions and return without throwing in the "already at target" / "transitioning during cleanup" cases. At minimum, special-case `errored → errored` as a no-op (just update `lastError` and return).

### B12. **Alarm Reschedule Skips on `stopped`/`errored` But Fails to Clear Pending Alarm**
- **Location:** `alarm()` line 407-409
- **Code:** `if (this.state.status !== "stopped" && this.state.status !== "errored") { this.ctx.storage.setAlarm(Date.now() + 30_000); }`
- **Problem:** If the alarm is firing while the container is in `stopped` (e.g. one final tick after `onStop` completes), the `if` branch is skipped — but the SDK's own `super.alarm(alarmProps)` (which the spec doesn't show being called!) re-arms unconditionally at `Date.now()` (verified `container.js:1518`). The next alarm fires immediately, then again, ad infinitum until the DO is evicted. After eviction, no more `alarm()` runs (no schedules, no container), so it's contained — but **inside a hot loop** it wastes thousands of invocations per second on a Durable Object that should be quiet.
- **Fix:** Either call `await this.ctx.storage.deleteAlarm()` when entering `stopped`/`errored`, or replace the alarm with `this.schedule()` (see B4) which has explicit lifecycle.

### B13. **`execClw` `captureOutput: false` and the `--json` Flag Are Contradictory**
- **Location:** WP-04 §3.2 `execClw` line 81-100, called from `ensureDataDirectories` line 178-182 with `captureOutput: false`
- **Problem:** `ensureDataDirectories` calls `execClw(["exec", "mkdir", "-p", "/data/chrome", "/data/workspace"], { captureOutput: false, timeoutMs: 5_000 })`. There is no `--json` here, but the call is fine for `mkdir`. However, the WP-04 helper passes `["/usr/local/bin/clw", ...args]` — so the actual command becomes `/usr/local/bin/clw exec mkdir -p /data/chrome /data/workspace`, which routes the `mkdir` *through* clw (clw is a snapshot tool, not a shell). `clw exec` is not a real subcommand in the clw CLI. The clw binary will respond with "unknown subcommand: exec" and the helper throws. (WP-04's helper treats the command as if it goes to bash; it should be a flat argv to clw itself.)
- **Fix:** Replace the mkdir via clw pattern with the actual path:
  - `this.execClw(["exec", "mkdir", ...])` should be `this.execClw(["mkdir", ...])` if `execClw` already prefixes `/usr/local/bin/clw`, OR drop the `clw` prefix and call `this.containerFetch("http://localhost:8080/exec", { method: "POST", body: JSON.stringify({argv:["mkdir","-p","/data/chrome","/data/workspace"]}) })` (the exec-server pattern from B1).
- This is a B-class issue because the entire `onStart` sequence fails on a fresh container (no `/data/chrome` yet, `mkdir` errors, hydrate fails, container is `errored`).

### B14. **`recordUsage` Posts to Non-Canonical Endpoint — WP-07 Has Already Rejected This**
- **Location:** WP-04 §3.2 `recordUsage()` line 336-359 (`POST ${this.env.BILLING_INGEST_URL}/v1/usage`)
- **Problem:** WP-07 §1 explicitly states: "DO NOT introduce a second parallel pipeline" and rejects the WP-04 `/v1/usage` path in favor of the canonical `/internal/v1/billing/usage` (`corelink-container/src/routes/billing_ingest.rs`). The D1 table `runner_devenv_usage` is also REJECTED in WP-07 — events go to `usage_event_staging` (migration 0017) and roll up via `usage_daily` (migration 0089). WP-06 inherits this contract: calling `recordUsage()` from `onStop`/`onError` will post to the wrong endpoint and insert into the wrong table.
- **Fix:** Replace WP-04 §3.2's `recordUsage` body with the WP-07 §3 payload shape, and add a comment in WP-06 §3.2 saying `recordUsage` is owned by WP-07 (the body is a stub that delegates to the `UsageEventEmitter` injected via `env.BILLING_DO`). Document the field contract in WP-06's I6 invariant: "recordUsage emits a `UsageEventKind::DevenvVcpuSeconds` to `env.BILLING_DO`, NOT a `fetch()` to a custom endpoint."

---

## 🟠 HIGH SEVERITY ISSUES

### H1. **No `Provision` State — First Call to `onStart` Is in `errored` Because There Is No Initial State**
- **Location:** `transitionState("provisioning", { traceId })` line 131 — but `provisioning` is not in WP-01's union
- **Problem:** After WP-01's fix, the initial state is `stopped`. The first call to `onStart` is from the SDK *after* `start()` was called on a `stopped` DO. WP-06's first transition in `onStart` is `provisioning → starting` — but the current state is `stopped`, not `provisioning`. The state machine throws `INVALID_STATE_TRANSITION: stopped → provisioning` on the very first start. The `stopped → starting` transition is already covered by WP-01's `requestStop` flow (which transitions to `stopping` then `stopped`); WP-06's `provisioning` was added without a path from `stopped`.
- **Fix:** Drop `provisioning` (use `stopped` as the pre-start state, then `stopped → starting` per WP-01's `start()`). Or, if `provisioning` is genuinely needed for DO creation, add it to WP-01's union with its own variant.

### H2. **3-Failure Threshold Caps the Counter but the State Machine Throws on the 4th `running → errored`**
- **Location:** `healthCheck()` line 354-358
- **Problem:** The threshold guard says `>= 3` and calls `transitionState("errored", ...)`. But once the state is `errored`, the *next* `healthCheck()` call (alarm at 30s) tries `transitionState("errored")` again from `onStart`'s caller path (or directly from `alarm()`), which throws `INVALID_STATE_TRANSITION: errored → errored`. The state persists but the throw propagates to the SDK, which then fires `onError` (per `container.js:1351-1366`) and the `lastError` becomes the throw message instead of the original health-check reason.
- **Fix:** B11's fix (idempotent transition) addresses this directly. Add `|| newStatus === oldStatus` to the early-return check.

### H3. **`containerAlive` Probe Uses `clw exec true` But `execClw` Routes Through clw, Not a Shell**
- **Location:** `healthCheck()` line 328
- **Code:** `await this.execClw(["exec", "true"], { timeoutMs: 2_000, captureOutput: false });`
- **Problem:** Same as B13. With WP-04's helper, this becomes `/usr/local/bin/clw exec true`, which clw doesn't understand. The probe always fails, so `containerAlive` is always `false`, so every health check increments the failure counter, so the 3-failure cap fires within 90 seconds of every healthy container starting.
- **Fix:** Use `this.containerFetch("http://localhost:8080/ping", { signal: AbortSignal.timeout(2_000) })` once the exec-server from B1 is in place, or use `this.getState()` (SDK method, `container.d.ts:78`) which returns the container's runtime state without an exec.

### H4. **Port Check `Promise.all` Short-Circuits on First Failure — Slow Ports Starve the Loop**
- **Location:** `waitForRequiredPorts()` line 197-200
- **Code:** `const allReady = await Promise.all(ports.map(port => this.checkPort(port)));`
- **Problem:** `Promise.all` does NOT short-circuit (it waits for all), so this is actually fine for correctness. The real problem is `checkPort()` invokes `execClw` which has a 2s timeout *per port*, and 3 ports sequentially inside `Promise.all` is 2s wall + 2s wall + 2s wall (parallel) = up to 2s. OK. But if `execClw` is waiting on a bash exec (B7) that's blocked on a half-open TCP socket (no RST), bash's `timeout 1` *doesn't actually fire* because the TCP probe is in the kernel — bash just hangs. The `timeoutMs: 2_000` in `execClw` is the SDK timeout, but the inner `timeout 1` (seconds) was meant to bound the bash wait. Real wall time per probe: up to 2s (good case) to **infinite** (worst case where bash's `timeout` doesn't apply because bash's `</dev/tcp/...` blocks in the bash process, not the spawned `timeout` PID).
- **Fix:** Replace with the SDK's `startAndWaitForPorts()` which uses an HTTP probe against the container's internal healthcheck port (if configured) or a kernel-level connect with its own timeout. The bash/TCP-probe pattern is fragile.

### H5. **No `fetch()` Override Cleanup — `/_health` Path Doesn't Authenticate**
- **Location:** `fetch()` line 373-383
- **Problem:** WP-01's `fetch()` already routes `/_health` to `this.healthCheck()` (line 528-532 of WP-01). WP-06's `override async fetch(request)` definition **replaces** WP-01's whole `fetch()` (the `override` keyword replaces, not merges). After WP-06 ships:
  1. WebSocket upgrades (`/vnc`, `/tty`, `/code`) are dropped
  2. `/api/status`, `/api/snapshot`, `/api/resize` are dropped
  3. The `super.fetch(request)` fallback is dropped
  4. `/_health` has no auth — anyone with the DO's address (which is `idFromName(tenantId)` — a brute-forceable mapping!) can read container state, snapshot metadata, ws_connections, health-check-failure count. Reconnaissance goldmine.
- **Fix:** Either (a) replace the `override async fetch()` with a *single line* `// see WP-01 §3.3 fetch() implementation` and just add a health-check branch — DO NOT redefine the whole method, OR (b) reproduce WP-01's full `fetch()` body in WP-06 and add an auth check on `/_health` (e.g. require `Authorization: Bearer <health-token>` header sourced from a wrangler secret).

### H6. **`wsConnections` Field Referenced in `alarm()` but Never Updated**
- **Location:** `alarm()` line 412-415
- **Problem:** `this.state.wsConnections` is read and `lastActivityAt` is updated when `wsConnections > 0`. But no code in the entire WP-06 spec increments or decrements `wsConnections`. It's always 0. The condition is dead code. WP-05 is the owner of WS lifecycle, and the WP-05 spec must increment on `acceptWebSocket()` and decrement on `webSocketClose()`. WP-06's invariant depends on WP-05 to function, and that contract is not stated.
- **Fix:** Add a contract in WP-06 §3.2: "WP-05 increments `state.wsConnections` in `proxyWebSocket` and decrements in `webSocketClose`/`webSocketError` (Hibernation runtime handlers). WP-06 reads it but does not mutate it." Cross-reference WP-05 §3.3 with the field name spelled identically.

### H7. **`execClw` `timeoutMs: 5_000` for `mkdir` Is Fine, but `timeoutMs: 600_000` (10 min) for `hydrate` Is Not Bounded by Anything Real**
- **Location:** WP-04 `execClw` line 85
- **Problem:** Default `300_000` (5 min), but `hydrate` callers pass `600_000`. The 10-min bound is the WP-04 I5 invariant. However, the SDK's `Container.exec()` (or `containerFetch` if we use that) has its own hard limit (`container.js` enforces `MAX_ALARM_RETRIES` and HTTP fetch is bounded by `signal`/context-deadline). A 10-min wall-clock `await` in a DO handler holds the DO's input gate for 10 min — during which time the alarm cannot fire, RPCs queue, the DO is "busy" and the runtime may evict. There is no `AbortSignal` plumbing to let the SDK's own timeout fire.
- **Fix:** Use `Promise.race` with an internal timeout, or pass an `AbortSignal` and propagate cancellation. Document that the DO will hold its single-threaded input gate for the full window — and that this is *the wrong shape*; the snapshot must be a cron-driven background job, not a synchronous handler.

### H8. **Health Check Doesn't Distinguish "Container is Up But Port is Down" From "Container Is Down"**
- **Location:** `healthCheck()` line 320-370
- **Problem:** A port that is genuinely broken (e.g. supervisord lost a process) and a container that crashed completely are treated identically (`overallHealthy = false`). For the broken-port case, the right response is "snapshot state, then destroy + respawn", not "transition to errored" (which is terminal in WP-01's machine).
- **Fix:** Split:
  - `containerAlive = false` → `transitionState("errored")` immediately
  - `containerAlive && !allPortsHealthy` → record as failure, but do NOT transition; log a `RESTART_PORT_HEALTH` event and after N failures, call `this.destroy()` then `this.start()` (respawn).
- Document the respawn cap in invariants (e.g. I-9: "max 2 respawns per hour to prevent loop").

### H9. **`lastHealthCheckAt` / `healthCheckFailures` Set in `healthCheck()` but Not Persisted Until `persistState()`**
- **Location:** `healthCheck()` line 346-351
- **Code:** `this.state = { ...this.state, healthCheckFailures: ..., lastHealthCheckAt: now }; this.persistState();`
- **Problem:** This is OK *if* the DO isn't evicted between the assignment and the `persistState()`. But `healthCheck()` is called from `alarm()` (line 403) and from the `/_health` RPC. If the DO is evicted *during* `await this.execClw(...)` inside `checkPort()` (line 339), the partial state update is lost — the next `healthCheck()` re-reads from storage with the OLD `healthCheckFailures`. The "3 consecutive failures" guarantee becomes probabilistic.
- **Fix:** Persist before the awaits. Or, more simply, persist each individual check separately and count at the alarm level (`ctx.storage.sql`).

### H10. **`onError` Calls `recordUsage` Even When `startedAt` Is `null` (e.g. crash before start completed)**
- **Location:** `recordUsage` line 336-337 in WP-04
- **Code:** `if (!this.state.startedAt) return;`
- **Problem:** If the container crashes before `onStart` finishes (e.g. `hydrate` throws after 8 minutes), `startedAt` is null. `recordUsage` silently no-ops. The customer is not billed. **But the customer's `requestStop` is queued** — so the Worker ingress has already done a billing-period check (per WP-07) assuming the session was legitimate. The discrepancy (Worker thinks session started, billing has no record) is a reconciliation bug.
- **Fix:** Track `createdAt` as the start of the *attempted* session, bill for at least one minute, and tag the event with `event_kind: "devenv_vcpu_seconds"`, `idem_key: BLAKE3(tenant|container|createdAt)`. WP-07 must own this; document in WP-06 I6.

### H11. **No `monitor()` Re-Subscription on `onStart` Re-Entry**
- **Location:** Implicit (no `monitor()` call in WP-06)
- **Problem:** The Container SDK's `monitor()` (inherited, returns a promise that resolves when the container exits) is what calls `onStop` / `onError` after the initial start. WP-01's skeleton doesn't override it. WP-06 doesn't override it. So far so good. But after `onError` transitions to `errored` and the operator manually calls `requestStop` (which becomes `destroy()` in errored state per WP-01 line 376-380), the monitor promise resolves, `callOnStop` runs (`container.js:1605`), and `onStop()` fires — but the state is `stopped` (WP-01's `requestStop` transitions to `stopped` BEFORE calling `destroy()`), and the `onStop` body tries to `acquireSnapshotLock` + `snapshotViaClw` on a destroyed container, which throws, which tries `transitionState("errored")` from `stopped` (B11 territory), which throws again. The DO ends up with two error states on the wire.
- **Fix:** Make `onStop` a no-op when state is `stopped` (the snapshot is moot). Pattern: `if (this.state.status === "stopped" || this.state.status === "errored") return;` at the top of `onStop`.

---

## 🟡 MEDIUM SEVERITY ISSUES

### M1. **`PersistedState` Type Mismatch — WP-06 Uses a Flat `DevEnvState` Interface While WP-01 Uses a Discriminated Union**
- **Location:** `interface DevEnvState` line 67-82
- **Problem:** WP-06 redefines `DevEnvState` from scratch as a flat interface (all fields always present, mostly nullable). WP-01 §3.2 ships a discriminated union. There is no `import` from `src/types/devenv.ts` in WP-06. Two parallel definitions → tsc allows them (they're different types in different files), but the persisted state shape in `this.ctx.storage.put("state", this.state)` will mismatch WP-01's `buildStatusResponse()` narrowing.
- **Fix:** Import `DevenvState` from `src/types/devenv.ts` (per WP-01 §3.2). Extend the union with the two new variants (`provisioning`, `port_wait`) or fold them out (B5's fix). The `SnapshotMetadata` type from WP-04 §3.2 must also be imported (not redeclared).

### M2. **`SnapshotMetadata` Type Re-declared Instead of Imported**
- **Location:** WP-04 §3.2 line 56-62 declares it; WP-06 line 76-77 uses it
- **Problem:** Same as M1. Two sources of truth.
- **Fix:** Move `SnapshotMetadata` to `src/types/devenv.ts` and import in both.

### M3. **`HealthCheckResponse` Re-declared Instead of Imported**
- **Location:** WP-06 line 385-394; WP-01 §3.2 line 167-176
- **Problem:** Two different `HealthCheckResponse` shapes (snake_case in WP-06, camelCase in WP-01). The wire format is inconsistent.
- **Fix:** Adopt WP-01's camelCase (`uptimeMs`, `containerAlive`, `healthCheckFailures`, `lastCheckAt`) and delete the duplicate.

### M4. **No Unit Test Skeleton**
- **Location:** §7 Completeness Checklist, last line
- **Problem:** Same gap WP-01 Iter 1 had (M1). The WP promises tests but ships none. Required tests:
  - State machine: each valid transition + 2 invalid
  - `transitionState` is idempotent on `errored → errored` (B11)
  - `healthCheck` increments on failure, resets on `running` transition (I2/I3)
  - `waitForRequiredPorts` raises `PORT_READINESS_TIMEOUT` after 60s
  - `execClw` propagates `exitCode != 0` as `Error`
  - `onStop` no-ops when state is `stopped`/`errored` (H11)
  - Alarm reschedule logic skips `stopped`/`errored` (B12)
- **Fix:** Add §3.5 with the test skeleton (mirroring WP-01 §3.5).

### M5. **`acquireSnapshotLock` and `releaseSnapshotLock` Not in the State Machine** 
- **Location:** `onStop` line 242-269, `onError` line 284-312
- **Problem:** The lock is `snapshotInProgress: boolean` on the state, but `transitionState` doesn't update it. The state transition `running → stopping` happens BEFORE the lock is acquired (line 238 vs 242), so another RPC (`snapshot()`) can come in during the lock window. The lock only protects against two **concurrent** snapshots, not against the operator-snapshot racing with `onStop`. (Same finding as WP-04, but worth surfacing here.)
- **Fix:** Acquire the lock in `requestStop` (before the state transition), not in `onStop`.

### M6. **`onStart` Doesn't Check the State Machine — Can Be Called Twice**
- **Location:** `onStart()` line 125-172
- **Problem:** If `onStart` is called while already `starting` (e.g. an SDK retry, a second `start()` from Worker), the body runs again — `hydrateViaClw` runs again, `waitForRequiredPorts` runs again, and the second `transitionState("running")` throws `INVALID_STATE_TRANSITION: starting → running` (B11). 
- **Fix:** Add `if (this.state.status !== "starting") throw new Error("onStart called in non-starting state");` at the top.

### M7. **`log()` Method Not Defined**
- **Location:** Lines 104, 118, 127, 137, 141, 166, 194, 202, 236, 245, 249, 263, 279, 286, 297, 303, 310
- **Problem:** WP-06 calls `this.log(...)` 17 times but the method is never declared. WP-04 §3.2 line 332 has a `private log()` but it just emits `console.log` without JSON structure or traceId propagation. WP-01 §3.3 has `logStateTransition()` (line 314) for JSON events, but no general-purpose `log()`. WP-06's `log(message, metadata)` is neither.
- **Fix:** Either consolidate into a single `private log(level: "info"|"warn"|"error", event: string, fields: Record<string, unknown>)` that emits one-line JSON (matching WP-01's `logStateTransition` pattern), or import from a shared `lib/logger.ts`. Document the schema.

### M8. **`fetch()` Override in WP-06 Shadows WP-01's Implementation Without `super.fetch()` Fallback**
- **Location:** `override async fetch(request)` line 373-383
- **Problem:** Same root cause as H5, but the medium-severity angle: the spec removes `/vnc`, `/tty`, `/code` WebSocket routes, `/api/status`, `/api/snapshot`, `/api/resize`, and the `super.fetch(request)` fallback. After WP-06 ships, the Worker ingress (WP-08) gets 404s for every legacy route.
- **Fix:** Don't re-declare `fetch()`. Add the `/_health` branch to WP-01's existing `fetch()` and link WP-06's spec to it.

### M9. **State Persisted to `ctx.storage.put("state", ...)` But Not Documented for SQLite Schema**
- **Location:** `persistState()` line 432, `initializeState()` line 435-462
- **Problem:** WP-01 uses a single `STATE_KEY = "state"` key/value pair. The full `DevenvState` (including the discriminated union's conditional fields) gets serialized to one column. A new field in the union (e.g. `lastProfileSnapshot: SnapshotMetadata | null`) requires a DO migration to be readable from old records. WP-01's wrangler config (`runner_dev_env.ts:678-683`) ships a `v1` migration with `new_sqlite_classes` — that's for class registration, not for state schema. WP-06's persistence needs a `v2` migration with a default for the new fields, or a default-on-read in `initializeState()`.
- **Fix:** WP-06's `initializeState()` (line 435-462) already merges defaults (`healthCheckFailures: stored.healthCheckFailures ?? 0`). Good. But it doesn't merge for `lastProfileSnapshot` / `lastWorkspaceSnapshot` (line 441) — add defaults: `lastProfileSnapshot: stored.lastProfileSnapshot ?? null`.

---

## 📋 DoD Gap Analysis

The DoD has 10 items. Checking each:

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | State machine enforces valid transitions only | ❌ **Fail** | B5, B6, B11, H2 — discriminated union bypassed, throws during errored→errored |
| 2 | `onStart` waits for all 3 required ports (6080, 7681, 8080) | ❌ **Fail** | B7 (bash/Alpine mismatch), B1 (exec doesn't exist), H4 (timeout not honored) |
| 3 | `onStart` transitions: provisioning → starting → port_wait → running | ❌ **Fail** | H1 — first state is `stopped`, not `provisioning`; `port_wait` not in union |
| 4 | `onStop` transitions: running → stopping → stopped | ⚠️ **Partial** | H11 — `onStop` after destroy double-fires |
| 5 | `onError` transitions to errored, attempts emergency snapshot | ⚠️ **Partial** | B1 — snapshot fails because exec doesn't exist; B3 — error type wrong |
| 6 | Health check endpoint returns correct structure | ⚠️ **Partial** | M3 — snake_case vs camelCase; H5 — `fetch()` shadowed |
| 7 | Health check runs every 30s via alarm | ❌ **Fail** | B4 — SDK re-arms to `Date.now()`; H3 — probe always fails |
| 8 | 3 consecutive health failures → errored state | ❌ **Fail** | H2 — state machine throws on 4th; H9 — partial persist loses count on eviction |
| 9 | Port check uses container exec (not external) | ❌ **Fail** | B1 — exec doesn't exist; B13 — argv path wrong; B7 — bash not in Alpine |
| 10 | State persists across DO restarts | ⚠️ **Partial** | M9 — new fields not defaulted on read |

**DoD Score: 0/10 PASS, 3/10 PARTIAL, 7/10 FAIL**

---

## 📋 Invariants Verification

| Invariant | Enforced in Code? | Verdict |
|-----------|-------------------|---------|
| I1: State transitions only follow `VALID_TRANSITIONS` | ❌ `transitionState` throws; B11 leaks through | **NOT ENFORCED** |
| I2: `healthCheckFailures` reset to 0 on transition to "running" | ⚠️ Reset in `transitionState` (line 112) but only because `running` literal is hard-coded; new states will silently break | **PARTIAL** |
| I3: `healthCheckFailures` increments only on failed health check | ❌ Increments on *every* health check failure including transient port flapping | **NOT ENFORCED** |
| I4: 3 consecutive failures → "errored" | ❌ H2 — throws on 4th | **NOT ENFORCED** |
| I5: `onStart` timeout: 60s for ports, 10 min for hydrate | ❌ Hydrate timeout is in WP-04, not enforced in WP-06; port timeout fails to fire (B7, H4) | **NOT ENFORCED** |
| I6: `onStop` and `onError` both call `recordUsage` exactly once | ❌ H10 — no-ops on `startedAt === null`; H11 — double-fires on destroy path | **NOT ENFORCED** |
| I7: Alarm rescheduled only when status ≠ "stopped" and ≠ "errored" | ❌ B4 — SDK re-arms unconditionally; B12 — alarm not cleared on stopped/errored | **NOT ENFORCED** |
| I8: `healthCheckFailures` never exceeds 3 (capped by transition) | ❌ Counter increments indefinitely; only the *transition* triggers errored | **NOT ENFORCED** |

**Invariants Enforced: 0/8 (0%)** — CATASTROPHIC

---

## 📋 Quality Standards Verification

| Standard | Met? | Evidence |
|----------|------|----------|
| State Machine: Explicit transitions with audit log | ❌ Throws in B11; B6 doesn't go through union | **FAIL** |
| Observability: Every transition logged with traceId, old→new, metadata | ⚠️ `transitionState` logs but `onError` skip | **PARTIAL** |
| Health Checks: Active probing (container exec) not passive | ❌ B1 — exec doesn't exist; H3 — probe always fails | **FAIL** |
| Failure Detection: 3 consecutive = hard failure | ❌ H2 | **FAIL** |
| Crash Safety: `onError` fires on ANY container exit | ⚠️ B3 — `Error` type wrong, may throw on non-Error values | **PARTIAL** |
| Idempotency: Health check safe to run concurrently with user traffic | ❌ DO is single-threaded, but `healthCheck` mutates state then `persistState` (H9) | **PARTIAL** |

**Quality Standards: 0/6 MET** — CATASTROPHIC

---

## 📋 Self-Check Points Analysis

### Self-Check 1: State Machine Completeness
- [ ] All 7 states defined: provisioning, starting, port_wait, running, stopping, stopped, errored — ❌ **6 are not in WP-01's union; only `starting`/`running`/`stopping`/`errored`/`stopped` exist**
- [ ] All valid transitions mapped — ❌ **B11 throws; B6 mutates wrong**
- [ ] No direct `stopped → running` — ✅ (must go through starting)
- [ ] `errored` can transition to `provisioning` or `stopped` — ⚠️ **transitions valid in map, but `provisioning` not in union**
- [ ] Invalid transitions throw descriptive error — ❌ **B11 — throws and loses original cause**

**Verdict: 0/5 PASS, 1/5 PARTIAL, 4/5 FAIL**

### Self-Check 2: Health Check Correctness
- [ ] Container liveness: `exec true` — ❌ **B1 — exec doesn't exist; H3 — argv path wrong**
- [ ] Port health: TCP connect to localhost from inside container — ⚠️ **Logic correct; bash not in Alpine (B7); exec doesn't exist (B1)**
- [ ] Not checking external connectivity — ✅
- [ ] 3 consecutive failures threshold — ❌ **H2 — throws on 4th**
- [ ] Health check runs via alarm every 30s when running — ❌ **B4 — SDK re-arms to `Date.now()` immediately**

**Verdict: 1/5 PASS, 1/5 PARTIAL, 3/5 FAIL**

### Self-Check 3: Crash Handling Guarantees
- [ ] `onError` fires on ANY container exit — ⚠️ **Per SDK yes; B3 signature wrong**
- [ ] Attempts snapshot for BOTH profile and workspace — ✅
- [ ] `Promise.allSettled` ensures isolation — ✅
- [ ] `recordUsage` called even on crash — ❌ **H10 — no-ops if `startedAt === null`**
- [ ] State shows `status: "errored"` with `lastError` — ⚠️ **B11 overwrites with throw message**
- [ ] DO remains alive for inspection — ✅ (DO itself never dies)

**Verdict: 2/6 PASS, 2/6 PARTIAL, 2/6 FAIL**

---

## 📊 SCORECARD

| Category | Score | Required | Gap |
|----------|-------|----------|-----|
| Blocking Issues | 14 | 0 | **-14** |
| High Issues | 11 | 0 | **-11** |
| Medium Issues | 9 | 0 | **-9** |
| DoD Pass Rate | 15% | 100% | **-85%** |
| Invariants Enforced | 0% | 100% | **-100%** |
| Quality Standards | 0% | 100% | **-100%** |
| Self-Check Pass | 20% | 100% | **-80%** |

**OVERALL VERDICT: ❌ FAIL — Catastrophic. Spec relies on SDK methods that do not exist (exec, bash in Alpine), fights the SDK's own lifecycle (alarm re-arm), and breaks WP-01's discriminated union contract. Requires full rewrite before sign-off.**

---

## 🔧 FIX PRIORITY

### Must Fix (Blockers)
1. **B1, B13, B7** — Decide on the exec transport (in-container exec-server vs SDK's `startAndWaitForPorts`). This unblocks 5+ other issues.
2. **B2, B3** — Fix `onStop(params: StopParams)` and `onError(error: unknown)` signatures.
3. **B4, B12** — Replace `setAlarm` with `this.schedule()`.
4. **B5, B6, H1, M1** — Reconcile state machine with WP-01's discriminated union (add variants or fold them).
5. **B8, B10, M2, M3** — Import from `src/types/devenv.ts`; remove duplicates.
6. **B11, H2, H6** — Make `transitionState` idempotent on same-state transitions.
7. **B14, H10** — Replace WP-04's `recordUsage` with WP-07's canonical billing path.
8. **B9, H7** — Bound wall-clock waits so the DO input gate isn't held for 10 min.

### Should Fix (High)
1. H3, H4 — Replace bash port check + clw `exec true` probe with SDK + HTTP exec-server.
2. H5, M8 — Don't re-declare `fetch()`; add `/_health` to WP-01's existing fetch.
3. H8 — Distinguish container-down from port-down.
4. H9 — Persist health-check state incrementally.
5. H11 — `onStop` no-ops on terminal states.

### Nice to Fix (Medium)
1. M4 — Add unit test skeleton.
2. M5 — Acquire snapshot lock in `requestStop`, not in `onStop`.
3. M6 — Guard `onStart` against re-entry.
4. M7 — Define `log()` once.
5. M9 — Default new persisted fields on read.

---

## 🔗 CROSS-WP COORDINATION NEEDS

| WP | Coordination Need |
|----|-------------------|
| **WP-01** | `DevenvState` discriminated union must be extended with `provisioning` + `port_wait` (or those must be folded out). `onStop(params)` / `onError(error: unknown)` signatures must be honored. `fetch()` is owned by WP-01 — WP-06 must NOT override it. `envVars` must be persisted to `this` (or to `this.ctx.storage` keyed by `envVars`) so lifecycle hooks can read `PROFILE_NAME` / `WORKSPACE_NAME`. |
| **WP-02** | Base image must include either `bash` (for the bash port check) or an exec-server (B1 fix path). If bash is shipped, document the apk add. If exec-server, expose port 8080 `POST /clw {argv}` and `POST /ping`. |
| **WP-03** | `supervisord` must NOT race with `clw hydrate` (the current WP-06 sequence is: mkdir → hydrate profile → hydrate workspace → wait for ports — but supervisord starts noVNC, ttyd, code-server immediately, so `waitForRequiredPorts` may succeed *before* hydrate completes). Fix: pause supervisord until `clw` signals "ready", OR have `onStart` `startAndWaitForPorts` AFTER hydrate. |
| **WP-04** | `execClw` / `hydrateViaClw` / `snapshotViaClw` / `acquireSnapshotLock` / `releaseSnapshotLock` / `recordUsage` must be either imported by WP-06 (and re-declared nowhere) or inlined into WP-06 (and removed from WP-04). Pick one. `recordUsage` body is REJECTED by WP-07 — must use canonical `/internal/v1/billing/usage`. |
| **WP-05** | `state.wsConnections` must be incremented on `acceptWebSocket` and decremented on `webSocketClose` / `webSocketError` (Hibernation runtime handlers). Field name must match exactly. |
| **WP-07** | Billing path is canonical. WP-06's `recordUsage` calls go through `env.BILLING_DO.emit({kind:"devenv_vcpu_seconds", ...})`, not a custom `fetch`. Idempotency key: `BLAKE3(tenant|container|startedAt)`. |
| **WP-08** | Worker ingress must call `RUNNER_DEVENV_DO` (idFromName(tenantId)) and the new `/_health` endpoint is reachable at `do.idFromName(tenantId).fetch(new Request("https://do/_health"))`. Auth on `/_health`: a wrangler secret bearer, NOT public. |

---

## NEXT STEPS

1. **Apply all BLOCKING fixes (B1-B14)** — non-negotiable
2. **Apply all HIGH fixes (H1-H11)** — required for quality
3. **Apply at least 50% of MEDIUM fixes (M1-M9 minimum)** — required for completeness
4. **Reconcile with WP-01, WP-02, WP-03, WP-04, WP-05, WP-07, WP-08** before re-review
5. **Re-verify all 10 DoD items pass**
6. **Re-verify all 8 invariants enforced in code**
7. **Re-verify all 6 quality standards met**
8. **Proceed to Iteration 2 review**

**Do NOT proceed to WP-07 (Billing) review until WP-06 is fixed and passes review. WP-07 already cites WP-04's `recordUsage` as REJECTED; WP-06's lifecycle is where that pattern lives.**

---

**END OF WP-06 ITERATION 1 REVIEW**
