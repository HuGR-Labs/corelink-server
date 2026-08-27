# WP-01 REVIEW — Iteration 1 (CRITICAL AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Verdict:** ❌ **FAIL — 11 BLOCKING ISSUES, 8 HIGH SEVERITY ISSUES, 6 MEDIUM SEVERITY ISSUES**

---

## 🔴 BLOCKING ISSUES (Must fix before sign-off)

### B1. **Self-Referential Recursive Call — Infinite Loop**
- **Location:** `start()` method, line 185
- **Code:** `await this.start({ envVars: this.envVars, enableInternet: true });`
- **Problem:** The RPC method `start()` calls itself recursively. The Container class parent method is `this.start()` (inherited), but this code is INSIDE the `start()` RPC method — it calls itself.
- **Real API:** `super.start()` or `this.container.start()`. Looking at `corelink-runners/deploy/cloudflare/src/index.ts:605`, `CheckHostContainer extends Container<Env>` uses `super.start({...})` or invokes via `this.start(...)` ONLY at the top-level handler — not recursively.
- **Fix:** Remove the `await this.start(...)` line. Container auto-starts when `envVars` are set and `requiredPorts` is configured. The start happens via the spawn flow, not in the RPC handler. The RPC handler should only update state, set envVars, and call `super.start({ envVars, enableInternet })` exactly once.

