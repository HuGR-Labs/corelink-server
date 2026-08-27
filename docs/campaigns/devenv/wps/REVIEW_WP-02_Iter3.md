# WP-02 REVIEW — Iteration 3 (CONVERGENCE CHECK)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Previous Verdicts:** ❌ FAIL Iter 1 (12B+9H+8M), ❌ FAIL Iter 2 (4B+4H+2M)  
**New Verdict:** ❌ **FAIL — 3 NEW BLOCKING + 1 CARRY-OVER BLOCKING**

---

## 1. Convergence Assessment

**Iter 1+2 fixes verified: 17/17 spot-checked correct.**

Spot checks:
| Iter | Fix | Status | Evidence |
|------|-----|--------|----------|
| 1 | B1: 9 clw-* crates COPYed | ✅ | Lines 93-101 |
| 1 | B2: Duplicate `clw-manifest` removed | ✅ | Single line 97 |
| 1 | B3: ttyd+code-server bin-downloader stage | ✅ | Lines 136-156 |
| 1 | B6: vnc_lite.html symlink | ✅ | Line 272 |
| 1 | B11: Exec form `clw --version` | ✅ | Line 239 |
| 1 | B12: PASSWORD_STORE removed | ✅ | Not in ENV block |
| 1 | H4: `/tcp` suffix on EXPOSE | ✅ | Line 306 |
| 1 | H5: OCI LABELS | ✅ | Lines 166-170 |
| 1 | H7+H8: Reproducibility + buildx | ✅ | Lines 363-372 |
| 2 | B13: sed-patched Cargo.toml + hard-coded list | ✅ | Lines 109, 120 |
| 2 | B14: COPY entrypoint.sh + supervisord.conf | ✅ | Lines 231-232 |
| 2 | B15: `--entrypoint=""` bypass | ✅ | Line 386 |
| 2 | B16: `/tmp/runtime-coder` setup | ✅ | Lines 262-264 |
| 2 | H11: SHA↔version coupling comment | ✅ | Lines 354-358 |
| 2 | H12: deny.toml in .dockerignore | ✅ | Line 330 |
| 2 | H13: DoD #1, #12, #13 | ✅ | Lines 421, 432-433 |
| 2 | M9: HEALTHCHECK all 3 ports | ✅ | Lines 282-286 |

**Trajectory:**
- Iter 1: 12B + 9H + 8M = 29 issues
- Iter 2: 4B + 4H + 2M = 10 new issues
- Iter 3: 3B + 0H + 0M = 3 new issues (all BLOCKING)

**Net: convergence detected, but with 3 new BLOCKING regressions of a different class** (semantic, not configuration). Trend is good but iter 4 is required to fix the new BLOCKINGs.

---

## 2. NEW BLOCKING ISSUES (Iter 3)

### B17. **`ARG TTYD_VERSION` and `ARG CODE_SERVER_VERSION` in `bin-downloader` Stage Lack Defaults → Empty String in URL**

- **Location:** Dockerfile lines 138-139 (bin-downloader stage)
- **Code:**
  ```dockerfile
  FROM debian:bookworm-slim@sha256:b29f74a... AS bin-downloader
  ARG TTYD_VERSION
  ARG CODE_SERVER_VERSION
  ```
