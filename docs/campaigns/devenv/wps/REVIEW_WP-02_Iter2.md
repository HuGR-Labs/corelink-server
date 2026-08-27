# WP-02 REVIEW — Iteration 2 (DEEPER AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Previous Verdict:** ❌ FAIL (Iter 1: 12 BLOCKING, 9 HIGH, 8 MEDIUM)  
**New Verdict:** ❌ **FAIL — 4 NEW BLOCKING ISSUES, 4 NEW HIGH SEVERITY ISSUES, 2 NEW MEDIUM SEVERITY ISSUES**

---

## 🔴 NEW BLOCKING ISSUES (Missed in Iteration 1 / Regressions)

### B13. **`cargo metadata` Will Fail — 3 Test-Only Workspace Members Missing from Build Context**
- **Location:** Dockerfile `clw-builder` stage, line 113
- **Code:**
  ```dockerfile
  FIRST_PARTY="$(cargo metadata --no-deps --format-version 1 --locked | jq -r '.packages[].name')"
  ```
- **Problem:** The workspace `corelink-workspaces/Cargo.toml:2-16` declares **12 members** including 3 test-only members (`clw-integration-tests`, `clw-conformance`, `clw-e2e-evidence`). The Dockerfile only COPYs 9 crates (cli, types, cache, chunk, manifest, client, snapshot, hydrate, run) — the 3 test crates are NOT in the build context. `cargo metadata --no-deps` enumerates **all** workspace members by reading their manifests from disk; with 3 manifests missing, it errors with `error: failed to load manifest for package clw-integration-tests` and the build halts BEFORE reaching `cargo clean`. This is a **regression of the original B1 bug** — the iter 1 fix only addressed the direct dependency closure, not the workspace-resolver enumeration.
- **Why it slipped through:** Iter 1 added 9 COPY lines for the dep closure but didn't notice the workspace resolver iterates ALL 12 members.
- **Fix:** Either:
  - (a) COPY all 12 crates (including the 3 test-only) into the build context (simple, but pulls dev-deps like `wiremock` into the registry cache, ~50MB).
  - (b) `RUN sed -i '/clw-integration-tests\|clw-conformance\|clw-e2e-evidence/d' Cargo.toml` to remove test members from the COPY'd manifest before `cargo metadata` (clean, but mutates an upstream file).
  - (c) Skip `cargo metadata` entirely and hard-code the first-party list: `for pkg in clw-cli clw-types clw-cache clw-chunk clw-manifest clw-client clw-snapshot clw-hydrate clw-run; do cargo clean -p $pkg; done`. Safest for reproducibility (no jq parse, no `cargo metadata` round-trip).

### B14. **`ENTRYPOINT ["/entrypoint.sh"]` But Dockerfile Does NOT COPY `entrypoint.sh` — Container Will Fail to Start**
- **Location:** Dockerfile line 271
- **Code:** `ENTRYPOINT ["/entrypoint.sh"]`
- **Problem:** This is the **same H9 from iter 1, marked unfixed**. The Dockerfile references `/entrypoint.sh` but does NOT `COPY entrypoint.sh /entrypoint.sh` (and does NOT `COPY supervisord.conf /etc/supervisor/conf.d/supervisord.conf`). When the container starts, the kernel exec's `/entrypoint.sh` which doesn't exist → exec format error or "no such file or directory" → container exits with code 127 before any process starts. WP-03 lists `entrypoint.sh` and `supervisord.conf` as in-scope artifacts but its "Out of Scope" section (line 30-34) says only "clw binary build → WP-02" — it does NOT add the COPY lines to the Dockerfile. There is **no WP in this campaign that adds these COPY lines**.
- **Why it slipped through:** Iter 1 flagged H9 as a cross-WP coordination issue, but the WP-02 file at hand still has the ENTRYPOINT without the COPY. The fix was deferred to WP-03 which has not yet been written.
- **Fix:** Add to WP-02's runtime stage (between the `COPY --from=bin-downloader` and `RUN chmod` lines):
  ```dockerfile
  # ── Cross-WP contract: entrypoint.sh and supervisord.conf are OWNED by WP-03 ──
  # WP-03 must provide these files at the Dockerfile build context root. The COPY
  # here MUST stay (B14 — ENTRYPOINT requires the file to exist in the image).
  COPY entrypoint.sh /entrypoint.sh
  COPY supervisord.conf /etc/supervisor/conf.d/supervisord.conf
  RUN chmod +x /entrypoint.sh
  ```
  Add a corresponding item to the Completeness Checklist and DoD: "WP-03 provides `entrypoint.sh` and `supervisord.conf` at the Dockerfile build context root (not in subdirectory)".

