# WP-05 REVIEW — Iteration 2 (DEEPER AUDIT)

**Reviewer:** Self (TechLead persona)
**Date:** 2026-08-26
**Previous Verdict:** ✅ PASS (Iter 1)
**New Verdict:** ❌ **FAIL — 20 NEW ISSUES (9 BLOCKING, 8 HIGH, 3 MEDIUM)**

---

## Iter 1 Fix Verification

| Fix | Status | Evidence |
|-----|--------|----------|
| B1: addEventListener removed from hibernated WS | ✅ Fixed | §3.4 `webSocketMessage` is runtime handler |
| B2: No duplicate webSocketMessage | ✅ Fixed | Single runtime handler §3.4 |
| B3: webSocketError(ws, error: unknown) | ✅ Fixed | §3.4 line 256 |
| B4: acceptWebSocket tags + serializeAttachment | ✅ Fixed | §3.3 line 133-134 |
| B5: wsConnections reconciled from getWebSockets() | ✅ Fixed | §3.9 line 496-501 |
| B6: getTcpPort(port).fetch() not super.fetch() | ✅ Fixed | §3.3 line 149 |
| B7, B8: Single canonical implementation | ✅ Fixed | §3.3 single block |
| B9: Object.values cast | ✅ Fixed | §3.3 line 125 |
| B10: log() method defined | ✅ Fixed | §3.11 |
| B11: Resize via ttyd WS JSON | ⚠️ **REGRESSED** (B18, B19 below — wrong direction + ttyd doesn't parse JSON) |
| B13: alarm() signature | ✅ Fixed | §3.10 line 513 |
| B14: Use canonical DevenvState from WP-01 | ⚠️ **REGRESSED** (B15 below — `wsConnections` not in WP-01 union) |
| H1: await persistState | ⚠️ **REGRESSED** (H12, H13 below — transitionWSCount is async but put not awaited; handleWsClose sync ignores returned promise) |
| H2: addEventListener only on non-hibernated | ✅ Fixed | §3.3 line 179-188 |
| H3: noteActivity in WS path | ⚠️ **REGRESSED** (H14 below — backpressure early return skips noteActivity) |
| H6: request.json() validation | ✅ Fixed | §3.7 line 374, 381 |
| H8: headers propagation | ✅ Fixed | §3.3 line 144 |
| H9: url.pathname | ⚠️ **REGRESSED** (B22 below — noVNC websockify serves WS at `/`, not `/vnc`) |
| H10: setHibernatableWebSocketEventTimeout | ❌ **NOT EFFECTIVE** (B20 below — called in redefined `initializeState` that never runs) |
| H11: bufferedAmount check | ✅ Fixed | §3.4 line 228, §3.5 line 285 |

**Iter 1 Regressions:** 5 (B11, B14, H1, H3, H9, H10) — critical patterns broken despite passing iter 1 review.

---

## 🔴 NEW BLOCKING ISSUES

### B15. **`wsConnections` Not in WP-01's `DevenvState` Union — TypeScript Error**
- **Location:** §3.9 `transitionWSCount` line 469, §3.7 `/_health` line 397
- **Code:** `this.state = { ...this.state, wsConnections: clamped }`
- **Problem:** WP-01's `DevenvState` union (WP-01 §3.2 lines 96-130) has FIVE variants: `stopped`, `starting`, `running`, `stopping`, `errored`. NONE of them include `wsConnections`. WP-05 mutates `this.state` with a `wsConnections` field that doesn't exist in the discriminated union. **TypeScript will reject `{ ...this.state, wsConnections: 5 }` because the union variants don't have that key.**
- **Cross-WP context:** WP-06 §3.6 H6 mandates the field name: "WP-05 increments in `acceptWebSocket`, decrements in `webSocketClose`/`webSocketError`; field name must match exactly". WP-06's `healthCheck()` reads `this.state.wsConnections` via cast. WP-01's iter 2 B14 already flagged that `containerHandle` was missing from the running variant. Now `wsConnections` is missing from ALL variants.
- **Why iter 1 missed it:** Iter 1 reviewer did not cross-check WP-01's union variants against WP-05's mutations. The `wsConnections` field was added in iter 1's fix but never reconciled with the canonical types.
- **Fix:** Extend the WP-01 union to include `wsConnections: number` in the `running` variant (and any other that may hold live WS). Or, use a SIDE-TABLE key (separate from `STATE_KEY`) like WP-06 does for snapshot metadata (§3.3 M9 fix). Side-table is preferred — keeps union shape clean.

### B16. **WP-05's `fetch()` Override Bypasses WP-06's HEALTH_TOKEN Auth Gate**
- **Location:** §3.7 `fetch()` line 359-404, especially `/_health` branch line 389-400
- **Problem:** WP-06 §3.4 specifies that the `/_health` route is OWNED by WP-01's `fetch()` method, with an auth gate via `Authorization: Bearer ${this.env.HEALTH_TOKEN}`. WP-06 §3.6: "`fetch()` handler — WP-01 — WP-06 adds ONE branch (`/_health`); does NOT override". But WP-05's `fetch()` IS an `override` and implements `/_health` itself WITHOUT auth, returning a minimal shape. **If both WP-01's iter 1 fetch and WP-05's iter 1 fetch are merged, WP-05's override wins (later extends earlier). The auth gate from WP-06 is LOST.**
- **Security impact:** Any client that can reach the DO's `/_health` endpoint can poll container liveness, port state, WS connection count, and uptime — useful for timing attacks (e.g., "is the container ready to serve a slow endpoint?"). For a publicly-routed DO this is information disclosure.
- **Why iter 1 missed it:** Iter 1 reviewer focused on the hibernation message-pipe bug, not on the `/_health` route. The auth gate was a WP-06 design that wasn't on iter 1's radar.
- **Fix:** WP-05's `fetch()` MUST delegate `/_health` to `super.fetch(request)` so WP-01's auth-gated handler fires. Or, the auth gate must be inlined in WP-05.

### B17. **`/_health` Returns Wrong Shape — Mismatches `HealthCheckResponse`**
- **Location:** §3.7 `/_health` catch block line 394-399
- **Code:**
  ```typescript
  return this.respondJSON({
      status: this.state.status,
      ws_connections: this.state.wsConnections,
      uptime_ms: this.state.status === "running" ? Date.now() - this.state.startedAt : 0,
  });
  ```
- **Problem:** WP-01 §3.2 line 167-176 defines `HealthCheckResponse`:
  ```typescript
  { status, state, uptimeMs, containerAlive, ports, wsConnections, healthCheckFailures, lastCheckAt }
  ```
  Two failures:
  1. Field names are `snake_case` (status, ws_connections, uptime_ms) — should be `camelCase` per `HealthCheckResponse`.
  2. Missing required fields: `state`, `containerAlive`, `ports`, `healthCheckFailures`, `lastCheckAt`.
  3. The `this.state.startedAt` access on `stopped`/`errored` states — `startedAt` only exists on `starting/running/stopping` variants. TypeScript will reject the union narrowing.
  4. `this.state.wsConnections` doesn't exist (B15).
- **Fix:** Use the canonical `HealthCheckResponse` shape from WP-01 §3.2. Build it explicitly with the available fields, defaulting to safe nulls where the union doesn't have the data.

### B18. **Resize via `{columns, rows}` JSON to ttyd is BROKEN — ttyd Doesn't Parse JSON**
- **Location:** §3.8 `resize()` line 437-446
- **Problem:** ttyd's WebSocket protocol is bidirectional raw terminal I/O. The browser (xterm.js) sends keystrokes as binary or text frames; ttyd forwards them to the PTY's stdin. ttyd does NOT parse JSON control messages on the WS. Sending `{"columns": 80, "rows": 24}` to ttyd's WS would cause those literal characters to be typed into the bash shell — corrupting any active command.
- **Why iter 1 "fixed" this:** Iter 1's B11 fix was "use WS message instead of shell escape". The reviewer didn't verify that ttyd actually consumes the proposed JSON message format. The pattern `{columns, rows}` is from ttyd's INITIAL connection URL query params (e.g., `ws://localhost:7681/ws?columns=80&rows=24`), not a mid-session message.
- **Real mid-session resize for ttyd:** ttyd does NOT support mid-session PTY resize. The PTY size is set at ttyd startup via `-w N` (cols) and `-h N` (rows) CLI flags, or via xterm.js initial connect. To resize mid-session, the container would need to send `TIOCSWINSZ` ioctl on the PTY master fd — but ttyd owns that fd, so external resize is impossible.
- **Fix:** Remove ttyd resize attempt entirely. Document that mid-session resize is a no-op for ttyd. For noVNC and code-server, the BROWSER handles resize (RFB `SetDesktopSize` from client→x11vnc, VS Code's own resize from client→code-server). WP-05 should NOT send anything to the container for resize.

### B19. **Resize for noVNC and code-server is in WRONG DIRECTION**
- **Location:** §3.8 `resize()` line 436-446
- **Problem:** The spec sends the resize JSON on `pair.containerWs` (the outbound WS from DO to container). For noVNC, `containerWs` connects to websockify (port 6080) which proxies to x11vnc. The RFB `SetDesktopSize` message is sent CLIENT→SERVER (browser→x11vnc). WP-05's `containerWs.send(...)` goes SERVER→CLIENT (DO→websockify→x11vnc). x11vnc would interpret an unsolicited RFB message as a protocol error.
- **Same for code-server:** VS Code's resize is CLIENT→SERVER over the WS, not the other direction. Sending a server-initiated message would be a protocol violation.
- **Real fix:** Resize is a CLIENT responsibility. The browser (noVNC, ttyd, code-server) handles its own viewport changes and sends the appropriate protocol message upstream. The server's role is to NO-OP on `/api/resize` (or just acknowledge for billing purposes).
- **Fix:** `resize()` should record activity for billing, log the request, and return `{ ok: true }` without sending anything to the container.

### B20. **`setHibernatableWebSocketEventTimeout` Never Called — `initializeState` Redefined, Not Overridden**
- **Location:** §3.9 `initializeState()` line 480-502
- **Problem:** WP-05's `initializeState` is a NEW method that includes `this.ctx.setHibernatableWebSocketEventTimeout(WS_EVENT_TIMEOUT_MS)` (line 491). But WP-01's `RunnerDevEnvDO` constructor (WP-01 §3.3 line 256-259) calls `this.initializeState()` — and WP-01 defines its OWN `initializeState` (WP-01 §3.3 line 266-274). If both are in the class, TypeScript rejects the duplicate definition. If WP-05's replaces WP-01's, then WP-01's state-loading logic (read `STATE_KEY` from storage, fall back to "stopped" + `createdAt: Date.now()`) is lost.
- **The actual call chain in the merged class:** WP-01's constructor calls WP-01's `initializeState` (which DOESN'T call `setHibernatableWebSocketEventTimeout`). WP-05's `initializeState` (with the timeout) is NEVER CALLED.
- **Impact:** Default 30s timeout. If a container is slow to acknowledge a ping during a cold-start (very common — supervisord launching all 6 processes can take 20-30s), the runtime marks the WS dead and fires `webSocketClose` on a healthy session. Result: client disconnects during cold-start → user reloads → repeat.
- **Why iter 1 missed it:** Iter 1 verified the CALL to `setHibernatableWebSocketEventTimeout` exists in the spec, but did not trace the call chain from constructor to the method.
- **Fix:** Either (a) call `this.ctx.setHibernatableWebSocketEventTimeout(WS_EVENT_TIMEOUT_MS)` in WP-01's `initializeState` (cross-WP), or (b) call it from the constructor directly (before `initializeState`), or (c) override `initializeState` to call `super.initializeState()` first then set the timeout.

### B21. **Post-Hibernation Rehydrate Path is DEFERRED — Messages Lost on Resume**
- **Location:** §3.4 `webSocketMessage` line 215-225
- **Code:** When `this.wsPairs.get(attachment.connId)` returns null, the code logs "ws_message_orphan_after_resume" and drops the message.
- **Problem:** After DO eviction + resume:
  1. The hibernated `server` WS is restored by the runtime.
  2. The `wsPairs` Map (in-memory, not hibernated) is LOST.
  3. The `containerWs` (outbound WS to container) — UNCERTAIN. The container process is separate from the DO process. If the container is still alive, the outbound WS may still be valid, but the DO has no reference to it.
  4. The runtime fires `webSocketMessage` for any messages the client sent during hibernation. WP-05 has no Map entry → drops.
- **Risk Register §9 admission:** "on first message after resume, re-establish container WS (TODO: implement in WP-06)". But WP-06 doesn't own the rehydrate path (WP-06 §3.6 lists `wsConnections` ownership as WP-05's). WP-06 doesn't implement this.
- **Impact:** All client messages sent during DO hibernation are LOST. For a typing user, this means: type "ls -la" during eviction → resume → server has no record. UI appears frozen; user reloads.
- **Why iter 1 missed it:** Iter 1 noted the issue in the Risk Register as deferred. Did not flag as BLOCKING.
- **Fix:** Implement the rehydrate path. On `webSocketMessage` with no `wsPairs` entry:
  1. Get port from `attachment.port`.
  2. Re-establish `containerWs` by calling `this.ctx.container.getTcpPort(port).fetch(...)` with a synthesized WS upgrade request (the original path/headers).
  3. Wire the new `containerWs` to the existing `server` (hibernated) via `wsPairs.set`.
  4. Forward the buffered message.
  This requires buffering messages during the rehydrate (add a per-connId buffer or just synchronously re-establish).

### B22. **noVNC websockify Path Mismatch — `/vnc` vs `/`**
- **Location:** §3.3 `proxyWebSocket` line 140-145
- **Code:** `const containerUrl = \`http://localhost:${port}${url.pathname}${url.search}\`;`
- **Problem:** Per WP-03 §3.3 line 485, noVNC is served via:
  ```
  websockify --web /usr/share/novnc --heartbeat 30 6080 localhost:5900
  ```
  websockify accepts WS upgrade requests at the ROOT path `/` (with optional query params). It does NOT serve a WS endpoint at `/vnc`. The `--web /usr/share/novnc` option serves the noVNC HTML/JS at the root, but the WS upgrade is at `/` (or `/?...`).
- **What WP-05 does:** Client connects to `wss://.../vnc`. WP-05 forwards to `ws://localhost:6080/vnc`. websockify returns 404 for `/vnc`.
- **The same issue applies to ttyd and code-server if their default WS paths differ.** Per WP-03, ttyd listens on `7681` (default WS path is `/ws` or `/` depending on ttyd version). code-server on `8080` (default WS path is `/`).
- **Why iter 1 missed it:** Iter 1's H9 fix acknowledged "WP-03 must mount noVNC at the exact path the client requests" but did not verify the actual default. H9 added `url.pathname` propagation without checking that the path matches what the container serves.
- **Fix:** Add a per-port path mapping:
  - 6080 (noVNC/websockify) → `/`
  - 7681 (ttyd) → `/` (ttyd 1.7.x serves WS at `/ws` by default; confirm with WP-03)
  - 8080 (code-server) → `/`
  Or, document the exact path each container service serves and pass it explicitly.

### B23. **Duplicate `fetch()` Definitions — WP-01 and WP-05 Both Override**
- **Location:** WP-01 §3.3 line 501-536 defines `override async fetch()`. WP-05 §3.7 line 359-404 ALSO defines `override async fetch()`.
- **Problem:** If both WP-01 and WP-05 are merged into `runner_dev_env.ts`, the `override async fetch()` is declared TWICE. TypeScript rejects the second declaration. The merge is broken at compile time.
- **Why iter 1 missed it:** Iter 1's "canonical" version was the only one in WP-05's spec, but WP-01's spec (which was approved in iter 4) ALSO has a `fetch()`. The cross-WP consolidation was never done.
- **Fix:** Single canonical `fetch()` in WP-01 that includes WP-05's WS routing + WP-06's `/_health` auth gate. WP-05's `fetch()` body should be moved to WP-01 (with WP-05's owner noted). WP-05 then has NO `fetch()` of its own.

---

## 🟠 NEW HIGH SEVERITY ISSUES

### H12. **`transitionWSCount` is `async` but the internal `put` is Not `await`ed**
- **Location:** §3.9 line 467-471
- **Code:**
  ```typescript
  private async transitionWSCount(newCount: number): Promise<void> {
      const clamped = Math.max(0, newCount);
      this.state = { ...this.state, wsConnections: clamped };
      this.ctx.storage.put(STATE_KEY, this.state);
  }
  ```
- **Problem:** The function body has `this.ctx.storage.put(...)` as the LAST expression, not `await this.ctx.storage.put(...)`. The async function's resolved value IS the put's promise (implicit return). If the caller awaits the function, the write completes. If the caller does NOT await (H13), the write is fire-and-forget. The function being `async` is misleading — it does NOT await internally. Iter 1's H1 fix was to "await persistState everywhere" — but the function body itself doesn't await.
- **Fix:** `await this.ctx.storage.put(STATE_KEY, this.state);` — explicit await inside the function.

### H13. **`handleWsClose` is Sync → Fire-and-Forget on `transitionWSCount`**
- **Location:** §3.6 line 316-349, §3.4 line 253
- **Problem:** `handleWsClose` is declared `private handleWsClose(...): void` (sync). It calls `this.transitionWSCount(...)` (which returns `Promise<void>`) without await. The promise is dropped. The runtime handler `webSocketClose` (async) calls `this.handleWsClose(...)` and the await chain is broken at the first sync call. The storage write can be lost on DO eviction within the next event-loop tick.
- **Call sites:**
  - `webSocketClose` (async) → `handleWsClose` (sync) → `transitionWSCount` (async, not awaited) → `ctx.storage.put` (async, not awaited).
  - `handleContainerClose` (sync, addEventListener) → `handleWsClose` (sync) → same chain.
  - `webSocketError` (async) → `handleWsClose` (sync) → same chain.
- **Iter 1's H1 fix:** "await this.persistState() everywhere, OR change `persistState` to return Promise and ALWAYS await at call sites." The fix was made to `transitionWSCount` (now returns Promise) but the call sites don't await. Partial fix.
- **Fix:** Make `handleWsClose` async; make all callers await. Or, use `this.ctx.waitUntil(this.transitionWSCount(...))` to keep the I/O gate open. Simpler: make `handleWsClose` async and update callers.

### H14. **`webSocketMessage` Backpressure Early Return Skips `noteActivity()`**
- **Location:** §3.4 line 227-231
- **Code:**
  ```typescript
  if (pair.containerWs.bufferedAmount > MAX_WS_BUFFERED_BYTES) {
      this.log("warn", "ws_backpressure_drop", { connId: attachment.connId });
      return;
  }
  // ... send ...
  this.noteActivity();
  ```
- **Problem:** On backpressure drop, the function returns BEFORE `this.noteActivity()`. So the activity clock doesn't tick for a dropped message. For an active session where the container is slow (backpressure frequent), the `sleepAfter` timer (30m idle) may not be reset, and the container can be put to sleep while WS are still "active" (just backpressured).
- **Iter 1's H3 fix:** "noteActivity on every WS message." The fix is partial — only fires on non-backpressured messages.
- **Fix:** Move `this.noteActivity()` to BEFORE the backpressure check, or call it in the backpressure drop branch too.

### H15. **`initializeState` Redefined Instead of Overridden — Conflict with WP-01**
- **Location:** §3.9 line 480-502
- **Problem:** Same structural issue as B23 (duplicate definition). WP-01's `initializeState` (WP-01 §3.3 line 266-274) reads `STATE_KEY` from storage, falls back to a fresh state. WP-05's `initializeState` adds the reconcile and timeout. If both exist in the class → TS error. If WP-05 replaces WP-01's → state loading logic is lost.
- **Fix:** `override private initializeState(): void { super.initializeState(); this.ctx.setHibernatableWebSocketEventTimeout(WS_EVENT_TIMEOUT_MS); /* reconcile */ }`. Or coordinate with WP-01 to add the timeout call there.

### H16. **No `request.signal` AbortController on Container Fetcher**
- **Location:** §3.3 `proxyWebSocket` line 150
- **Code:** `const containerResponse = await containerFetcher.fetch(containerRequest);`
- **Problem:** If the client disconnects during container cold-start (long await), the DO holds the fetcher open until the container responds. No timeout, no abort. The `request.signal` (client's signal) is not propagated. Iter 1's H4 fix was acknowledged but deferred ("full AbortController deferred to follow-up"). It's still deferred.
- **Fix:** `const ac = new AbortController(); request.signal.addEventListener("abort", () => ac.abort()); const containerResponse = await containerFetcher.fetch(containerRequest, { signal: ac.signal });`.

### H17. **Container `alarm()` Override Conflicts with WP-06's `schedule()` Pattern**
- **Location:** §3.10 line 513-524
- **Problem:** WP-06 §3.6 B4 fix: use SDK's `schedule()` (not `setAlarm`) for periodic tasks. The SDK's `Container.alarm()` (in the `cloudflare:workers` parent) manages the schedule SQL table. WP-05's `override async alarm()` reimplements the periodic-reschedule logic via `ctx.storage.setAlarm(...)`. If both WP-05 and WP-06 are merged, the override would either:
  - Skip `super.alarm()`, breaking SDK schedule management (schedule callbacks never fire).
  - Call `super.alarm()`, and the SDK's own reschedule logic fights WP-05's `setAlarm` (race).
- **Fix:** Remove WP-05's `alarm()` override. Periodic activity persistence is owned by WP-06's `healthCheckTick` via `schedule()`. WP-05 should NOT touch the alarm.

### H18. **HOP-BY-HOP Headers Propagated to Container**
- **Location:** §3.3 line 143-145
- **Code:** `headers: request.headers`
- **Problem:** `request.headers` includes `Upgrade`, `Connection`, `Sec-WebSocket-Key`, `Sec-WebSocket-Version` — all HOP-BY-HOP for WS upgrades. The CF container fetcher SHOULD strip these and inject its own, but the behavior is not documented. If it doesn't strip, the container's WS handshake may fail.
- **Fix:** Explicitly construct the forward headers: `["Host", "Sec-WebSocket-Protocol"]` (or empty for default). Let the container fetcher handle Upgrade/Connection.

### H19. **`this.state.wsConnections` Read in `/_health` (camelCase violation)**
- **Location:** §3.7 line 396
- **Code:** `ws_connections: this.state.wsConnections`
- **Problem:** Field is `snake_case` on the wire but `wsConnections` is the in-memory name. Iter 1's M3 fix was to add `Content-Type: application/json` but didn't address the wire format. The canonical `HealthCheckResponse` uses `wsConnections` (camelCase) per WP-01 §3.2 line 173.
- **Fix:** Use `wsConnections` in the response (along with the rest of the `HealthCheckResponse` shape per B17).

---

## 🟡 MEDIUM SEVERITY ISSUES

### M10. **`/_health` Catch Block Uses `this.state.startedAt` Without Type Narrowing**
- **Location:** §3.7 line 397
- **Code:** `this.state.status === "running" ? Date.now() - this.state.startedAt : 0`
- **Problem:** When `this.state.status === "running"`, the union narrows to the `running` variant which has `startedAt: number`. OK in this branch. But the response shape doesn't match `HealthCheckResponse` (B17). The `uptimeMs` field should be present (camelCase) and the value should be safe across all states.

### M11. **`containerWs` Dead-but-Not-Closed Case Not Handled**
- **Location:** §3.4 `webSocketMessage` line 233-236
- **Problem:** If the container process crashes WITHOUT firing the WS close event (e.g., SIGKILL, network partition), the `containerWs.readyState` may remain `OPEN` but no messages will flow. The DO's `webSocketMessage` will continue calling `pair.containerWs.send(...)` which will queue forever (or until the runtime times out the socket).
- **Fix:** Add a heartbeat / liveness check on `containerWs`. Or, after N consecutive send failures, close the client too. (Out of scope for iter 2; document as known limitation.)

### M12. **`noteActivity()` Skipped for Resize for vnc/code-server**
- **Location:** §3.8 `resize()` line 450
- **Problem:** The resize for ttyd calls `this.noteActivity()` (line 450, after the loop). For vnc and code-server, no activity is recorded. The comment says "We just record the activity for billing" — but the activity is only recorded if the ttyd branch ran. If the resize is for vnc only, activity is not recorded.
- **Fix:** Always call `this.noteActivity()` (regardless of ttyd branch).

---

## 📋 DoD Re-Verification (Iter 2)

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | `/vnc` WS upgrades to port 6080 (noVNC) | ❌ **Fail** | B22: path `/vnc` doesn't match websockify root |
| 2 | `/tty` WS upgrades to port 7681 (ttyd) | ⚠️ **Partial** | Path may not match (depends on ttyd default); ttyd URL needs `?token=` |
| 3 | `/code` WS upgrades to port 8080 (code-server) | ⚠️ **Partial** | Path may not match |
| 4 | Hibernation API used with tags | ✅ Pass | acceptWebSocket + tags + serializeAttachment |
| 5 | Bidirectional piping works (after resume) | ❌ **Fail** | B21: rehydrate path missing; messages lost on resume |
| 6 | Connection count tracked accurately post-resume | ⚠️ **Partial** | Reconciled on init but B15: `wsConnections` not in union → TS error |
| 7 | DO hibernates when no WS | ⚠️ **Partial** | `alarm()` override conflicts with WP-06 (H17) |
| 8 | WS close cleans up both sides (idempotent) | ✅ Pass | handleWsClose idempotent |
| 9 | Resize API forwards via ttyd WS message | ❌ **Fail** | B18: ttyd doesn't parse JSON; B19: wrong direction for noVNC/code-server |
| 10 | 50+ concurrent WS per DO | ✅ Pass | MAX_WS_PER_DO=100, backpressure |
| 11 | WS errors don't crash DO | ⚠️ **Partial** | log() defined; B15 (TS error) and H13 (fire-and-forget write) reduce reliability |

**DoD Score: 2/11 PASS, 3/11 PARTIAL, 6/11 FAIL**

---

## 📋 Invariants Re-Verification

| Invariant | Enforced? | Verdict |
|-----------|-----------|---------|
| I1: wsConnections ≥ 0; accurate | ❌ | Math.max clamps, but field doesn't exist in union (B15); reconcile works in code but won't compile |
| I2: acceptWebSocket once per upgrade | ✅ | Single call in proxyWebSocket |
| I3: One WebSocketPair per call | ✅ | new WebSocketPair() once |
| I4: pipe bidirectional, text+ArrayBuffer | ⚠️ | Code-correct but B21: messages lost on resume |
| I5: On client close: container closed, count-- | ⚠️ | Code OK; H13 fire-and-forget on count write |
| I6: On container close: client closed, count-- | ⚠️ | Same as I5 |
| I7: DO hibernates when wsConnections === 0 | ⚠️ | alarm() override conflicts with WP-06 (H17) |
| I8: lastActivityAt updated on WS message | ⚠️ | Backpressure drop skips noteActivity (H14) |
| I9: Resize 1 ≤ w,h ≤ 8192 | ✅ | Validated |
| I10: Each WS pair has at most one containerWs and one clientWs | ✅ | wsPairs keyed by connId |
| I11: webSocketClose is idempotent | ✅ | handleWsClose early-returns if pair missing |

**Invariants Enforced: 4/11 (36%) — REGRESSION from iter 1 (11/11)**

---

## 📊 UPDATED SCORECARD

| Category | Iter 1 | Iter 2 | Trend |
|----------|--------|--------|-------|
| Blocking Issues | 14 → 0 | 9 NEW | ❌ Regression |
| High Issues | 11 → 0 | 8 NEW | ❌ Regression |
| Medium Issues | 9 → 0 | 3 NEW | ❌ Regression |
| DoD Pass Rate | 100% | 18% (2/11) | ↓ |
| Invariants Enforced | 100% | 36% (4/11) | ↓ |
| Quality Standards | 100% | 43% (3/7) | ↓ |

**OVERALL VERDICT: ❌ FAIL — Iteration 2 found severe regressions and unresolved cross-WP coordination gaps**

---

## 🔧 FIX PRIORITY

### Must Fix (Blockers) — 9
1. **B15**: Add `wsConnections` to WP-01's `DevenvState` union OR use side-table key
2. **B16**: WP-05 `fetch()` `/_health` MUST delegate to `super.fetch()` (preserve WP-06 auth)
3. **B17**: Use canonical `HealthCheckResponse` shape
4. **B18, B19**: Resize is a no-op for the container — record activity, return ok
5. **B20**: Move `setHibernatableWebSocketEventTimeout` to WP-01's `initializeState`
6. **B21**: Implement post-hibernation rehydrate in `webSocketMessage`
7. **B22**: Per-port path mapping (noVNC: `/`, ttyd: `/`, code-server: `/`); or document
8. **B23**: Consolidate `fetch()` into WP-01; remove duplicate from WP-05

### Should Fix (High) — 8
1. **H12**: `await` inside `transitionWSCount`
2. **H13**: Make `handleWsClose` async, await all callers
3. **H14**: Move `noteActivity()` before backpressure check
4. **H15**: `override initializeState` and call `super.initializeState()`
5. **H16**: AbortController on container fetcher with `request.signal`
6. **H17**: Remove `alarm()` override (let WP-06's `schedule()` own it)
7. **H18**: Strip HOP-BY-HOP headers from forwarded request
8. **H19**: Use `wsConnections` (camelCase) per `HealthCheckResponse`

### Nice to Fix (Medium) — 3
1. **M10**: Safe `uptimeMs` across all states
2. **M11**: Document containerWs liveness limitation
3. **M12**: Always call `noteActivity()` in resize

---

## 🔗 Cross-WP Coordination Required

| Concern | Owner | WP-05 Action |
|---------|-------|--------------|
| `wsConnections` field on state | WP-01 | WP-05 must request WP-01 union extension (B15) |
| `/_health` auth gate | WP-01 + WP-06 | WP-05's `fetch()` must `super.fetch()` (B16) |
| `fetch()` consolidation | WP-01 | WP-05's `fetch()` body moves to WP-01 (B23) |
| `setHibernatableWebSocketEventTimeout` call site | WP-01 | WP-05 requests WP-01 add it to `initializeState` (B20) |
| `initializeState` consolidation | WP-01 | WP-05's additions extend WP-01's method (H15) |
| `alarm()` ownership | WP-06 | WP-05 removes its override; WP-06's `schedule()` owns periodic (H17) |
| Per-port WS paths | WP-03 | WP-05 documents exact paths; WP-03 confirms (B22) |
| ttyd resize protocol | WP-03 | Mid-session resize unsupported; document (B18) |
| `state.wsConnections` field access pattern | WP-06 | Already reads via cast in `healthCheck`; OK with B15 fix |
| exec-server vs container exec | WP-02/03/04/06 | WP-05 doesn't exec; no direct conflict |
| Billing handoff for resize | WP-07 | WP-05's resize records activity; WP-07 reads from `state.lastActivityAt` |

---

## NEXT STEPS

1. Apply all 7 BLOCKING fixes (B15–B23) — non-negotiable
2. Apply all 4 HIGH fixes (H12–H19) — required for quality
3. Apply 3 MEDIUM fixes (M10–M12)
4. Re-verify DoD (target: 11/11 PASS)
5. Re-verify invariants (target: 11/11)
6. Coordinate with WP-01 (union + fetch + initializeState), WP-06 (alarm + _health auth)
7. Proceed to Iteration 3

**Do NOT declare PASS until 2 consecutive iterations pass without new BLOCKING issues.**

---

**END OF WP-05 ITERATION 2 REVIEW**