- **Problem:** In BuildKit, ARG scope is per-stage. Each `FROM` starts a fresh ARG namespace. The top-of-file `ARG TTYD_VERSION=1.7.7` (line 66) and `ARG CODE_SERVER_VERSION=4.96.4` (line 67) belong to the **clw-builder** stage (the stage active when they're declared, even though BuildKit re-declares ARGs at every `FROM` boundary). When the `bin-downloader` stage begins at line 136, those ARGs are NOT in scope. Lines 138-139 re-declare them WITHOUT defaults → both are empty strings in this stage. The RUN at line 149 then composes: `https://github.com/tsl0922/ttyd/releases/download//ttyd.x86_64` (empty `${TTYD_VERSION}`) → 404 download → build fails. **This is a regression introduced in iter 1 (B3 fix) and undetected in iter 2.**
- **Why it slipped through:** Iter 1 introduced the bin-downloader stage (B3 fix) with these ARG re-declarations. Iter 2 verified the SHA256-pinning and reproducibility check but did not catch that the values would be empty at runtime. The H11 fix (SHA↔version coupling comment) actually makes this MORE likely to bite — the comment says "you MUST update the SHAs in the Dockerfile" but the values aren't even being passed.
- **Fix:** Add defaults to the bin-downloader ARG re-declarations:
  ```dockerfile
  ARG TTYD_VERSION=1.7.7
  ARG CODE_SERVER_VERSION=4.96.4
  ARG TTYD_SHA256=fb6e4987f4b1de8e9522f8f6c4f6a7b7a5b8e8b7c8d8b8b8b8b8b8b8b8b8b8b
  ARG CODE_SERVER_SHA256=0000000000000000000000000000000000000000000000000000000000000000
  ```
  Note: SHA256 placeholders are also unaddressed (see B19), but the B17 fix only requires the defaults. Both ARGs and the SHA256s should be moved into a `versions.env` file sourced at build time, but that's a refactor for later.

### B18. **`cargo build --locked` Will Fail After `sed` Removes Workspace Members — Cargo.lock Becomes Stale**

- **Location:** Dockerfile lines 92, 109, 125
- **Code:**
  ```dockerfile
  COPY Cargo.toml Cargo.lock rust-toolchain.toml ./  # Cargo.lock COPYed
  ...
  RUN sed -i '/clw-integration-tests\|clw-conformance\|clw-e2e-evidence/d' Cargo.toml  # Manifest mutated
  ...
  cargo build --release --locked -p clw-cli --bin clw  # --locked validates lock vs mutated manifest
  ```
- **Problem:** `cargo build --locked` requires that `Cargo.lock` is "consistent with the current Cargo.toml" — it refuses to build if the lockfile references packages no longer in the workspace. After the `sed` (line 109) removes the 3 test-only members from `Cargo.toml`, `Cargo.lock` still contains locked version entries for those 3 packages. `cargo build --locked` will fail with `error: the lock file ... needs to be updated`. The build halts before producing `/out/clw`. **This is a regression introduced in iter 2 (B13 fix).**
- **Why it slipped through:** Iter 2 was focused on the workspace-resolver enumeration problem (B13 — `cargo metadata` failing on missing manifests). The fix correctly avoids `cargo metadata` but introduced a new failure mode: the lockfile is stale after the sed. Neither iter caught this because the spec is text-only and no actual `cargo build` was attempted.
- **Fix:** After the sed, regenerate the lockfile before the build. Two options:
  - **(a) Recommended — minimal change:** Replace `sed` with a more surgical patch that ALSO drops the test crate entries from Cargo.lock, OR add `cargo update --workspace` after the sed (regenerates lock entries for the modified workspace). The catch: `cargo update` mutates the lockfile, which is fine because the build context's `Cargo.lock` is throwaway; the published `clw` binary doesn't ship with a lockfile. Add this step:
    ```dockerfile
    RUN sed -i '/clw-integration-tests\|clw-conformance\|clw-e2e-evidence/d' Cargo.toml && \
        cargo update --workspace --offline 2>/dev/null || cargo generate-lockfile
    ```
    Note: `cargo update --workspace --offline` may fail on first build (no cache); fallback to `cargo generate-lockfile` (creates a fresh lockfile from Cargo.toml — same end state).
  - **(b) Cleaner — restructure the build context:** COPY only the 9 first-party crate sources + a custom hand-written `Cargo.toml` and `Cargo.lock` that already excludes the test members. More work, but eliminates the sed-and-update dance.
- **Recommendation:** Option (a) for minimal blast radius. Document the rationale in a comment.

### B19. **`ttyd` and `code-server` SHA256 Digests Are Still Placeholders — Build Will Fail (CARRY-OVER, NOT A REGRESSION)**

- **Location:** Dockerfile lines 140-141
- **Code:**
  ```dockerfile
  ARG TTYD_SHA256=fb6e4987f4b1de8e9522f8f6c4f6a7b7a5b8e8b7c8d8b8b8b8b8b8b8b8b8b8b
  ARG CODE_SERVER_SHA256=0000000000000000000000000000000000000000000000000000000000000000
  ```
- **Problem:** Acknowledged in iter 1 (B3 fix) and iter 2 (H13 — DoD #1/12/13 added). The placeholders remain because real upstream digests need to be fetched at build time (out-of-band, by an operator). The iter 2 review marked this as "DO NOT BUILD until real digests are pinned" but the WP still has placeholders. DoD #12 is the test: `grep ^ARG.*SHA256 Dockerfile.runner-devenv` should return non-zero hash strings — currently returns `fb6e4987...b8b8b8b8b8b8` (pattern: `b8b8b7` repeated) and all-zeros. **Both fail DoD #12.**
- **Why it slipped through:** This is a known limitation, not a regression. The fix requires external action (fetch real SHAs from GitHub releases). Iter 3 cannot fix this without network access. Flagging as carry-over BLOCKING.
- **Fix (out of iter 3 scope):** Operator action — fetch `sha256sum` of `ttyd.x86_64` for v1.7.7 and `code-server-4.96.4-linux-amd64.tar.gz` for v4.96.4 from their respective GitHub release pages. Replace the placeholders. Re-verify the build. This is a 1-line PR.

---

## 3. NEW HIGH/MEDIUM ISSUES (Iter 3)

**None found.** All discovered issues are BLOCKING (semantic, not configuration).

---

## 4. REGRESSION CHECK (Iter 1+2 Fixes)

All 17 spot-checked iter 1+2 fixes are still correctly applied. No regressions of previously-fixed issues.

---

## 5. CROSS-WP CONSISTENCY (Iter 3)

### WP-01 ↔ WP-02
- **defaultPort = 6080** ✓ matches WP-02 HEALTHCHECK
- **requiredPorts = [6080, 7681, 8080]** ✓ matches WP-02 EXPOSE + HEALTHCHECK
- **Static envVars (CLW_REF_DOMAIN="runner", CLW_ENDPOINT)** ✓ matches WP-02 ENV comment
- **Mutable envVars (CLW_TENANT, CLW_TOKEN, WORKSPACE_NAME, PROFILE_NAME)** ✓ matches WP-02 ENV comment

### WP-02 ↔ WP-03
- **`/data/chrome` and `/data/workspace`** ✓ Both WPs agree
- **CLW binary at `/usr/local/bin/clw`** ✓ Both WPs agree
- **Ports 6080, 7681, 8080** ✓ Both WPs agree
- **`/entrypoint.sh` + `/etc/supervisor/conf.d/supervisord.conf`** ✓ WP-02 COPYs (B14 fix)
- **`XDG_RUNTIME_DIR=/tmp/runtime-coder`** ✓ WP-02 creates dir, WP-03 sets env

### WP-02 ↔ WP-05 (WebSocket Proxy)
- **`TTYD_CRED` and `CODE_SERVER_PASSWORD`** — WP-03 reads these from env (line 506, 539 of WP-03 supervisord.conf), but **WP-01's mutable envVars spread (lines 336-342 of WP-01) does NOT include them**. Confirmed by grep: `STATIC_ENV_VARS` only has `CLW_REF_DOMAIN` and `CLW_ENDPOINT`; mutable envVars inject only the 4 above. **If WP-05 doesn't inject TTYD_CRED/CODE_SERVER_PASSWORD, both ttyd and code-server start with empty credentials.** This is a cross-WP finding (already raised in iter 2 review at line 194) and remains unresolved. WP-02 cannot fix this — it's WP-01's responsibility (add to envVars spread) or WP-05's (inject at spawn).

### WP-02 ↔ WP-04 (clw Integration)
- **`/usr/local/bin/clw`** ✓
- **clw dynamic linking to glibc** ✓ I7 invariant enforced

**No new cross-WP issues found in iter 3.**

---

## 6. INTERNAL CONSISTENCY (Iter 3)

### Section 3.2 (Dockerfile) vs Section 4 (DoD)
- DoD #1: "Dockerfile builds without errors (GIVEN real SHA256 digests are pinned)" — **FAIL** in current state (B17 + B18 + B19 all block the build)
- DoD #2: All `FROM` lines have `@sha256:` — ✓
- DoD #3-11: All pass when build succeeds
- DoD #12: SHA256 digests are real — **FAIL** (B19, carry-over)
- DoD #13: WP-03 provides entrypoint + supervisord — ✓ (file COPY is in Dockerfile)

### Section 3.2 (Dockerfile) vs Section 5 (Invariants)
- I1: Base image digests pinned — ✓
- I2: clw built from pinned Cargo.lock — ✓ (when B18 fixed)
- I3: USER coder — ✓
- I4: /data dirs owned by coder — ✓
- I5: Only 3 ports exposed — ✓
- I6: No .git/node_modules/target — ✓
- I7: clw dynamically linked to glibc — ✓
- I8: SHA256-verified binary downloads — ⚠️ Partial (placeholders, B19)
- I9: chmod 0755 binaries — ✓
- I10: STOPSIGNAL SIGTERM — ✓

### Section 7 (Completeness) vs Section 3.2
- Checklist says "`ttyd` and `code-server` SHA256 digests filled in (currently placeholders — DO NOT BUILD until real digests are pinned)" — honest; matches current state.

---

## 7. UPDATED SCORECARD

| Category | Iter 1 | Iter 2 | Iter 3 | Trend |
|----------|--------|--------|--------|-------|
| Blocking Issues | 12 | 4 NEW | 3 NEW (B17, B18, B19 carry) | ↓ but still 3 |
| High Issues | 9 | 4 NEW | 0 NEW | ↓ converged |
| Medium Issues | 8 | 2 NEW | 0 NEW | ↓ converged |
| DoD Pass Rate | 36% | 45% | 38% (5/13) | ↓ regressed due to B17/B18 (cargo build won't run) |
| Invariants Enforced | 62.5% | 100% | 87% (7/8) | ↓ due to B19 |
| Quality Standards | 67% | 100% | 100% | → maintained |
| Self-Checks | 53% | 100% | 100% | → maintained |

**Convergence: PARTIAL.** HIGH and MEDIUM severities have converged to 0 new issues. BLOCKING issues have NOT converged: 3 new BLOCKINGs found (B17, B18) plus 1 carry-over BLOCKING (B19).

---

## 8. CONVERGENCE PREDICTION

| Metric | Iter 1→2 | Iter 2→3 | Iter 3→4 predicted |
|--------|----------|----------|---------------------|
| New BLOCKING | 12→4 | 4→3 | 3→0 (all addressed) |
| New HIGH | 9→4 | 4→0 | 0→0 |
| New MEDIUM | 8→2 | 2→0 | 0→0 |
| Net improvement | 19 | 7 | 3+ (B19 resolved externally) |

**Iter 4 prediction:** 0-1 new issues expected, all cosmetic. WP-02 should reach PASS in iter 4.

---

## 9. FIXES REQUIRED FOR ITERATION 4

### Must Fix (Blockers)
1. **B17**: Add defaults to `ARG TTYD_VERSION=1.7.7` and `ARG CODE_SERVER_VERSION=4.96.4` in bin-downloader stage (lines 138-139). ALSO add defaults to the SHA256 ARGs (line 140-141) — they already have them, but move them to the same block for consistency.
2. **B18**: After `sed -i ... Cargo.toml` (line 109), regenerate the lockfile before `cargo build --locked`. Recommended: `cargo update --workspace` or `cargo generate-lockfile`.
3. **B19 (carry-over)**: Operator action — fetch real SHA256 digests and replace placeholders. Out of iter 3 scope; flag for next pass.

### Should Fix (None — all HIGH converged)

### Nice to Fix (None)

### Cross-WP Coordination Required
- **WP-01 owner**: Confirm `TTYD_CRED` and `CODE_SERVER_PASSWORD` are injected at spawn time. This was raised in iter 2 (line 194) and remains unresolved. Without these, ttyd and code-server will start with empty credentials.

---

## 10. RECOMMENDATION

**Apply B17 and B18 fixes (2-line change to Dockerfile).** Re-run iter 4. Expected to find 0-1 cosmetic issues. **WP-02 should reach PASS in iter 4** assuming B19 (SHA placeholders) is resolved externally.

**Do NOT proceed to WP-03+ review until WP-02 reaches PASS.**

---

## NEXT STEPS

1. **Apply B17 fix** — add defaults to bin-downloader ARG declarations
2. **Apply B18 fix** — regenerate Cargo.lock after sed (use `cargo update --workspace` or `cargo generate-lockfile`)
3. **B19 stays as operator action** — flag in PR description
4. **Re-run iter 4 review** — expect 0-1 new issues
5. **WP-02 reaches PASS** in iter 4 (predicted)

---

**END OF WP-02 ITERATION 3 REVIEW**