### B15. **`verify-image.sh` Will Fail — `docker run` Invokes ENTRYPOINT, Not The Binary**
- **Location:** `verify-image.sh` lines 337, 340-345, 348
- **Code:**
  ```bash
  docker run --rm "$IMAGE_TAG" clw --version
  for bin in chromium xvfb-run x11vnc websockify ttyd code-server supervisord clw; do
    docker run --rm "$IMAGE_TAG" which "$bin" || ...
  done
  ```
- **Problem:** The image's `ENTRYPOINT` is `["/entrypoint.sh"]`. `docker run` appends the CMD after the ENTRYPOINT, so the actual exec is `/entrypoint.sh clw --version` or `/entrypoint.sh which chromium`. Per WP-03, `entrypoint.sh` is bash and its first action is `validate_env`, which REQUIRES `CLW_TENANT` and `CLW_TOKEN` (no defaults; line 102-103 of WP-03) and calls `exit 1` if missing. The verify script runs with no env vars → every `docker run` exits non-zero → script fails BEFORE checking the binaries it claims to check. Even after B14 is fixed (entrypoint exists), the script still won't work because it doesn't pass the required env.
- **Why it slipped through:** Iter 1 added reproducibility test (H7) and buildx (H8) but didn't re-examine the rest of the script.
- **Fix:** Use `--entrypoint=""` to bypass the image ENTRYPOINT and run the command directly via the shell:
  ```bash
  docker run --rm --entrypoint="" "$IMAGE_TAG" /usr/local/bin/clw --version
  for bin in chromium xvfb-run x11vnc websockify ttyd code-server supervisord clw; do
    docker run --rm --entrypoint="" "$IMAGE_TAG" which "$bin" || ...
  done
  docker run --rm --entrypoint="" "$IMAGE_TAG" id coder
  docker run --rm --entrypoint="" "$IMAGE_TAG" test -d /data/chrome && test -d /data/workspace
  ```
  Alternatively, mount the binaries into a separate scratch image — but `--entrypoint=""` is the standard fix.

### B16. **`/tmp/runtime-coder` Directory Required by Chromium But Not Created in Image**
- **Location:** Dockerfile runtime stage (missing) vs WP-03 supervisord.conf line 456
- **Code (WP-03):** `environment=DISPLAY=":99",HOME="/home/coder",XDG_RUNTIME_DIR="/tmp/runtime-coder"` (in the `[program:chromium]` block)
- **Problem:** Chromium on Linux REQUIRES `XDG_RUNTIME_DIR` to point to a directory it owns with mode 0700, otherwise it prints `NSS_InitWithProperties: cannot initialize NSS` and may refuse to start (or worse, leaks state to `/tmp`). WP-03 sets the env var, but **WP-02's Dockerfile does NOT `mkdir -p /tmp/runtime-coder && chown coder:coder /tmp/runtime-coder && chmod 0700 /tmp/runtime-coder`**. The image's `/tmp` is owned by root with mode 1777 (sticky world-writable). When `chromium` runs as `coder` and tries to use `/tmp/runtime-coder`, the directory doesn't exist → Chromium creates it with the wrong ownership/permissions (typically `coder coder 0755`) → security warning + potential failure.
- **Why it slipped through:** WP-02's iter 1 review didn't know what WP-03's supervisord would set as env (WP-03 was written in parallel). Cross-WP gap.
- **Fix:** Add to WP-02's runtime stage (after the `/data` mkdir block, before `USER coder`):
  ```dockerfile
  # XDG_RUNTIME_DIR required by chromium (WP-03 sets env var). Must be owned by
  # `coder` with mode 0700; chromium will reject other perms at startup.
  RUN mkdir -p /tmp/runtime-coder \
   && chown coder:coder /tmp/runtime-coder \
   && chmod 0700 /tmp/runtime-coder
  ```
  Document the contract: WP-03's `XDG_RUNTIME_DIR=/tmp/runtime-coder` matches this path. If WP-03 ever changes the path, both files must update in lockstep.

---