### B2. **`this.container.stop()` Does Not Exist**
- **Location:** `stop()` method, line 205
- **Code:** `await this.container.stop();`
- **Problem:** `this.container` is the low-level Durable Object Container binding (available at `this.ctx.container` in Cloudflare Containers SDK). It does NOT have a `stop()` method. The correct way to stop a Container is `this.stop()` (inherited from `Container` class) which calls `signal(SIGTERM)` and triggers `onStop()` lifecycle hook.
- **Real API:** Looking at `@cloudflare/containers`, the `Container` class has `stop(signal = SIGTERM): Promise<void>` as a public method.
- **Fix:** Replace `await this.container.stop();` with `await this.stop();` (no await needed actually, it's fire-and-forget but await is fine).

### B3. **`this.container` Reference is Wrong**
- **Location:** `stop()` method, line 205
- **Problem:** In the `Container` class, `this.container` doesn't exist. The low-level container binding is `this.ctx.container`. The `Container` class itself has `stop()`, `destroy()`, `monitor()`, `getTcpPort()` as instance methods.
- **Fix:** Remove `this.container.stop()`. Use `this.stop()` directly.

### B4. **Missing `exec()` Method Documentation**
- **Location:** Throughout (referenced in WP-04, WP-05, WP-06)
- **Problem:** WP-01 mentions `this.container.exec()` (line in WP-04/06) and `this.execClw()` but the `exec()` method is provided by the Container class via `this.ctx.container.exec({ cmd, timeoutMs, stdout, stderr })`. The skeleton doesn't document the `exec()` API that downstream WPs depend on.
- **Fix:** Add a section documenting the `exec()` method signature and usage pattern from `@cloudflare/containers` SDK. Specifically:
  ```typescript
  const execProcess = await this.ctx.container.exec({
    cmd: ["bash", "-c", "command"],
    timeoutMs: 5000,
    stdout: "pipe",  // or "ignore"
    stderr: "pipe",
  });
  const output = await execProcess.output();
  // output: { stdout: ArrayBuffer, stderr: ArrayBuffer, exitCode: number }
  ```

### B5. **`proxyWebSocket()` Signature is Incomplete**
- **Location:** `proxyWebSocket(request, port)` method, line 233
- **Problem:** The method signature doesn't specify what it returns or how it handles hibernation. Downstream WP-05 needs to implement this, but the current stub throws `NOT_IMPLEMENTED` with no guidance on the expected return type.
- **Real API:** The method must:
  1. Validate `Upgrade: websocket` header
  2. Create `WebSocketPair`
  3. Call `this.ctx.acceptWebSocket(server)` for hibernation
  4. Forward to container via `super.fetch()` (which proxies HTTP/WS to container)
  5. Set up bidirectional piping
  6. Return `new Response(null, { status: 101, webSocket: client })`
- **Fix:** Add detailed doc comment with the full implementation pattern, or implement the full method now (since it's well-defined).

### B6. **`fetch()` Handler Doesn't Route to Container**
- **Location:** `fetch()` method, line 237
- **Problem:** The `fetch()` method routes WebSocket upgrades to `proxyWebSocket()` and returns 404 for everything else. But the Container class's `fetch()` is designed to proxy ALL HTTP requests to the container. The skeleton should delegate to `super.fetch()` as fallback.
- **Fix:** Add `return super.fetch(request);` as the final fallback in `fetch()`.

### B7. **`initializeState()` Runs Sync I/O in Constructor**
- **Location:** Constructor, line 129-132
- **Problem:** `this.ctx.storage.get()` is synchronous (it returns immediately from in-memory cache), so this is OK. But the comment says "Restore from SQLite or initialize fresh" — `ctx.storage.get` reads from SQLite-backed DO storage which is synchronous. However, the WP-06 alarm-based approach uses `ctx.storage.setAlarm()` which IS async. The constructor pattern should explicitly note this is sync.
- **Fix:** Add doc comment: "Sync read from DO storage (in-memory cache; first read hydrates from SQLite)."

### B8. **No `Debug` Implementation for `RunnerDevEnvDO`**
- **Location:** Class definition (missing)
- **Problem:** Quality Standards say "envVars redacted in toString()/Debug" but no `Debug` impl is provided. The `Container` parent class may not redact.
- **Fix:** Add `debug()` method that redacts `CLW_TOKEN` and other secrets.

### B9. **`requiredPorts` is `readonly [6080, 7681, 8080] as const` but `envVars` is `Record<string, string>`**
- **Location:** Class config, lines 111 and 114
- **Problem:** Mixed mutability. `requiredPorts` is `readonly` (immutable) but `envVars` is NOT readonly (it MUST be mutable to inject tenant/token at runtime). The invariant `I8: envVars.CLW_REF_DOMAIN always "runner"` requires it to be immutable.
- **Fix:** Split `envVars` into two:
  ```typescript
  private readonly staticEnvVars: Record<string, string> = {
    CLW_REF_DOMAIN: "runner",
    CLW_ENDPOINT: "https://corelink-api.humangr.com",
  };
  private mutableEnvVars: Record<string, string> = {
    CLW_TENANT: "",
    CLW_TOKEN: "",
    WORKSPACE_NAME: "",
    PROFILE_NAME: "browser-profile",
  };
  // At spawn time: merge into this.envVars
  ```

### B10. **`DevEnvState` Interface Uses Non-Discriminated Union for Status**
- **Location:** Line 63
- **Problem:** `status: "starting" | "running" | ...` is a union but `startedAt`, `containerHandle` are nullable, making the type weak. A discriminated union would make invalid states unrepresentable.
- **Fix:** Use branded types or tagged unions:
  ```typescript
  type DevEnvState = 
    | { status: "stopped"; /* no other fields */ }
    | { status: "starting"; workspaceName: string; profileName: string; startedAt: number }
    | { status: "running"; workspaceName: string; profileName: string; startedAt: number; containerHandle: string }
    | { status: "stopping"; workspaceName: string; profileName: string; startedAt: number }
    | { status: "errored"; lastError: string; lastWorkspaceName: string };
  ```

### B11. **`fetch()` Returns Plain Text for 404 — Inconsistent**
- **Location:** Line 252
- **Problem:** Returns `new Response("Not Found", { status: 404 })` without `Content-Type: application/json`. The rest of the API returns JSON. Inconsistent.
- **Fix:** `return new Response(JSON.stringify({ error: "Not Found" }), { status: 404, headers: { "Content-Type": "application/json" } });`

---

## 🟠 HIGH SEVERITY ISSUES

### H1. **No `schedule()` Method Despite WP-06 Requiring It**
- **Location:** Missing
- **Problem:** WP-06 requires `this.ctx.storage.setAlarm(Date.now() + 30_000)` for periodic health checks. The skeleton doesn't document or scaffold this.
- **Fix:** Add `scheduleNextHealthCheck()` helper method and document `alarm()` override pattern.

### H2. **No `noteActivity()` Pattern**
- **Location:** Missing
- **Problem:** `corelink-runners/deploy/cloudflare/src/index.ts:454` uses `this.renewActivityTimeout()` and `this.noteActivity()` (a custom pattern) to keep the idle window open. The skeleton should document or scaffold this.
- **Fix:** Add `noteActivity()` method that writes to `ctx.storage.put("lastActivityAt", Date.now())`.

### H3. **`stop()` Doesn't Check for State Transitions**
- **Location:** `stop()` method, line 197
- **Problem:** The method returns early if `status === "stopped" || "stopping"` but doesn't validate other states. If called when `status === "errored"`, it proceeds without consideration.
- **Fix:** Define explicit state machine transitions (see B10). `stop()` should only work from `running` or `starting` states.

### H4. **`start()` Doesn't Validate `clwToken` Format**
- **Location:** `start()` method, line 163
- **Problem:** Only checks `!clwToken` (truthy), not format. PATs have a specific format (e.g., `cl_...` prefix in corelink).
- **Fix:** Add format validation: `/^cl_[a-zA-Z0-9_]{32,}$/.test(payload.config.clwToken)`.

### H5. **`workspaceName` Not Validated**
- **Location:** `start()` method
- **Problem:** No validation that `workspaceName` matches clw naming rules (alphanumeric, underscore, hyphen, max 128 chars).
- **Fix:** Use `clw_types::validate_workspace_name()` equivalent (we should have a shared validator).

### H6. **Missing `Context` Type for `Env`**
- **Location:** Line 129
- **Problem:** The `Env` type is referenced but not defined. The skeleton assumes it exists in scope.
- **Fix:** Add `Env` interface import from `worker-configuration.d.ts` (auto-generated by wrangler).

### H7. **No Metrics/Observability**
- **Location:** Class (missing)
- **Problem:** No metrics for cold start time, warm resume time, snapshot size, etc. The campaign plan calls out `MetricsDO` but the skeleton doesn't integrate.
- **Fix:** Add `bumpMetric(name, value)` calls in lifecycle hooks.

### H8. **No `RPC` Export for Type Safety**
- **Location:** Class (missing)
- **Problem:** The class uses async methods as RPC, but `@cloudflare/containers` and DO RPC require explicit `extends RpcTarget` or similar. The pattern is to define a separate `interface RunnerDevEnvRPC` for type safety.
- **Fix:** Define `export interface RunnerDevEnvRPC { start(payload): Promise<...>; ... }` and have the class implement it explicitly.

---

## 🟡 MEDIUM SEVERITY ISSUES

### M1. **No Unit Test Skeleton with Concrete Examples**
- **Location:** Test file (missing content)
- **Problem:** The WP says "Unit test skeleton exists and compiles" but doesn't provide the skeleton.
- **Fix:** Provide actual test file content with at least 3-5 test cases:
  - Test: class compiles
  - Test: state machine transitions
  - Test: envVars immutability
  - Test: config validation
  - Test: WebSocket proxy signature

### M2. **No Documentation of `onStart`/`onStop`/`onError` Semantics**
- **Location:** Stubs at lines 270-279
- **Problem:** Just throws `NOT_IMPLEMENTED`. No documentation of when they fire, what they should do, what they MUST NOT do.
- **Fix:** Add JSDoc with:
  - `onStart`: Called once when container transitions to "running". MUST hydrate profile+workspace, set up state, NOT throw (or container is marked errored).
  - `onStop`: Called when container is being stopped (graceful). MUST snapshot before container dies.
  - `onError`: Called when container crashes/errors. MUST attempt emergency snapshot.

### M3. **No Documentation of `Container` Class Parent Methods**
- **Location:** Missing
- **Problem:** WP-01 inherits from Container but doesn't document which parent methods are available (`start`, `stop`, `fetch`, `exec`, `monitor`, `signal`, `destroy`, `getTcpPort`, `interceptOutboundHttp`, etc.)
- **Fix:** Add a section "Inherited from Container" listing all available methods with brief descriptions.

### M4. **`StatusResponse.ports` Type Mismatch**
- **Location:** Line 82
- **Problem:** `readonly [6080, 7681, 8080]` is a tuple type but `buildStatusResponse()` returns `[6080, 7681, 8080]` as a regular array. TypeScript will infer the regular array as `number[]` not the tuple. Will fail strict type check.
- **Fix:** `return { ..., ports: [6080, 7681, 8080] as const, ... };` or change `StatusResponse.ports` to `readonly number[]`.

### M5. **No Integration with `MetricsDO`**
- **Location:** Class (missing)
- **Problem:** Campaign plan mentions `MetricsDO` for cold start tracking but no integration.
- **Fix:** Add `private bumpMetric(name: string, value: number)` method that posts to `env.METRICS_DO`.

### M6. **No `defaultAllowedHosts` / `defaultDeniedHosts`**
- **Location:** Missing
- **Problem:** Container class supports `allowedHosts` and `deniedHosts` for egress filtering. The skeleton doesn't set these, so the container can reach any host. For security, should restrict to CAS/AC endpoints only.
- **Fix:** Add `allowedHosts = ["corelink-api.humangr.com", "*.cloudflarestorage.com"]` or similar.

---

## 📋 DOd Gap Analysis

The DoD has 10 items. Checking each:

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | Class compiles | ❓ **Cannot verify** | Has BLOCKING issues (B1, B2, B3) that will cause compile errors |
| 2 | Extends Container | ✅ Pass | Line 101: `extends Container` |
| 3 | Config properties defined | ⚠️ **Partial** | Present but mixed mutability (B9) |
| 4 | RPC methods defined | ⚠️ **Partial** | Defined but `start()` has recursive call (B1) |
| 5 | WebSocket proxy signature | ❌ **Fail** | Signature incomplete (B5) |
| 6 | Wrangler config | ✅ Pass | Lines 286-310 |
| 7 | Unit test skeleton | ❌ **Fail** | Not provided (M1) |
| 8 | No `any` types | ❌ **Fail** | Line 248: `(request as any).tenantId` would be needed, but currently code doesn't have this. Actually code is OK. **RECHECK** |
| 9 | NOT_IMPLEMENTED are Error instances | ✅ Pass | All are `throw new Error(...)` |
| 10 | State persistence correct | ✅ Pass | Uses `ctx.storage.put/get` |

**DoD Score: 5/10 PASS, 3/10 PARTIAL, 2/10 FAIL**

---

## 📋 Invariants Verification

| Invariant | Enforced in Code? | Verdict |
|-----------|-------------------|---------|
| I1: status always one of 5 | ❌ No, type is union but not validated at runtime | **NOT ENFORCED** |
| I2: workspaceName non-empty iff running | ❌ No check | **NOT ENFORCED** |
| I3: containerHandle non-null iff running | ❌ No check | **NOT ENFORCED** |
| I4: envVars.CLW_TENANT/TOKEN set before start returns | ⚠️ Set in start() but not validated | **PARTIAL** |
| I5: requiredPorts = [6080, 7681, 8080] | ✅ Literal | **ENFORCED** |
| I6: sleepAfter parseable | ✅ Literal "30m" | **ENFORCED** |
| I7: defaultPort = requiredPorts[0] | ✅ Both 6080 | **ENFORCED** |
| I8: CLW_REF_DOMAIN always "runner" | ⚠️ In Record<string,string> so mutable | **NOT ENFORCED** |

**Invariants Enforced: 4/8 (50%)** — INSUFFICIENT

---

## 📋 Quality Standards Verification

| Standard | Met? | Evidence |
|----------|------|----------|
| Type Safety: Zero `any` in public API | ✅ Pass | No `any` in class |
| Error Handling: typed or Error with message | ⚠️ Partial | Some are generic `Error` without specific subtypes |
| Immutability: readonly, replacement not mutation | ❌ Fail | envVars is mutable Record (B9) |
| Observability: console.log structured | ❌ Fail | No logging in skeleton |
| Security: CLW_TOKEN redacted | ❌ Fail | No Debug impl, no redaction (B8) |
| Performance: no blocking I/O in constructor | ✅ Pass | `ctx.storage.get` is sync |

**Quality Standards: 3/6 MET** — INSUFFICIENT

---

## 📋 Self-Check Points Analysis

The WP has 3 self-check points. Checking each:

### Self-Check 1: SOTA Compliance
- [x] Extends Container — ✅
- [x] Uses sleepAfter, requiredPorts, defaultPort — ✅
- [x] Uses this.ctx.container for low-level ops — ❌ **Code uses this.container.stop() (wrong)**
- [x] Hibernation WebSocket API — ⚠️ Mentioned in self-check but not implemented in proxyWebSocket

**Verdict:** 2/4 PASS, 1/4 FAIL, 1/4 PARTIAL

### Self-Check 2: Zero Ambiguity
- [x] Method signatures have types — ✅
- [x] Config values literal — ✅
- [x] Error types specified — ✅
- [x] State machine transitions explicit — ❌ **No state machine defined in WP-01 (only in WP-06)**

**Verdict:** 3/4 PASS, 1/4 FAIL

### Self-Check 3: On Track for Dependencies
- [x] envVars matches entrypoint.sh — ✅
- [x] requiredPorts matches supervisord — ✅
- [x] proxyWebSocket signature enables WP-05 — ⚠️ **Signature too vague**
- [x] onStart/onStop stubs enable WP-06 — ✅
- [x] DevEnvConfig type shared with WP-04 — ✅

**Verdict:** 4/5 PASS, 1/5 PARTIAL

---

## 📊 SCORECARD

| Category | Score | Required | Gap |
|----------|-------|----------|-----|
| Blocking Issues | 11 | 0 | **-11** |
| High Issues | 8 | 0 | **-8** |
| Medium Issues | 6 | 0 | **-6** |
| DoD Pass Rate | 50% | 100% | **-50%** |
| Invariants Enforced | 50% | 100% | **-50%** |
| Quality Standards | 50% | 100% | **-50%** |
| Self-Check Pass | 75% | 100% | **-25%** |

**OVERALL VERDICT: ❌ FAIL — Requires major rework before sign-off**

---

## 🔧 FIX PRIORITY

### Must Fix (Blockers)
1. B1, B2, B3 — Fix recursive call and wrong container references
2. B5 — Implement proxyWebSocket or provide full doc
3. B10 — Use discriminated union for state
4. B8, B9 — Add Debug impl, split envVars

### Should Fix (High)
1. H1, H2 — Add schedule() and noteActivity() patterns
2. H3 — State machine validation
3. H4, H5 — Input validation
4. H6, H8 — Env type, RPC interface

### Nice to Fix (Medium)
1. M1 — Provide unit test skeleton
2. M2 — JSDoc for lifecycle hooks
3. M3 — Document inherited methods
4. M6 — Add allowedHosts/deniedHosts

---

## NEXT STEPS

1. **Apply all BLOCKING fixes** (B1-B11) — non-negotiable
2. **Apply all HIGH fixes** (H1-H8) — required for quality
3. **Apply at least 50% of MEDIUM fixes** (M1-M3 minimum)
4. **Re-verify all 10 DoD items pass**
5. **Re-verify all 8 invariants enforced in code**
6. **Re-verify all 6 quality standards met**
7. **Proceed to Iteration 2 review**

**Do NOT proceed to WP-02 until WP-01 is fixed and passes review.**

---

**END OF WP-01 ITERATION 1 REVIEW**