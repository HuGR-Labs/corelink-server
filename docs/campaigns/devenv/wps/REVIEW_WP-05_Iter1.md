# WP-05 REVIEW — Iteration 1 (CRITICAL AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Verdict:** ❌ **FAIL — 14 BLOCKING ISSUES, 11 HIGH SEVERITY ISSUES, 9 MEDIUM SEVERITY ISSUES**

---

## 🔴 BLOCKING ISSUES (Must fix before sign-off)

### B1. **Hibernation Message-Pipe is BROKEN — Messages Lost After Resume**
- **Location:** `webSocketMessage()` handler, line 307-313; `pipeWebSockets()`, line 135-172
- **Problem:** WP says "Messages are piped automatically via pipeWebSockets" but the runtime invokes `webSocketMessage()` for hibernated WebSockets — the `addEventListener("message", ...)` registered in `pipeWebSockets` does NOT fire for hibernated WS (Hibernation API requires handlers be the runtime methods, per `worker-configuration.d.ts:628`). After DO eviction + resume, the hibernated `server` WS receives messages via the runtime `webSocketMessage` hook, which is a no-op. Result: **all client→container messages are silently dropped on resume**.
- **Real pattern:** Inside `webSocketMessage(ws, message)`, look up the paired `containerWs` (via tags / a Map keyed by `ws.deserializeAttachment()`) and forward.
- **Fix:** Remove `addEventListener("message", ...)` from `pipeWebSockets`. Implement forwarding INSIDE `webSocketMessage` runtime handler. Persist `containerWs → clientWs` pairing via `ws.serializeAttachment({ containerWsId })` + a Map.

### B2. **Duplicate `webSocketMessage`/`Close`/`Error` Handlers Fire → Messages Sent TWICE**
- **Location:** `pipeWebSockets()`, line 137-156; runtime handlers, line 307-323
- **Problem:** `pipeWebSockets` registers `addEventListener("message", ...)` on the hibernated `server` WS. The runtime also calls the `webSocketMessage` runtime method on the same WS. Both fire → message forwarded to container twice → terminal echoes keystrokes, noVNC RFB corrupts.
- **Fix:** Use ONLY the runtime handler. Use `ws.deserializeAttachment()` to recover the paired container WS, and a non-hibernated in-memory Map for `containerWs → clientWs` (the container WS is NOT hibernated — it's a regular client of the DO's outbound container Fetcher).

### B3. **`webSocketError` Type Mismatch — Will Fail Type-Check**
- **Location:** Line 321
- **Code:** `async webSocketError(ws: WebSocket, error: Error): Promise<void>`
- **Problem:** Per `worker-configuration.d.ts:630`, signature is `webSocketError?(ws: WebSocket, error: unknown): void | Promise<void>`. Parameter is `unknown`, not `Error`. Spec mismatch → TypeScript error.
- **Fix:** `async webSocketError(ws: WebSocket, error: unknown): Promise<void>` then `error instanceof Error ? error.message : String(error)`.

### B4. **No `acceptWebSocket` Tags → Cannot Pair WS Across Hibernation**
- **Location:** Line 80, 399 — `this.ctx.acceptWebSocket(server)` with no tags
- **Problem:** After hibernation + resume, the runtime invokes `webSocketMessage(ws, message)` with the `ws` — but the DO has no idea which `containerWs` to forward to. Tags (`[`port-6080`, connId]`) or `ws.serializeAttachment({ connId })` are required to recover the pairing.
- **Fix:** Tag with port + connection ID: `this.ctx.acceptWebSocket(server, [\`port-${port}\`, \`conn-${connId}\`])`. Persist `connId → { containerWs, clientWs }` in a non-hibernated `Map`. On `webSocketMessage`/`webSocketClose`/`webSocketError`, recover from `ws.deserializeAttachment()`.

### B5. **`wsConnections` Reset to 0 on DO Wakeup → Invariant Violated After Resume**
- **Location:** `initializeState()`, line 281
- **Code:** `wsConnections: 0, // Always start at 0 on DO activation`
- **Problem:** When DO is evicted and resumed, the hibernated WS set is restored by the runtime (`ctx.getWebSockets()` returns the live set). But the spec resets the count to 0 → drift. Capacity / billing / health checks undercount active sessions. Invariant I1 (`wsConnections ≥ 0`, accurate) is violated.
- **Fix:** `wsConnections: this.ctx.getWebSockets().length` on activation. Use `getWebSockets(tag)` for tagged accounting.