## 🟠 NEW HIGH SEVERITY ISSUES

### H10. **`cargo clean --release --locked $CLEAN_ARGS` Will Fail for Test-Only Members**
- **Location:** Dockerfile line 118
- **Code:** `cargo clean --release --locked $CLEAN_ARGS;`
- **Problem:** Same root cause as B13. `CLEAN_ARGS` is built from `cargo metadata` which (as established) fails. Even if B13 is fixed by hard-coding the list, if option (a) "COPY all 12 crates" is chosen, the dev-dependencies of the 3 test crates (`wiremock`, `tempfile`, `assert_cmd`, `async-trait`, `proptest`) need to resolve from crates.io — which works in a network-enabled build, but: `wiremock` 0.6+ has its own dependency closure that adds ~30 crates and ~200MB to the registry cache. The image is built in CI without `sccache`; the extra registry fetch adds 60-90s. Acceptable but worth noting. **However:** if option (b) "sed to remove from manifest" is chosen, `cargo clean` for the remaining 9 packages works correctly. The fix is to choose option (b) or (c), not (a).
- **Fix:** See B13. Use option (c) (hard-coded list) to avoid `cargo metadata` entirely.

### H11. **No Lock Between `ttyd`/`code-server` SHA256 ARGs and Verify Script Versions**
- **Location:** Dockerfile lines 66-67, 134-135, vs verify-image.sh lines 313-314
- **Code:**
  ```dockerfile
  ARG TTYD_VERSION=1.7.7
  ARG CODE_SERVER_VERSION=4.96.4
  ...
  ARG TTYD_SHA256=fb6e4987f4b1de8e9522f8f6c4f6a7b7a5b8e8b7c8d8b8b8b8b8b8b8b8b8b8b
  ARG CODE_SERVER_SHA256=0000000000000000000000000000000000000000000000000000000000000000
  ```
  vs
  ```bash
  TTYD_VERSION="${TTYD_VERSION:-1.7.7}"
  CODE_SERVER_VERSION="${CODE_SERVER_VERSION:-4.96.4}"
  ```
- **Problem:** The Dockerfile has 4 separate ARGs (TTYD_VERSION, CODE_SERVER_VERSION, TTYD_SHA256, CODE_SERVER_SHA256). The verify script passes only the 2 VERSION ARGs and assumes the SHA256 ARGs are unchanged. This creates a hidden coupling: a developer could update the SHA256s in the Dockerfile (correct) but forget to update verify-image.sh (silent — verify still works) OR a developer could update the versions in the verify script (correct) but not the SHA256s (silent mismatch — verify uses new version, old SHA256, build fails). The proper fix is to have verify-image.sh source these from a single source of truth.
- **Why it slipped through:** Iter 1 introduced the SHA256 ARGs (B3 fix) but didn't tie them to the verify script.
- **Fix:** Either (a) document a `versions.env` file with all 4 values sourced by both, or (b) inline a comment in the verify script pointing at the Dockerfile lines, or (c) have verify-image.sh `grep` the Dockerfile for the 4 ARGs and `eval` them. Recommended: add a `versions.env` to the build context and `COPY` it in Dockerfile (with `--from=bin-downloader` consumption in a multi-stage way). At minimum, add a `# Keep in sync with ARGs above` comment in verify-image.sh.

### H12. **WP-02 Does Not Pin `deny.toml` — But The Workspace Has One**
- **Location:** `.dockerignore` (does not exclude `deny.toml`)
- **Code:** `.dockerignore` excludes `*.toml` (was the iter 1 fix H2 — now removed in iter 2), so `deny.toml` is now NOT excluded.
- **Problem:** `corelink-workspaces/deny.toml` is in the workspace root. Iter 1's `.dockerignore` had `*.toml` blanket exclusion with `!Cargo.toml`/`!Cargo.lock` negations. Iter 2 removed `*.toml` entirely (now relies on explicit `!` negations). But `deny.toml` is now implicitly allowed → gets COPYed into the build context as `/build/deny.toml`. It's not USED by `cargo build` (cargo-deny is a separate tool), so it doesn't break the build. BUT it adds bytes to the build context (small, ~2KB) and pollutes the build dir. More importantly: if a future developer adds `cargo deny check` as a build step (e.g., in CI), the presence of `deny.toml` is fine — but its *content* (license allowlist) might reference crates not in the build context, causing false positives. The bigger problem: **`rust-toolchain.toml` is at the workspace root and was iter 1's H2 fix** — let me verify it's still in the negation list.
- **Verification:** The current `.dockerignore` lines 286-288: `!Cargo.toml`, `!Cargo.lock`, `!rust-toolchain.toml`. ✓ `rust-toolchain.toml` IS allowed. **H2 from iter 1 is FIXED in iter 2.** Downgrade to MEDIUM (cosmetic — `deny.toml` leak is harmless).

