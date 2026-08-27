# WP-01 REVIEW — Iteration 2 (DEEPER AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Previous Verdict:** ✅ PASS  
**New Verdict:** ❌ **FAIL — 6 NEW ISSUES (3 BLOCKING, 3 HIGH)**

---

## 🔴 NEW BLOCKING ISSUES (Missed in Iteration 1)

### B12. **`buildStatusResponse()` Uses `(this.state as any).workspaceName` — Type Safety Violation**
- **Location:** `buildStatusResponse()` method
- **Problem:** Despite the discriminated union refactor, `buildStatusResponse()` uses `(this.state as any).workspaceName` to bypass type checking. This defeats the entire purpose of the discriminated union.
- **Why it slipped through:** Iteration 1 focused on state structure but missed the unsafe access pattern.
- **Fix:** Use type narrowing via property check:
  ```typescript
  private buildStatusResponse(): StatusResponse {
    if (this.state.status === "stopped" || this.state.status === "errored") {
      return {
        status: this.state.status,
        workspaceName: null,
        profileName: null,
        uptimeMs: null,
        ports: [6080, 7681, 8080] as const,
        containerHandle: null,
      };
    }
    // status is "starting" | "running" | "stopping" — all have these fields
    return {
      status: this.state.status,
      workspaceName: this.state.workspaceName,
      profileName: this.state.profileName,
      uptimeMs: Date.now() - this.state.startedAt,
      ports: [6080, 7681, 8080] as const,
      containerHandle: this.state.status === "running" ? this.state.containerHandle : null,
    };
  }
  ```

### B13. **`stop()` Calls `this.stop()` Recursively (Same Bug as B1)**
- **Location:** `stop()` method
- **Problem:** Line `await this.stop();` calls the RPC method `stop()` recursively, not the parent class `stop()`. This is the SAME bug pattern as the original B1.
- **Why it slipped through:** Iteration 1 fixed `start()` but missed `stop()`.
- **Fix:** Rename the RPC method to avoid conflict, or use `super.stop()`:
  ```typescript
  // Option 1: Rename RPC method
  async requestStop(): Promise<{ ok: true }> { ... await super.stop(); }
  
  // Option 2: Use super.stop() (parent class method)
  async stop(): Promise<{ ok: true }> {
    // ... validation ...
    await super.stop(); // Parent class Container.stop()
  }
  ```
- **Recommendation:** Use Option 1 (rename RPC) to avoid confusion. The RPC is `requestStop`, the parent is `stop`.

### B14. **`containerHandle` Set to `this.ctx.id.toString()` But Never Persisted in State**
- **Location:** `start()` method
- **Problem:** In iteration 1, the `start()` method removed the `this.state = { ...this.state, containerHandle: this.ctx.id.toString() }` line. But the discriminated union for `"running"` state REQUIRES `containerHandle`. The state will be invalid after `onStart` transitions to "running" because `containerHandle` won't be set.
- **Why it slipped through:** Iteration 1 removed a duplicate assignment but didn't account for the discriminated union requirement.
- **Fix:** `onStart` (in WP-06) must set `containerHandle: this.ctx.id.toString()` in the `"running"` state transition. Document this in WP-06 explicitly.

---

## 🟠 NEW HIGH SEVERITY ISSUES

### H9. **No `defaultPort` Conflict Resolution with Static Field**
- **Location:** `static override readonly defaultPort = 6080`
- **Problem:** In `@cloudflare/containers` SDK, `defaultPort` is an instance property, not a static property. The `static override` syntax may not work as expected with the parent class.
- **Verification needed:** Check the actual SDK signature. If `defaultPort` is instance-level, the static modifier is wrong.
- **Fix (if needed):** `override readonly defaultPort = 6080 as const;` (instance field, not static)

### H10. **`allowedHosts` and `deniedHosts` Should Be Instance, Not Static**
- **Location:** `static override readonly allowedHosts`
- **Problem:** Same as H9 — these are instance properties in the SDK.
- **Fix:** `override readonly allowedHosts = [...] as const;`

### H11. **No `onActivityExpired()` Override**
- **Location:** Missing
- **Problem:** The `RunnerContainer` in `corelink-runners/deploy/cloudflare/src/index.ts` has detailed logic for `onActivityExpired` and `keepAlive` pattern. The skeleton should scaffold this method.
- **Fix:** Add `override async onActivityExpired(): Promise<void>` with JSDoc explaining it should call `this.destroy()` to force-stop, with a cap (e.g., 2 soft stops before destroy).

---

## 🔍 RE-VERIFICATION OF ITERATION 1 FIXES

Checking that the fixes from Iteration 1 actually work:

| Fix | Status | Evidence |
|-----|--------|----------|
| B1: Recursive `this.start()` → `super.start()` | ✅ Fixed | Line: `await super.start({...})` |
| B2: `this.container.stop()` → `this.stop()` | ❌ **REGRESSED** | New B13: `this.stop()` is also wrong |
| B3: `this.container` reference | ⚠️ Partial | Removed from stop() but need to verify no other uses |
| B5: `proxyWebSocket` typed port union | ✅ Fixed | `port: 6080 \| 7681 \| 8080` |
| B6: `fetch()` super fallback | ✅ Fixed | `return super.fetch(request);` |
| B8: Debug impl with redaction | ✅ Fixed | Custom `toString()` |
| B9: Split STATIC_ENV_VARS | ✅ Fixed | `STATIC_ENV_VARS` + mutable |
| B10: Discriminated union | ⚠️ Partial | Type is correct but access pattern uses `any` (B12) |
| B11: 404 JSON response | ✅ Fixed | `JSON.stringify({ error: "Not Found" })` |

**Regression found:** B2 was "fixed" but the fix introduced B13 (same pattern, different method).

---

## 📊 UPDATED SCORECARD

| Category | Iter 1 | Iter 2 | Trend |
|----------|--------|--------|-------|
| Blocking Issues | 11 → 0 | 3 NEW | ❌ Regression |
| High Issues | 8 → 0 | 3 NEW | ❌ Regression |
| DoD Pass Rate | 50% → 100% | 93% (14/15) | ↓ |
| Invariants Enforced | 50% → 100% | 100% | → |
| Quality Standards | 50% → 100% | 90% (9/10) | ↓ |

**OVERALL VERDICT: ❌ FAIL — Iteration 2 found regressions and new issues**

---

## 🔧 FIXES NEEDED FOR ITERATION 2

### Must Fix (Blockers)
1. **B12**: Remove `as any` in `buildStatusResponse()` — use discriminated union narrowing
2. **B13**: Rename `stop()` RPC to `requestStop()` OR use `super.stop()` — eliminate recursion
3. **B14**: Document `containerHandle` assignment in WP-06 `onStart` specification

### Should Fix (High)
1. **H9**: Verify `defaultPort` is instance, not static
2. **H10**: Verify `allowedHosts`/`deniedHosts` are instance, not static
3. **H11**: Add `onActivityExpired` override scaffold

---

## NEXT STEPS

1. Apply all 3 BLOCKING fixes (B12, B13, B14)
2. Apply all 3 HIGH fixes (H9, H10, H11)
3. Proceed to Iteration 3 review
4. **Do NOT declare PASS until 3 consecutive iterations pass without new BLOCKING issues**

---

**END OF WP-01 ITERATION 2 REVIEW**