### B6. **`super.fetch()` Does Not Route to Requested Port — Goes to `defaultPort`**
- **Location:** "Correction" block, line 413
- **Code:** `const containerResponse = await super.fetch(containerRequest);`
- **Problem:** The high-level `Container` class (from `cloudflare:workers`, which the WP imports) overrides `fetch()` to proxy requests to `defaultPort` (6080) — NOT to the port specified by the request. The `super.fetch()` call routes `/tty` and `/code` upgrades to the same `defaultPort` (noVNC), which doesn't speak ttyd/code-server protocols. All three endpoints become noVNC.
- **Real API:** `this.ctx.container.getTcpPort(port).fetch(request)` is the correct way to target a specific port. The high-level `Container.fetch()` is a convenience over `defaultPort` only.
- **Fix:** `const fetcher = this.ctx.container.getTcpPort(port); const containerResponse = await fetcher.fetch(containerRequest);`. This also matches the pattern at `worker/src/durable_object.ts:408`.

### B7. **Two `containerFetch` Definitions — TypeScript Redeclaration Error**
- **Location:** Section 3.3, lines 349-353 AND 359-378
- **Problem:** Two methods named `containerFetch` in the same class. First one is plain, second tries to use `container.getTcpPort(8080)` (HARDCODED to 8080 regardless of requested port). Both would appear in the spec simultaneously → compilation failure.
- **Fix:** Delete BOTH ad-hoc definitions. The `proxyWebSocket` "Correction" block calls `super.fetch()` directly (which has its own B6 problem). Single, correct implementation: `private async containerFetch(request: Request, port: number) { return this.ctx.container.getTcpPort(port).fetch(request); }`.

### B8. **Two Complete `proxyWebSocket` Implementations in Spec — Conflicting**
- **Location:** First version, lines 67-133 AND "Correction" version, lines 387-431
- **Problem:** Two full method bodies for `proxyWebSocket`. A copy-paste-driven merge would land both in the class → TypeScript redeclaration error. Worse, they CONFLICT: first version uses `this.containerFetch` (broken, B7), second uses `super.fetch` (broken, B6).
- **Fix:** Single canonical implementation using `this.ctx.container.getTcpPort(port).fetch(request)`. Mark first version as **DEPRECATED — REMOVE**.

### B9. **`Object.values(webSocketPair)` Typed as `unknown[]` — Compilation Fails Strict**
- **Location:** Line 76 and 396
- **Code:** `const [client, server] = Object.values(webSocketPair);`
- **Problem:** `Object.values()` returns `unknown[]` in strict mode. Tuple destructuring into `WebSocket`-typed variables fails. WP-01 fixed this with `as [WebSocket, WebSocket]` cast (line 457). WP-05 missing.
- **Fix:** `const [client, server] = Object.values(webSocketPair) as [WebSocket, WebSocket];`.

### B10. **`this.log()` Called But Not Defined — Runtime TypeError**
- **Location:** Lines 124, 143, 154, 167, 188, 318, 322
- **Problem:** `this.log(...)` is invoked 7 times. No `log()` method exists on the class. `console.log` is the base. Calling `this.log` resolves to the `log` property (undefined for a fresh class) → `TypeError: this.log is not a function`. Every WS error path crashes the handler.
- **Fix:** Add `private log(level: "info" | "warn" | "error", event: string, fields: Record<string, unknown>): void { console.log(JSON.stringify({ level, event, devenv_id: this.ctx.id.toString(), timestamp: Date.now(), ...fields })); }` mirroring WP-01's `logStateTransition` pattern.

### B11. **Resize via printf to `/dev/pts/0` Doesn't Work — Wrong IPC Path**
- **Location:** `resize()`, lines 257-260
- **Code:** `printf '\\033[8;${height};${width}t' > /dev/pts/0 2>/dev/null || true`
- **Problem:** (a) The escape sequence for setting terminal size is `\033[8;rows;cols t` but writing it to a pty doesn't resize the pty — `ioctl(TIOCSWINSZ)` does. `printf` to the pty master just sends data through it. (b) `clw exec` runs in a NEW shell, not the user's ttyd-allocated `/dev/pts/0`. (c) Different processes (ttyd, noVNC, code-server) handle resize differently — ttyd expects a JSON WS message `{columns, rows}`, noVNC handles RFB `SetDesktopSize` client-side, code-server uses VS Code's own resize message. The spec sends ONE generic shell command to ALL three.
- **Fix:** Resize must be propagated via the WebSocket: ttyd expects `{"columns": N, "rows": M}` JSON on its WS. Send a message on the open `/tty` WS (via a per-port WS map). For noVNC and code-server, the browser client handles resize via its own protocol. Either drop the server-side resize for vnc/code OR forward to the container over an RPC and let the client adjust.