### H13. **`ttyd 1.7.7` and `code-server 4.96.4` Versions May Not Exist — Build Will Fail**
- **Location:** Dockerfile lines 66-67
- **Code:** `ARG TTYD_VERSION=1.7.7` and `ARG CODE_SERVER_VERSION=4.96.4`
- **Problem:** Verified against GitHub releases: ttyd's latest release as of 2026-08 is 1.7.7 (✓ exists), but code-server 4.96.4 is suspect — code-server uses **semantic dating** with releases like `v4.96.4` (which is plausible), but the actual filename is `code-server-4.96.4-linux-amd64.tar.gz` (verified URL pattern). The bigger concern: **without the SHA256 ARGs being real values, the build will fail at `sha256sum -c`** (line 144 and 147) because the placeholder hashes don't match the real binaries. The placeholders (`fb6e4987...` for ttyd, all-zeros for code-server) were noted in iter 1 as "DO NOT BUILD until real digests are pinned" (line 417 of WP-02). Iter 1 also noted this in the Fix Priority list. **This is a known blocker for BUILD, but the WP doesn't flag it as a DoD blocker — it says "passes on clean machine" which is FALSE with placeholder SHAs.**
- **Why it slipped through:** Iter 1 added the SHA256 placeholders but the DoD still says the build passes. There's a logical disconnect.
- **Fix:** Update DoD item #1 to "Dockerfile builds without errors **GIVEN that real SHA256 digests are pinned**" OR add a new DoD item: "`ttyd` and `code-server` SHA256 digests are pinned to real upstream values (not placeholders)".

---

## 🟡 NEW MEDIUM SEVERITY ISSUES

### M9. **No `HEALTHCHECK` for `ttyd` (Port 7681) or `code-server` (Port 8080)**
- **Location:** Dockerfile line 243-244
- **Code:** `HEALTHCHECK --interval=10s --timeout=3s --start-period=60s --retries=5 CMD curl -fsS http://localhost:6080/vnc_lite.html || exit 1`
- **Problem:** HEALTHCHECK only probes port 6080 (noVNC). If ttyd (7681) or code-server (8080) crash while noVNC stays up, the container is reported healthy but 2 of 3 user-facing services are down. CF Containers' `requiredPorts` (per WP-01, `[6080, 7681, 8080]`) does TCP-only liveness — it doesn't run the HEALTHCHECK. The HEALTHCHECK should probe all 3 ports or be a compound check.
- **Fix:** Either (a) expand the HEALTHCHECK to probe all 3: `CMD curl -fsS http://localhost:6080/vnc_lite.html && curl -fsS http://localhost:7681/ && curl -fsS http://localhost:8080/healthz` (note: ttyd and code-server may not respond to plain HTTP GETs without proper headers; `ttyd` upgrades to WebSocket on any path, `code-server` returns 302 to `/login`), or (b) add a `healthcheck.sh` script that probes all 3. Simplest: TCP probe on 3 ports: `CMD bash -c 'for p in 6080 7681 8080; do timeout 2 bash -c "</dev/tcp/localhost/$p" || exit 1; done'`.

### M10. **`EXPOSE 7681` for `ttyd` but ttyd Uses WebSocket Upgrade, Not Plain HTTP**
- **Location:** Dockerfile line 264
- **Code:** `EXPOSE 6080/tcp 7681/tcp 8080/tcp`
- **Problem:** `EXPOSE` is documentation only (no runtime effect in Docker; only `docker run -p` cares). But the comment in WP-02 says "7681: ttyd (terminal)" — fine, but the verify-image.sh check on line 351 verifies all 3 are exposed. ✓ OK. **No real issue**, but worth a note: ttyd is a WebSocket-only service; a plain TCP probe to 7681 will succeed even if ttyd's internal state is broken. Same as M9.

---

## 🔍 RE-VERIFICATION OF ITERATION 1 FIXES

Checking that the fixes from Iteration 1 actually work (or were applied):

| Fix | Status | Evidence |
|-----|--------|----------|
| B1: All 8+ clw-* crates in dep closure | ⚠️ **PARTIAL — REGRESSION** | 9 crates COPYed correctly, but `cargo metadata` at line 113 now fails because workspace still has 3 test-only members (B13) |
| B2: Duplicate removed | ✅ Fixed | `clw-manifest` listed once |
| B3: ttyd + code-server via bin-downloader | ✅ Fixed | New stage 1.5; SHA256-pinned (placeholder values) |
| B4: Tag cross-link comment | ✅ Fixed | Lines 60-62 cross-reference `rust-toolchain.toml` |
| B5: code-server runtime deps | ✅ Fixed | 12 libs explicitly installed (libnss3, libatk1.0-0, etc.) |
| B6: vnc_lite.html symlink | ✅ Fixed | Line 237: `ln -sf ... vnc_lite.html ... index.html` |
| B7/B9: HEALTHCHECK | ✅ Fixed | Lines 243-244 |
| B8: USER after WORKDIR | ✅ Fixed | Line 247 (USER) after line 248 (WORKDIR) |
| B10: clw-target cache ID | ✅ Fixed | Line 111: `id=clw-target` |
| B11: Exec form for clw --version | ✅ Fixed | Line 223: `RUN ["/usr/local/bin/clw", "--version"]` |
| B12: PASSWORD_STORE removed | ✅ Fixed | Not in ENV list |
| H1: Apt version pins | ❌ **NOT APPLIED** | No `=version` pins; `apt-get install -y --no-install-recommends pkg` (latest) |
| H2: `rust-toolchain.toml` allowed in `.dockerignore` | ✅ Fixed | `.dockerignore` line 288: `!rust-toolchain.toml` |
| H3: WORKDIR /build comment | ❌ **NOT APPLIED** | No cross-WP comment on `/build` collision |
| H4: `/tcp` suffix on EXPOSE | ✅ Fixed | Line 264: `6080/tcp 7681/tcp 8080/tcp` |
| H5: OCI LABELS | ✅ Fixed | Lines 160-164 |
| H6: STOPSIGNAL | ✅ Fixed | Line 249: `STOPSIGNAL SIGTERM` |
| H7: Reproducibility check | ✅ Fixed | Lines 316-333 (two builds + digest diff) |
| H8: docker buildx | ✅ Fixed | Lines 317, 323 use `docker buildx build` |
| H9: COPY entrypoint.sh | ❌ **NOT APPLIED — REGRESSION** | Still missing; ENTRYPOINT references file not in image (B14) |
| M1: `**/target` | ✅ Fixed | Line 291: `**/target` |
| M2: `/data` overlay-mount comment | ❌ **NOT APPLIED** | No comment explaining ephemeral mount semantics |
| M3: Drop `fonts-dejavu-core` | ✅ Fixed | Not in package list |
| M4: `DEBIAN_FRONTEND=noninteractive` ARG | ✅ Fixed | Line 65 |
| M6: PASSWORD_STORE | ✅ Fixed | (Same as B12) |
| M7: Defensive mkdir in entrypoint | ❌ **NOT APPLICABLE** | Out of WP-02 scope (entrypoint is WP-03) |
| M8: `chmod +x` on verify-image.sh | ❌ **NOT APPLIED** | No `chmod +x` instruction in WP; the WP says "and executable" in checklist but doesn't provide the command |

**Regression found:** B1 was "fixed" but introduced B13 (workspace-resolver iteration). H9 was "fixed" via WP-03 cross-WP coordination but the WP-02 file itself still has the bug.

**4 iter 1 fixes unapplied:** H1, H3, H9, M2, M8. (5 actually, depending on M7 scope.)

---

## 🔍 CROSS-WP CONSISTENCY CHECK

### WP-01 ↔ WP-02
- **defaultPort = 6080** ✓ matches WP-02's noVNC port and HEALTHCHECK
- **requiredPorts = [6080, 7681, 8080]** ✓ matches WP-02's EXPOSE
- **STATIC_ENV_VARS.CLW_REF_DOMAIN = "runner"** ✓ matches WP-02's ENV block (line 254 comment references "CLW_REF_DOMAIN at spawn time")
- **envVars shape** (CLW_TENANT, CLW_TOKEN, WORKSPACE_NAME, PROFILE_NAME, CLW_ENDPOINT) ✓ all match WP-03's entrypoint.sh reads
- **containerHandle** (set in onStart, not in Dockerfile) ✓ WP-02 doesn't set it
- **No `defaultPort` value mismatch** ✓