### B12. **Resize API Ignores Request Body — Uses Hardcoded Dimensions**
- **Location:** `fetch()` `/api/resize` route, line 219-222
- **Code:** `const body = await request.json(); return new Response(JSON.stringify(await this.resize(body)));`
- **Problem:** Wait — actually this looks right (passes `body`). BUT the `resize()` method then ignores its `payload` argument and doesn't use `width`/`height` (it does extract them at line 246 — actually OK). Re-verify: `payload.width`/`payload.height` are used. The real bug is that `resize()` does the wrong thing (B11). Mark as **related to B11**; not separate.
- **Verdict:** Roll into B11 fix.

### B13. **`alarm()` Override Missing `alarmInfo` Parameter — Type Mismatch**
- **Location:** Line 335
- **Code:** `override async alarm(): Promise<void>`
- **Problem:** Parent signature per `worker-configuration.d.ts:627` is `alarm?(alarmInfo?: AlarmInvocationInfo): void | Promise<void>`. Missing parameter → TypeScript error in strict mode.
- **Fix:** `override async alarm(alarmInfo?: AlarmInvocationInfo): Promise<void>`.

### B14. **State Machine Mismatch — WP-05 vs WP-01 vs WP-06**
- **Location:** Section 3.2, line 58-61 (`DevEnvState` interface)
- **Problem:** WP-05 defines its OWN `DevEnvState` interface with `status: "stopped" | "starting" | "running" | "stopping" | "errored" | undefined` (line 59 implicit) — a different shape than WP-01's discriminated union (`src/types/devenv.ts`). Also conflicts with WP-06's 7-status machine (`provisioning/starting/port_wait/running/stopping/stopped/errored`). A class extending both is impossible without a redesign. Three WPs, three state machines.
- **Fix:** Import the canonical `DevenvState` from `../types/devenv` (WP-01). Update DoD/invariants to reference the discriminated union. Coordinate with WP-06 — the WS count must work in tandem with the port-wait state.

---

## 🟠 HIGH SEVERITY ISSUES