### WP-02 ↔ WP-03
- **`/data/chrome` and `/data/workspace`** ✓ Both WPs use these paths
- **CLW binary at `/usr/local/bin/clw`** ✓ Both WPs reference this path
- **Ports 6080, 7681, 8080** ✓ Both WPs agree (WP-03 also adds 5900 for VNC, 9222 for chromium remote debugging — internal, not in EXPOSE)
- **`XDG_RUNTIME_DIR=/tmp/runtime-coder`** ❌ **GAP** (B16) — WP-03 uses it, WP-02 doesn't create the dir
- **`/entrypoint.sh` ENTRYPOINT** ❌ **GAP** (B14) — WP-02 references, WP-03 owns the file, neither COPYs it
- **`/etc/supervisor/conf.d/supervisord.conf`** ❌ **GAP** — same as B14
- **`ttyd -c ${TTYD_CRED}`** — WP-03 expects `TTYD_CRED` env var; WP-01's `STATIC_ENV_VARS` does NOT include `TTYD_CRED` (and shouldn't, it's per-tenant). WP-01's mutable envVars spread doesn't show TTYD_CRED — is this injected at spawn? **Need to verify WP-01 covers this.** Looking at WP-01 line 336-342: the mutable envVars only set CLW_TENANT, CLW_TOKEN, WORKSPACE_NAME, PROFILE_NAME. TTYD_CRED and CODE_SERVER_PASSWORD are MISSING. **NEW CROSS-WP FINDING** — flag in findings.

### WP-02 ↔ WP-04 (clw Integration)
- **`/usr/local/bin/clw`** ✓
- **`--ref-domain runner`** — WP-03's entrypoint uses `--ref-domain "${CLW_REF_DOMAIN}"` which WP-01 injects as "runner". ✓

### NEW Cross-WP Issue (logged below as B17 in iter 3, but flagging here)

---

## 🔍 INTERNAL CONSISTENCY CHECK (WP-02 alone)

### Section 3.2 vs Section 3.4 (Dockerfile vs verify script)
- ✅ Both reference the same ports (6080, 7681, 8080)
- ❌ verify script tests `xvfb-run` but Dockerfile doesn't directly install it (xvfb package provides it, but the test should be on `Xvfb` which WP-03 actually uses)
- ❌ verify script invokes `clw --version` via ENTRYPOINT (B15)

### Section 3.2 vs Section 5 (Dockerfile vs Invariants)
- **I7 ("clw dynamically linked to glibc")** — fix from iter 1 ✓ correct now
- **I8 (SHA256-verified binaries)** — partially (placeholders, see H13)
- **I9 (`chmod 0755`)** — ✓ enforced at line 220

### Section 7 (Completeness) vs Section 4 (DoD)
- DoD #1: "Dockerfile builds without errors" — FALSE with placeholder SHAs and missing `entrypoint.sh` COPY
- DoD #9: "Build is reproducible" — verified by verify-image.sh (which itself is broken, B15)
- DoD #11: "verify-image.sh script passes" — FALSE (B15 + missing binaries)

---

## 📊 UPDATED SCORECARD

| Category | Iter 1 | Iter 2 | Trend |
|----------|--------|--------|-------|
| Blocking Issues | 12 → 0 | 4 NEW | ❌ Regression |
| High Issues | 9 → 0 | 4 NEW | ❌ Regression |
| Medium Issues | 8 → 0 | 2 NEW | ❌ Regression |
| DoD Pass Rate | 36% | 45% (5/11) | ↑ but still FAIL |
| Invariants Enforced | 62.5% | 75% (6/8) | ↑ |
| Quality Standards | 67% | 83% (5/6) | ↑ |
| Self-Checks | 53% | 60% (3/5) | ↑ |

**OVERALL VERDICT: ❌ FAIL — Iteration 2 found 4 BLOCKING regressions and 4 HIGH issues, primarily around the unaddressed `entrypoint.sh` COPY (H9 from iter 1) and new cross-WP gaps revealed by the parallel write of WP-03.**

---

## 🔧 FIXES NEEDED FOR ITERATION 2

### Must Fix (Blockers)
1. **B13**: Use option (c) — hard-code the first-party list in Dockerfile (no `cargo metadata`), OR option (b) — `sed` to remove test members from Cargo.toml before `cargo metadata`.
2. **B14**: Add `COPY entrypoint.sh /entrypoint.sh` + `COPY supervisord.conf /etc/supervisor/conf.d/supervisord.conf` + `RUN chmod +x /entrypoint.sh` to WP-02's runtime stage.
3. **B15**: Update `verify-image.sh` to use `--entrypoint=""` for all `docker run` invocations.
4. **B16**: Add `RUN mkdir -p /tmp/runtime-coder && chown coder:coder /tmp/runtime-coder && chmod 0700 /tmp/runtime-coder` to runtime stage.

### Should Fix (High)
1. **H10**: Tied to B13 fix — choose option (b) or (c) over (a).
2. **H11**: Document the Dockerfile ↔ verify-image.sh coupling for `ttyd`/`code-server` SHAs.
3. **H12**: Add `deny.toml` to `.dockerignore` to prevent build context bloat (cosmetic).
4. **H13**: Update DoD #1 to acknowledge SHA256 placeholder is a build-blocker until pinned.

### Nice to Fix (Medium)
1. **M9**: Expand HEALTHCHECK to probe all 3 ports (or use TCP probe).
2. **M10**: (No real issue — informational.)

### Cross-WP Coordination Required
- **WP-01 ↔ WP-03** (new finding): WP-03's `ttyd -c ${TTYD_CRED}` and `code-server` `PASSWORD=${CODE_SERVER_PASSWORD}` require per-tenant env vars that are NOT in WP-01's `STATIC_ENV_VARS` (immutable) or the mutable envVars spread (line 336-342 of WP-01). WP-05 (WebSocket proxy) likely owns generating these, but WP-01 must be updated to inject them into `super.start({envVars})`. This is out of scope for WP-02 fix but must be raised to WP-01 owner.
- **WP-02 ↔ WP-03**: B14 (entrypoint.sh COPY) — WP-02 must own the COPY since the ENTRYPOINT is in WP-02. WP-03 must OWN the file content. Document the contract.
- **WP-02 ↔ WP-03**: B16 (`/tmp/runtime-coder`) — WP-02 creates the dir with correct perms; WP-03 sets the env var. Both must agree on the path.

---

## 📈 TREND ANALYSIS

| Metric | Iter 1 | Iter 2 |
|--------|--------|--------|
| New BLOCKING found | 12 | 4 |
| New HIGH found | 9 | 4 |
| New MEDIUM found | 8 | 2 |
| Fixed from previous | 0 | 19 |
| Net improvement | -29 | -10 (4B+4H+2M) |

**Trajectory:** Net improvement is good (29 → 10 new issues), but the new BLOCKINGs are CRITICAL regressions (B14 = container won't start, B15 = verify script lies, B16 = chromium may fail). **Convergence predicted:** Iter 3 will find 1-2 new BLOCKINGs (H1, H3, M2 still unapplied) + 0-1 cross-WP issues. Iter 3 should approach PASS.

---

## 🎯 RECOMMENDATION

**Apply all 4 BLOCKING fixes (B13-B16) + 4 HIGH fixes (H10-H13).** Re-run Iter 3. Expected to find 1-2 new issues (H1 apt version pins, H3 WORKDIR comment, M2 overlay-mount comment — all MEDIUM/cosmetic at this point).

**Do NOT proceed to WP-03 review until B14 is fixed** — the ENTRYPOINT ↔ file gap is the highest-priority regression.

---

## NEXT STEPS

1. **Apply all 4 BLOCKING fixes (B13-B16)** — non-negotiable
2. **Apply all 4 HIGH fixes (H10-H13)** — required for production build
3. **Apply M9 (HEALTHCHECK expansion)** — recommended
4. **Raise cross-WP finding to WP-01 owner**: TTYD_CRED / CODE_SERVER_PASSWORD missing from envVars spread
5. **Coordinate with WP-03 owner on entrypoint.sh file location** (B14 cross-WP contract)
6. **Proceed to Iteration 3 review**

**Do NOT proceed to WP-03+ review until WP-02 reaches PASS.**

---

**END OF WP-02 ITERATION 2 REVIEW**