### H1. **`this.persistState()` Called Without Await — Fire-and-Forget Bug**
- **Location:** Lines 84, 110, 181, 264, 310, 338, 401
- **Problem:** `ctx.storage.put` returns `Promise<void>`. Calling `this.persistState()` synchronously (the method itself doesn't return the promise) means the write is lost if the isolate dies within the next ms. WS connect/disconnect accounting can drift.
- **Fix:** `await this.persistState()` everywhere, OR change `persistState` to `private persistState(): Promise<void> { return this.ctx.storage.put(STATE_KEY, this.state); }` and ALWAYS await at call sites.

### H2. **`addEventListener` on Hibernated WS — Undefined Behavior**
- **Location:** `pipeWebSockets()`, line 137-156; `proxyWebSocket` close/error listeners, line 119-126
- **Problem:** Per Cloudflare docs, hibernated WebSockets (those passed to `ctx.acceptWebSocket`) deliver events ONLY via the `webSocketMessage`/`Close`/`Error` runtime hooks. `addEventListener` works on NON-hibernated WS. Using both → undefined behavior (potentially double-firing, potentially missing events after hibernation).
- **Fix:** All `addEventListener` on the hibernated `server` WS removed (see B1, B2). For the non-hibernated `containerWs`, `addEventListener` is the correct API.

### H3. **No `noteActivity()` in `proxyWebSocket` Path — Activity Clock Stops**
- **Location:** `proxyWebSocket()`, line 67-133
- **Problem:** WP-01's `proxyWebSocket` calls `this.noteActivity()` after WS accept. WP-05's version does NOT. Result: container stays warm per `sleepAfter` (in-memory timer) but `lastActivityAt` storage key never updates → idle reaper (WP-06 alarm) destroys the container while WS are live.
- **Fix:** `this.noteActivity()` immediately after `acceptWebSocket` and inside `webSocketMessage` handler.

### H4. **No `request.signal` / `AbortController` for Slow Container Connect**
- **Location:** `proxyWebSocket()`, line 102 — `await this.containerFetch(containerRequest)`
- **Problem:** If container is slow to start or hang, this `await` blocks the DO I/O loop. Client disconnect during the await goes unnoticed. Resource leak (DO holds open fetcher with no timeout).
- **Fix:** `const ac = new AbortController(); request.signal.addEventListener("abort", () => ac.abort()); const containerResponse = await this.ctx.container.getTcpPort(port).fetch(containerRequest, { signal: ac.signal });`. Or wrap with `Promise.race` against a timeout.

### H5. **`/api/resize` Hardcodes 1920×1080**
- **Location:** NOT IN WP-05 file — this was in WP-01, not WP-05. **RECHECK**: line 220-222 of WP-05 actually does pass `body` correctly. **Withdraw**. Move to cross-WP note.

### H6. **`/api/resize` POST Without Body Validation**
- **Location:** `fetch()` `/api/resize` route, line 219-222
- **Problem:** Reads `request.json()` without try/catch — malformed JSON returns 500 instead of 400. No validation that body has `width`/`height` fields.
- **Fix:** `.catch(() => ({}))` and explicit validation: `if (typeof body.width !== "number" || typeof body.height !== "number") return 400`.

### H7. **`containerFetch` First Definition Hardcodes Port 8080**
- **Location:** Line 366
- **Code:** `const fetcher = container.getTcpPort(8080); // This gets a fetcher for the container's port`
- **Problem:** Comment claims "the container's port" but 8080 is only one of three. `proxyWebSocket(request, 6080)` would still hit 8080. Defeats the entire WP.
- **Fix:** Already covered by B7 (delete the broken definitions). Mentioning here for impact.

### H8. **`Accept-Language`/Cookies/Auth Headers Stripped on Forward**
- **Location:** Lines 90-97 (first version); line 406-410 (Correction: passes whole `request.headers`)
- **Problem:** First version manually picks 5 headers and drops everything else. The Correction version uses `request.headers` — better. But neither preserves the `request.body` for non-GET methods (the Correction adds `body: request.body` which is `null` for GET, fine for WS upgrade since GET has no body).
- **Fix:** Use the Correction pattern (`headers: request.headers`) for the canonical implementation. Note: for WS upgrade, `Upgrade`/`Connection`/`Sec-WebSocket-*` must propagate — `request.headers` does this naturally.

### H9. **`containerUrl` Path Uses `request.url` — Pathname May Differ**
- **Location:** Line 87, 405
- **Code:** `http://localhost:${port}${new URL(request.url).pathname}${new URL(request.url).search}`
- **Problem:** If client connects to `/vnc` and noVNC's WebSocket handler is mounted at `/vnc/websockify` (the typical noVNC config), the container's noVNC server may not accept the bare `/vnc` path. The path mapping depends on the container's WS server. WP-03 (`supervisord.conf`) hasn't been written yet — WP-05 can't assume a specific path.
- **Fix:** Add an optional `pathSuffix` parameter to `proxyWebSocket` (e.g., `proxyWebSocket(req, 6080, "/")`) or document the exact path the container's WS server mounts at. **Cross-WP coordination required with WP-03.**

### H10. **No `HibernatableWebSocketEventTimeout` Configured**
- **Location:** `proxyWebSocket()`, line 80 — no call to `ctx.setHibernatableWebSocketEventTimeout`
- **Problem:** Default timeout is 30s — if the container is slow to acknowledge a ping during the 30s window, the WS is considered dead and the runtime fires `webSocketClose`. This can falsely close healthy WS during container cold-start.
- **Fix:** `this.ctx.setHibernatableWebSocketEventTimeout(60_000)` (or higher) in the constructor or at WS accept time. Document the choice.

### H11. **No Backpressure / Send Queue Limit**
- **Location:** `pipeWebSockets()`, all `send()` calls
- **Problem:** If container is slower than client (e.g., user pastes a huge script), `clientWs.send()` queues in memory unbounded. Memory grows → eventually OOM. No `bufferedAmount` check, no pause/resume.
- **Fix:** Check `if (clientWs.bufferedAmount > MAX_BUFFER)` and either drop or pause forwarding. `MAX_BUFFER` of ~1 MiB is reasonable.

---

## 🟡 MEDIUM SEVERITY ISSUES

### M1. **`noteActivity` Pattern Inconsistency with WP-01**
- **Location:** `webSocketMessage`, line 310
- **Problem:** Updates `lastActivityAt` in-memory only ("not persist on every message"). But WP-01's `noteActivity` is `this.renewActivityTimeout() + this.ctx.storage.put(ACTIVITY_KEY, Date.now())`. Two WPs, two activity-tracking implementations. Will drift in observability/billing.
- **Fix:** WP-05 should use `this.noteActivity()` (defined in WP-01) — same as WP-01's `proxyWebSocket` does. Document the in-memory vs persistence tradeoff in one place.

### M2. **`scheduleActivityPersistence()` Defined But Never Called**
- **Location:** Line 330-333
- **Problem:** Method exists but no call site. Dead code. Also overlaps with `alarm()` (line 335) which does the same thing.
- **Fix:** Remove `scheduleActivityPersistence()` and consolidate the alarm logic. Or document where it's invoked.

### M3. **No `Content-Type: application/json` on `/_health`**
- **Location:** `fetch()` `/_health` route, line 226-231
- **Problem:** Header not set; some clients will infer text/plain. WP-01 sets it. Inconsistent.
- **Fix:** Add `headers: { "Content-Type": "application/json" }`.

### M4. **`/_health` Endpoint Returns Ad-hoc Shape (Not `HealthCheckResponse`)**
- **Location:** Line 226-231
- **Problem:** Returns `{ status, ws_connections, uptime_ms }` but WP-01 defines `HealthCheckResponse` with `{ status, state, uptimeMs, containerAlive, ports, wsConnections, healthCheckFailures, lastCheckAt }`. Two different contracts for the same endpoint.
- **Fix:** `return new Response(JSON.stringify(await this.healthCheck()), { headers: { "Content-Type": "application/json" } });` (WP-01 stub throws `NOT_IMPLEMENTED` — coordinate with WP-06 to fill it in).

### M5. **`/api/snapshot` Defaults `force: true` Silently**
- **Location:** `fetch()`, line 215-217
- **Code:** `await request.json().catch(() => ({ force: true }));`
- **Problem:** A malformed body or empty body silently triggers a force snapshot. Force snapshot can be expensive (full profile+workspace upload). Should be explicit.
- **Fix:** `.catch(() => null)` then `if (body === null) return 400` OR explicit `{ force: false }` default.

### M6. **No Rate Limit / Connection Cap per DO**
- **Location:** `proxyWebSocket`, line 67
- **Problem:** A single malicious client can open thousands of WS → OOM. DoD #10 says "50+ simultaneous connections no degradation" but no upper bound.
- **Fix:** `if (this.state.wsConnections >= MAX_WS_PER_DO) return new Response("Too Many Connections", { status: 429 });` where `MAX_WS_PER_DO = 100`.

### M7. **No Reconnection / Resumability Strategy for Container**
- **Location:** All
- **Problem:** If container restarts (e.g., OOM), all live WS die with no auto-reconnect logic. The client has to retry from scratch. For long-lived VNC sessions (multi-hour), losing state on container restart is a UX cliff.
- **Fix:** Document this as known limitation. Provide a `GET /api/status` polling endpoint that returns `lastContainerRestartAt` so the client UI can show "reconnecting…".

### M8. **No Metrics Emission**
- **Location:** All
- **Problem:** No `bumpMetric("ws_connect", 1)` etc. Cold start count, WS count over time, message throughput — all unobservable. Campaign plan calls out `MetricsDO`.
- **Fix:** Add `bumpMetric("ws_connect", 1)`, `bumpMetric("ws_disconnect", 1)`, `bumpMetric("ws_message_bytes", event.data.byteLength)` in the runtime handlers.

### M9. **Dead-Code Path in Section 3.3 Reads Like Author Stream-of-Consciousness**
- **Location:** Lines 345-431 (whole section 3.3)
- **Problem:** Section shows the author's failed attempts (`containerFetch` v1, `containerFetch` v2, "Correction:" v3) and never converges on a single correct version. A merge of this file as-is would be unreviewable. Comments like "This won't work directly" (line 373) are evidence the author knew the code was wrong but left it in.
- **Fix:** Section 3.3 deleted in its entirety. Replaced with a SINGLE canonical `proxyWebSocket` (using `ctx.container.getTcpPort(port).fetch(req)`) and a 5-line `containerFetch` helper.

---

## 📋 DoD Gap Analysis

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | `/vnc` WS upgrades to port 6080 | ❌ **Fail** | `super.fetch()` routes to `defaultPort` (6080) — works for vnc only by accident; tty/code BROKEN (B6) |
| 2 | `/tty` WS upgrades to port 7681 | ❌ **Fail** | Same as #1 (B6) |
| 3 | `/code` WS upgrades to port 8080 | ❌ **Fail** | Same as #1 (B6) |
| 4 | Hibernation API used | ❌ **Fail** | `acceptWebSocket` called, but message forwarding BROKEN (B1, B2) |
| 5 | Bidirectional piping works | ❌ **Fail** | Pipe breaks on hibernation resume (B1) |
| 6 | `wsConnections` tracked | ⚠️ **Partial** | Tracked in-memory, lost on DO wakeup (B5) |
| 7 | DO hibernates when wsConnections === 0 | ⚠️ **Partial** | Runtime handles, but `alarm()` always reschedules (M-issue below) |
| 8 | WS close cleans up both sides | ❌ **Fail** | `webSocketClose` doesn't close container WS (broken pairing, B4) |
| 9 | Resize API forwards to container | ❌ **Fail** | Writes to wrong pty, wrong escape sequence (B11) |
| 10 | 50+ concurrent WS per DO | ⚠️ **Partial** | No upper bound, no backpressure (H11, M6) |
| 11 | WS errors don't crash DO | ❌ **Fail** | `this.log is not a function` TypeError crashes every error path (B10) |

**DoD Score: 0/11 PASS, 2/11 PARTIAL, 9/11 FAIL**

---

## 📋 Invariants Verification

| Invariant | Enforced? | Verdict |
|-----------|-----------|---------|
| I1: wsConnections ≥ 0 | ⚠️ Math.max(0, ...) on decrement, but reset to 0 on wakeup | **PARTIAL** |
| I2: acceptWebSocket once per upgrade | ✅ Yes, called once in proxyWebSocket | **ENFORCED** |
| I3: One WebSocketPair per call | ✅ new WebSocketPair() once | **ENFORCED** |
| I4: pipeWebSockets bidirectional, text+ArrayBuffer | ❌ Pipe broken on resume | **NOT ENFORCED** |
| I5: Client close → server closed, decremented | ⚠️ Server close called, but count NOT decremented in webSocketClose (only in pipeWebSockets which is broken) | **NOT ENFORCED** |
| I6: Server close → client closed, decremented | ❌ `webSocketClose` only logs; doesn't close client (lost pairing, B4) | **NOT ENFORCED** |
| I7: DO hibernates when wsConnections === 0 | ⚠️ `alarm()` always reschedules — prevents hibernation | **NOT ENFORCED** |
| I8: lastActivityAt updated on message | ⚠️ Updated in memory only; never persisted (M1) | **PARTIAL** |
| I9: Resize 1..8192 | ✅ Validated | **ENFORCED** |

**Invariants Enforced: 3/9 (33%) — INSUFFICIENT**

---

## 📋 Quality Standards Verification

| Standard | Met? | Evidence |
|----------|------|----------|
| Hibernation uses acceptWebSocket | ⚠️ Used but pipe broken (B1) |
| Both `string` and `ArrayBuffer` pass through | ⚠️ `send()` accepts both, but broken on resume |
| Backpressure | ❌ No `bufferedAmount` check (H11) |
| Error isolation | ❌ `this.log is not a function` crashes handler (B10) |
| Cleanup | ❌ Event listeners on hibernated WS undefined (H2) |
| Binary support | ⚠️ Type-correct, but pipe broken |
| Ping/pong | ⚠️ Default 30s timeout, no override (H10) |

**Quality Standards: 0/7 MET — INSUFFICIENT**

---

## 📋 Self-Check Points Analysis

### Self-Check 1: Hibernation API Correctness
- [x] Uses `acceptWebSocket` — yes but no tags (B4)
- [x] Implements webSocketMessage/Close/Error — yes, but wrong type (B3) and no-op (B1)
- [x] DO hibernates when wsConnections === 0 — **NO** (alarm always reschedules)
- [x] No setTimeout/setInterval — ✅ no timers in this WP
- [x] alarm() for periodic persistence — **NO**, alarm always reschedules (see also H-issue below)

**Verdict: 2/5 PASS, 3/5 FAIL**

### Self-Check 2: WebSocket Pair Piping Correctness
- [x] `MessageEvent.data` can be string/ArrayBuffer — code accepts both
- [x] `WebSocket.send()` accepts both — ✅
- [x] Binary (RFB) passes through — code yes, runtime BROKEN (B1)
- [x] Text passes through — code yes, runtime BROKEN (B1)
- [x] No JSON.parse/stringify — ✅

**Verdict: 5/5 code-correct, but RUNTIME BROKEN — FAIL**

### Self-Check 3: Connection Lifecycle Completeness
- [x] Client close → server.close, count-- — code does, count NOT decremented
- [x] Server close → client.close, count-- — **NO** (B4)
- [x] Server error → client.close(1011) — code yes, but `this.log` TypeErrors (B10)
- [x] Client error → server.close — code yes, but `this.log` TypeErrors (B10)
- [x] No listener accumulation — ✅ (none in loops)
- [x] wsConnections clamped at 0 — ✅ Math.max(0, ...)

**Verdict: 1/6 PASS, 5/6 FAIL**

---

## 📊 SCORECARD

| Category | Score | Required | Gap |
|----------|-------|----------|-----|
| Blocking Issues | 14 | 0 | **-14** |
| High Issues | 11 | 0 | **-11** |
| Medium Issues | 9 | 0 | **-9** |
| DoD Pass Rate | 0% | 100% | **-100%** |
| Invariants Enforced | 33% | 100% | **-67%** |
| Quality Standards | 0% | 100% | **-100%** |
| Self-Check Pass | 27% | 100% | **-73%** |

**OVERALL VERDICT: ❌ FAIL — Requires major rework before sign-off**

---

## 🔧 FIX PRIORITY

### Must Fix (Blockers)
1. B1, B2 — Fix hibernation message forwarding (runtime handler, not addEventListener)
2. B4 — Tag WS, persist pairing in Map
3. B5 — Restore `wsConnections` from `getWebSockets().length` on wakeup
4. B6 — Use `ctx.container.getTcpPort(port).fetch()` not `super.fetch()`
5. B7, B8 — Delete duplicate dead-code paths, single canonical implementation
6. B9 — Type cast for `Object.values`
7. B10 — Add `log()` method
8. B11 — Resize via WS messages, not shell escape
9. B3, B13 — Fix handler type signatures
10. B14 — Use canonical `DevenvState` from WP-01

### Should Fix (High)
1. H1 — Await `persistState()` everywhere
2. H2 — Remove addEventListener on hibernated WS
3. H3 — Call `noteActivity()` in WS path
4. H4 — AbortSignal on slow fetch
5. H6, H8, H9, H10, H11 — Input validation, header propagation, path config, timeout, backpressure

### Nice to Fix (Medium)
1. M1 — Use WP-01's `noteActivity`
2. M2 — Remove `scheduleActivityPersistence` dead code
3. M3, M4 — Use canonical `HealthCheckResponse` shape
4. M5 — Don't default `force: true` on snapshot
5. M6 — Connection cap
6. M7, M8 — Reconnect, metrics
7. M9 — Delete dead-code section 3.3

---

## NEXT STEPS

1. **Apply all BLOCKING fixes** (B1-B14) — non-negotiable
2. **Apply all HIGH fixes** (H1-H11) — required for quality
3. **Apply at least 6 of 9 MEDIUM fixes** (M1, M2, M3, M4, M5, M6 minimum)
4. **Re-verify all 11 DoD items pass**
5. **Re-verify all 9 invariants enforced in code**
6. **Re-verify all 7 quality standards met**
7. **Proceed to Iteration 2 review**

**Do NOT proceed to WP-06 until WP-05 is fixed and passes review.**

---

## 🔗 Cross-WP Coordination Required

- **WP-01 (Skeleton):** `DevenvState` shape, `noteActivity`, `super.fetch`/`stop` semantics, log pattern. Reuse canonical types.
- **WP-03 (Entrypoint/Supervisord):** Confirm WS endpoint paths (`/vnc/websockify`? bare `/vnc`?). Spec is path-naive.
- **WP-04 (clw Integration):** Resize message protocol for ttyd may need to use `clw` over WebSocket. Coordinate on JSON message shape.
- **WP-06 (Lifecycle):** State machine 7-status vs WP-05's 5-status — must align. `healthCheck()` endpoint shape. Alarm reschedule logic when WS count > 0.
- **WP-07 (Billing):** `wsConnections` count is a billing signal. WP-05 must be reliable.

---

**END OF WP-05 ITERATION 1 REVIEW**
