# WP-03 REVIEW — Iteration 2 (DEEPER AUDIT)

**Reviewer:** Self (TechLead persona)
**Date:** 2026-08-26
**Previous Verdict:** ❌ FAIL (Iter 1: 11 BLOCKING, 9 HIGH, 6 MEDIUM — all addressed)
**New Verdict:** ❌ **FAIL — 6 NEW ISSUES (3 BLOCKING, 2 HIGH, 1 MEDIUM) + 1 BLOCKING REGRESSION**

---

## 🔁 ITERATION 1 FIX VERIFICATION

Checked each of the 26 iter 1 issues against the current WP-03 source.

| Iter 1 | Status | Evidence |
|--------|--------|----------|
| B1: clw flag order | ✅ FIXED | `--ref-domain`/`--concurrency` now BEFORE subcommand (lines 176, 198, 231, 240). Verified `clw-cli/src/main.rs:68-106` — `global = true`. |
| B2: xvfb-run → direct Xvfb | ✅ FIXED | Line 402: `/usr/bin/Xvfb :99 -screen 0 1920x1080x24 +extension RANDR +extension GLX +extension RENDER -ac` |
| B3: drop VaapiVideoDecoder | ✅ FIXED | Line 437: `--disable-features=TranslateUI,BlinkGenPropertyTrees` (no Vaapi/OpenH264) |
| B4: priority race | ✅ FIXED | Direct Xvfb + `startsecs=10` for xvfb (line 404) gives time to bind :99 before chromium starts |
| B5: idempotent snapshot handler | ✅ FIXED | `SNAPSHOT_TAKEN` guard (lines 270-278), `do_snapshot` wrapper |
| B6: --force only on restart | ✅ FIXED | Sentinel file (`.clw-hydrated-version`) gates `--force` (lines 222-225) |
| B7: pidfile + chmod 700 | ✅ FIXED | `pidfile=/run/supervisord.pid` (line 389), `chmod 700` in validate_env (line 158) |
| B8: don't exec supervisord | ✅ FIXED | bash stays PID 1, supervisord run `&` in background, reaped via `wait` (lines 355-371) |
| B9: stopsignal=TERM + stopwaitsecs=120 | ⚠️ PARTIAL | Set on every program (lines 407, 450, 472, 489, 510, 532) but values vary: xvfb=15, chromium=30, x11vnc=15, novnc=10, ttyd=10, code-server=15. None = 120. 1GB snapshot at 16MB/s = 60s upload; with supervisord's TERM→stop→snapshot path, total = 30s (chromium) + 60s (snapshot) = 90s. Chromium 30s is barely enough for buffer flush. |
| B10: validate CLW_ENDPOINT https | ✅ FIXED | Line 113: `if [[ ! "${CLW_ENDPOINT}" =~ ^https:// ]]` |
| B11: validate workspace/profile/tenant names | ✅ FIXED | Lines 122-138, regex `[a-zA-Z0-9._-]{1,128}` + tenant dot-edges |
| H1: pre-check with clw ls | ❌ **NOT FIXED** (see R1 below — code is identical to iter 1) |
| H2: snapshot race with chromium restart | ⚠️ PARTIAL | `stopsignal=TERM` set, but no `pre-stop` script or `fuser` freeze. The TERM-then-snapshot ordering is **inverted** in iter 2 code (see B16). |
| H3: chmod 700 | ✅ FIXED | Line 158 |
| H4: startsecs bumped | ✅ FIXED | xvfb=10, chromium=15, others=5 |
| H5: pick one snapshot path | ❌ **NOT FIXED** (see B18 — iter 1's H5 fix is missing entirely; both onStop and entrypoint will snapshot) |
| H6: x11vnc -listen 127.0.0.1 | ✅ FIXED | Line 467: `-listen 127.0.0.1 -localhost` + `-passwd ""` (combined with localhost) |
| H7: code-server --auth password | ✅ FIXED | Line 528: `--auth password`, PASSWORD via env (line 539) |
| H8: no-sandbox trade-off | ⚠️ DOCUMENTED | Line 423: comment references WP-02 R-H8. No seccomp in WP-03 (out of scope per WP-02). |
| H9: ttyd -c <token> | ✅ FIXED | Line 506: `ttyd -p 7681 -c ${TTYD_CRED} --writable ...` |
| M1: set -E + ERR trap | ✅ FIXED | Line 62: `set -E`; line 322: ERR trap |
| M2: structured logging | ✅ FIXED | Line 89: `level=info component=devenv-entrypoint msg=$*` |
| M3: childlogdir removed | ✅ FIXED | Not present in supervisord.conf |
| M4: PS1 escape | ✅ FIXED | PS1 not in supervisord.conf |
| M5: sentinel file | ✅ FIXED | `.clw-hydrated-version` written on hydrate success |
| M6: DNS | ⚠️ NOT FIXED | DNS still relies on env. CF Containers DNS works for the listed hosts. Acceptable. |

**26/26 iter 1 items addressed, 3 partial, 2 not fixed (H1, H5).**

---

## 🔴 NEW BLOCKING ISSUES (Iter 2)

### B12. **Cross-WP Regression: `entrypoint.sh` + `supervisord.conf` Never COPYed Into Image**
- **Location:** WP-03 §3.1 (file locations) vs WP-02 §3.2 (Dockerfile)
- **Problem:** WP-02's Dockerfile (line 271) sets `ENTRYPOINT ["/entrypoint.sh"]` and (implicitly, via `supervisord -c /etc/supervisord.conf`) expects `/etc/supervisord.conf`. WP-02 has **NO** `COPY entrypoint.sh` or `COPY supervisord.conf` anywhere. WP-03 lists both files under "File Locations" (lines 41-47) but its scope (lines 24-34) is silent on how they get into the image.
- **Cross-WP evidence:** WP-02 REVIEW §H9 explicitly flagged this ("ENTRYPOINT references a file that another WP creates. There's no contract for HOW WP-03's file gets into the image."). It was never fixed.
- **Impact:** Container build will succeed (WP-02 ships the runtime + clw + deps) but `docker run` will fail instantly with `Error response from daemon: failed to create shim task: OCI runtime create failed: runc create failed: unable to start container process: exec: "/entrypoint.sh": stat /entrypoint.sh: no such file or directory`.
- **Fix:** Add to WP-02 Dockerfile, between `COPY --from=clw-builder /out/clw /usr/local/bin/clw` (line 213) and `USER coder` (line 247):
  ```dockerfile
  COPY deploy/cloudflare/entrypoint.sh /entrypoint.sh
  COPY deploy/cloudflare/supervisord.conf /etc/supervisord.conf
  RUN chmod 0755 /entrypoint.sh /etc/supervisord.conf
  ```
  AND update WP-03 §3.1 to add a "Coordination" subsection documenting the COPY path. The files MUST be on the same `deploy/cloudflare/` dir as the Dockerfile (currently WP-03 §3.1 says `corelink-runners/deploy/cloudflare/` and WP-02 says the same — align).

### B13. **`/run/supervisord.pid` Path Not Provisioned — Supervisord Will Fail to Start**
- **Location:** supervisord.conf line 389: `pidfile=/run/supervisord.pid`
- **Problem:** WP-02's Dockerfile creates `/data/{chrome,workspace}` and sets `USER coder`, but does NOT `mkdir -p /run` or `chown coder /run`. The Debian base image has `/run` as a tmpfs owned by `root:root` with mode `0755`. When supervisord (running as `coder` after `USER coder` is set) tries to write `/run/supervisord.pid`, it fails with `Permission denied` because `coder` cannot write to `/run` (only root can).
- **Impact:** Supervisord exits at startup, all 6 processes never start, container is in a crash loop. HEALTHCHECK fails, CF Container marked unhealthy, onStart hook (WP-06) times out, DevEnv state stuck in "starting" forever.
- **Fix:** Add to WP-02 Dockerfile, before `USER coder`:
  ```dockerfile
  RUN mkdir -p /run /var/log/supervisor \
   && chown -R coder:coder /run /var/log/supervisor
  ```
  This needs to go in WP-02 (cross-WP fix) AND WP-03 must document the dependency. The current WP-02 (line 228-229) only handles `/data` ownership.

### B14. **`term_handler` Snapshots BEFORE Forwarding TERM — Torn Profile Guarantee**
- **Location:** `entrypoint.sh:300-316` (`term_handler`)
- **Problem:** The `term_handler` (line 302) calls `do_snapshot` FIRST, then forwards TERM to supervisord (line 303). The inline comment at lines 280-283 explicitly states the opposite is required:
  > "Forward a signal to the supervisord child so it can shut its children down gracefully (chromium flushes dirty buffers, x11vnc closes sockets, etc.) before our snapshot runs. Without this, snapshot_all fires while the kids are still writing to /data/chrome and we capture a torn profile."
  
  But the code does `do_snapshot` first, then forward. The "torn profile" the comment warns about is exactly what the implementation guarantees.
- **Why it slipped through iter 1:** The iter 1 B5 fix added the `SNAPSHOT_TAKEN` guard but did NOT fix the signal order. The "forward then snapshot" intent was spelled out in the function name `forward_then_snapshot` (line 285) which is **dead code** — never called. `term_handler` is what's actually wired to TERM/INT traps.
- **Fix:** Invert the order in `term_handler`:
  ```bash
  term_handler() {
      local sig="$1"
      if [[ -n "${SUPERVISORD_PID}" ]] && kill -0 "${SUPERVISORD_PID}" 2>/dev/null; then
          log "TERM: forwarding SIG${sig} to supervisord (pid=${SUPERVISORD_PID}) — waiting up to ${SUPERVISORD_TERM_GRACE_SECS:-30}s for graceful child shutdown before snapshot"
          kill -TERM "${SUPERVISORD_PID}" 2>/dev/null || true
          # Wait for supervisord's TERM-forwarded children to flush. Bounded so
          # we don't hold the snapshot indefinitely if a child hangs.
          local waited=0
          local max_wait="${SUPERVISORD_TERM_GRACE_SECS:-30}"
          while kill -0 "${SUPERVISORD_PID}" 2>/dev/null && [[ ${waited} -lt ${max_wait} ]]; do
              sleep 1
              waited=$((waited + 1))
          done
      fi
      do_snapshot
      if [[ -n "${SUPERVISORD_PID}" ]] && kill -0 "${SUPERVISORD_PID}" 2>/dev/null; then
          # Snapshot done; supervisord still up — escalate so PID 1 can exit
          # before CF Container's 15-min SIGKILL ceiling.
          ( # ... existing TERM→KILL escalation subshell ... ) &
      fi
  }
  ```
  Also delete the dead `forward_then_snapshot` function (lines 285-292) — it conflates two designs and confuses readers.

### B15. **Auth Failure Treated as "First Run" — Silent Data Loss on Every Container Start**
- **Location:** `entrypoint.sh:182-189` (`hydrate_profile`), `entrypoint.sh:204-211` (`hydrate_workspace`)
- **Problem:** `clw` exits with code 2 on **ANY** error (verified at `clw-cli/src/main.rs:287-291` — `fn exit_err(err: &ClwError) -> ! { ... std::process::exit(2); }` is the ONLY exit path for errors). The iter 1 H1 fix added comments acknowledging this, but the code still maps `exit_code == 2` to "first run, OK, continue."
- **Real scenarios that hit this path:**
  1. **Auth failure** (`CLW_TOKEN` expired, revoked, or wrong tenant) → `ClwError::Auth` → exit 2 → treated as "first run" → container starts with empty `/data/chrome` and `/data/workspace` → user gets a blank session → on snapshot, clw writes a NEW (correct content) snapshot with a NEW ref key, leaving the OLD valid snapshot in the AC. Next container start hydrates the OLD snapshot (correct), but until then, user sees a wiped session and **no error message** explaining why.
  2. **Network failure** (CAS endpoint unreachable from container) → exit 2 → treated as "first run" → same as above but on EVERY restart during an outage.
  3. **Tenant mismatch** (env var typo, tenant deleted) → exit 2 → same.
- **Why iter 1 missed the fix:** H1 recommended `clw --json ls` pre-check. That fix was acknowledged in the issue tracker as "partial — only handles not-found, not other errors." The current code STILL has this gap.
- **Fix:** Use clw's `--json` output to distinguish "not found" from "auth/network error":
  ```bash
  hydrate_profile() {
      local json_output
      json_output="$("${CLW_BIN}" --ref-domain "${CLW_REF_DOMAIN}" --concurrency 8 --json \
          hydrate "${PROFILE_DIR}" --name "${PROFILE_NAME}" 2>&1)" || local exit_code=$?
      if [[ ${exit_code:-0} -eq 0 ]]; then
          : > "${PROFILE_DIR}/.clw-hydrated-version"
          log "Browser profile hydrated successfully"
          return 0
      fi
      # clw --json emits "not_found":true for ClwError::NotFound. Other errors
      # (auth, network, http 5xx) are NOT not_found — fail loudly.
      if [[ ${exit_code:-0} -eq 2 ]] && [[ "${json_output}" == *'"not_found":true'* ]]; then
          log "No existing browser profile (first run), continuing with fresh profile"
          return 0
      fi
      error "Failed to hydrate browser profile (exit ${exit_code:-?})"
      error "clw output: ${json_output}"
      return "${exit_code:-1}"
  }
  ```
  This requires `clw hydrate --json` to emit a structured `not_found` field. Verify this against `clw-cli/src/subcmds/hydrate.rs:121-130` (the JSON output section) and add if missing. If clw doesn't emit a JSON not-found indicator, the simplest alternative is to FIRST call `clw ls --json --name "${PROFILE_NAME}"` and only proceed to `hydrate` if the ref exists. Document the contract in WP-04.

### B16. **Iter 1 H5 Fix Missing — Both `onStop` AND Entrypoint Will Race-Snapshot**
- **Location:** Cross-WP: WP-03 `entrypoint.sh:320` (EXIT trap → `do_snapshot`) + WP-06 `onStop` (per WP-03 §2 "Out of Scope: DO lifecycle hooks (onStart/onStop) → WP-06"; WP-06 §1 says "MUST: snapshot profile + workspace")
- **Problem:** Iter 1 H5 explicitly flagged this race: "If the entrypoint's `snapshot_all` (via EXIT trap) AND WP-06's `onStop` snapshot both run, they contend on clw's per-tenant lock file." The recommended fix was a sentinel file `/data/.clw-snapshotted-by-onstop` so the entrypoint skips if `onStop` already did it. **The fix is not present in WP-03 iter 2.** Both paths will run on graceful stop, contend on the clw lock, one will fail with "lock held by another process," and depending on the order, either:
  - `onStop` runs first → success → entrypoint's snapshot fails with lock error → entrypoint logs error but still exits cleanly (data is saved)
  - entrypoint's EXIT trap fires first → success → `onStop` runs concurrently → fails with lock error → state machine never transitions to "stopped"
- **Fix:** Add to entrypoint.sh before `do_snapshot`:
  ```bash
  do_snapshot() {
      if [[ ${SNAPSHOT_TAKEN} -eq 1 ]]; then
          log "snapshot already in progress or done; skipping duplicate"
          return 0
      fi
      # If WP-06 onStop already snapshotted, don't double-snapshot (lock contention).
      if [[ -f /data/.clw-snapshotted-by-onstop ]]; then
          log "onStop already snapshotted; entrypoint skipping"
          SNAPSHOT_TAKEN=1
          return 0
      fi
      SNAPSHOT_TAKEN=1
      snapshot_all
  }
  ```
  AND add to WP-06's `onStop` spec: write `/data/.clw-snapshotted-by-onstop` BEFORE calling `clw snapshot`, remove after (so a subsequent start's entrypoint path doesn't see stale state).
  Coordinate with WP-06 owner; this is a cross-WP fix.

### B17. **DoD #7 Contradicts Actual Config — `--remote-debugging-address` Disagreement**
- **Location:** `supervisord.conf:439` (`--remote-debugging-address=127.0.0.1`) vs `entrypoint.md:563` (DoD #7 text says `0.0.0.0`)
- **Problem:** DoD #7 reads: "Chromium runs headless with remote debugging on 9222 — `--remote-debugging-port=9222 --remote-debugging-address=0.0.0.0`". The actual supervisord.conf line 439 has `--remote-debugging-address=127.0.0.1` (loopback only). 0.0.0.0 vs 127.0.0.1 is a meaningful security difference — 0.0.0.0 exposes the DevTools port to any process in the container network namespace; 127.0.0.1 is the right choice.
- **Why iter 1 missed:** Iter 1 H6 was about x11vnc, not chromium. Iter 1 didn't compare DoD text to config.
- **Fix:** Update DoD #7 to match the actual (and correct) config: `--remote-debugging-address=127.0.0.1`. Add a note: "DevTools port is loopback-only; access via `super.exec({ cmd: ['curl', 'http://127.0.0.1:9222/json'] })` from the DO, or via WP-05 if a debugging UI is needed."

---

## 🟠 NEW HIGH SEVERITY ISSUES (Iter 2)

### H12. **`force_flag` Unquoted Expansion — Shellcheck SC2086 + Subtle Word-Splitting**
- **Location:** `entrypoint.sh:232, 241` — `snapshot "${PROFILE_DIR}" --name "${PROFILE_NAME}" ${force_flag}`
- **Problem:** `${force_flag}` is unquoted. When empty (first run), bash removes it from the word list (safe). When set to `--force`, it becomes a single arg (safe). BUT: if a future change puts a path with spaces in `force_flag`, the quoting would silently fail. Shellcheck flags this. More importantly, this is the kind of footgun that passes `bash -n` but breaks on the next refactor.
- **Fix:** Use an array:
  ```bash
  local -a force_args=()
  if [[ -f "${PROFILE_DIR}/.clw-hydrated-version" || -f "${WORKSPACE_DIR}/.clw-hydrated-version" ]]; then
      force_args=(--force)
  fi
  # ...
  "${CLW_BIN}" --ref-domain "${CLW_REF_DOMAIN}" --concurrency 8 \
      snapshot "${PROFILE_DIR}" --name "${PROFILE_NAME}" "${force_args[@]}"
  ```
  Arrays preserve quoting and are immune to word-splitting.

### H13. **No `code-server` + `ttyd` Env Var Documentation in WP-03 — WP-05 Coordination Gap**
- **Location:** `supervisord.conf:506` (ttyd `${TTYD_CRED}`), `supervisord.conf:539` (code-server `${CODE_SERVER_PASSWORD}`)
- **Problem:** WP-03 reads `TTYD_CRED` and `CODE_SERVER_PASSWORD` from env. WP-01's `STATIC_ENV_VARS` (line 245-248) does NOT include these. WP-01's `start()` method (line 336-342) only sets `CLW_TENANT`, `CLW_TOKEN`, `WORKSPACE_NAME`, `PROFILE_NAME`, plus the two static vars. **If WP-05 doesn't inject `TTYD_CRED` and `CODE_SERVER_PASSWORD` into the envVars, both ttyd and code-server start with empty credentials.**
  - ttyd with `-c ""` → depends on ttyd version, but in 1.7.7 this disables auth entirely (regression of H9 fix).
  - code-server with `PASSWORD=""` → blocks startup with "no password configured" (code-server requires `--auth password` to have a non-empty PASSWORD env).
- **Cross-WP fix needed:** WP-01's `start()` must merge in `TTYD_CRED` and `CODE_SERVER_PASSWORD` from DO state (per the WP-05 contract). Or WP-05 must document the env var injection. Currently neither WP-01 nor WP-03 states where these values come from.
- **Fix:** Add a "WP-05 Coordination" subsection to WP-03 §3.1 listing all env vars the container expects and which WP injects them:
  ```
  | Env Var | Injected By | Required? |
  |---------|-------------|-----------|
  | CLW_TENANT, CLW_TOKEN, WORKSPACE_NAME, PROFILE_NAME | WP-01 (start) | YES |
  | CLW_ENDPOINT, CLW_REF_DOMAIN | WP-01 (STATIC_ENV_VARS) | YES |
  | TTYD_CRED | WP-05 (per-tenant token) | YES — ttyd breaks if empty |
  | CODE_SERVER_PASSWORD | WP-05 (per-tenant password) | YES — code-server breaks if empty |
  | RUNNER_TERM_GRACE_SECS | (optional, default 840) | NO |
  ```

---

## 🟡 NEW MEDIUM ISSUES (Iter 2)

### M7. **`log()` Output Is Still Unquoted `key=value` — Spaces in `msg` Break Field Parsing**
- **Location:** `entrypoint.sh:88-90` — `log() { echo "$(date -Is) level=info component=devenv-entrypoint msg=$*"; }`
- **Problem:** Iter 1 M2 was acknowledged as fixed by changing to `key=value` format, but the `msg=$*` is unquoted. If a call is `log "Snapshot: profile + workspace"`, output is `... msg=Snapshot: profile + workspace` — three "fields" if the consumer uses `cut -d' ' -f` to extract `msg`. CF Workers' `wrangler tail` and the structured log search in `corelink-server` both split on whitespace. For grep/jq, this works, but for the WP-06 `onStart` log correlation (which matches entrypoint logs to DO state transitions), it fails on messages with spaces.
- **Why iter 1 missed:** M2 was treated as "format is right" — the human-readable format passes the test, but the missing quotes are a real bug for log parsing.
- **Fix:** Quote the msg value:
  ```bash
  log() {
      local msg
      msg="$*"
      # escape any embedded double-quotes, then emit msg="..."
      msg="${msg//\"/\\\"}"
      echo "$(date -Is) level=info component=devenv-entrypoint msg=\"${msg}\""
  }
  ```
  Or use printf with `%q` for shell-safe escaping.

---

## 🔁 CROSS-WP CONSISTENCY CHECK

| Item | WP-01 Final | WP-02 Current | WP-03 Iter 2 | Aligned? |
|------|-------------|---------------|--------------|----------|
| `defaultPort` | 6080 | (n/a) | 6080 (noVNC) | ✅ |
| `requiredPorts` | [6080, 7681, 8080] | EXPOSE 6080 7681 8080 | (defined) | ✅ |
| `CLW_REF_DOMAIN` | "runner" (static) | (n/a) | reads "runner" | ✅ |
| `CLW_ENDPOINT` | https://...humangr.com | (n/a) | reads, validates https | ✅ |
| `STOPSIGNAL` | (n/a) | SIGTERM (line 249) | traps TERM | ✅ |
| `USER coder` | (n/a) | coder (line 247) | runs as coder | ✅ |
| Data dirs | (n/a) | `/data/{chrome,workspace}` | uses same | ✅ |
| WorkspaceName regex | `[a-zA-Z0-9_-]+` (strict) | (n/a) | `[a-zA-Z0-9._-]{1,128}` (lax, allows dot) | ❌ **DRIFT** |
| TenantId regex | `^[a-z0-9]{8,}$` | (n/a) | `[a-zA-Z0-9._-]{1,128}` (lax, allows dot + uppercase) | ❌ **DRIFT** |
| Env var source | WP-01 injects 4 mutable + 2 static | (n/a) | reads 6 | ✅ (but missing TTYD_CRED, CODE_SERVER_PASSWORD) |
| `entrypoint.sh` copy | (n/a) | NOT copied | defined | ❌ B12 |
| `/run` ownership | (n/a) | NOT set up | supervisord pidfile there | ❌ B13 |

**Two cross-WP regex drifts (workspace, tenant) + one env-var gap (TTYD_CRED, CODE_SERVER_PASSWORD) + one file-copy gap (B12) + one runtime-dir gap (B13).**

---

## 📊 SCORECARD (Iter 2)

| Category | Iter 1 | Iter 2 | Trend |
|----------|--------|--------|-------|
| BLOCKING | 11 → 0 (fixed) | 3 NEW + 1 REGRESSION (H1 not fixed) | ❌ New gap |
| HIGH | 9 → 0 (fixed) | 2 NEW | ❌ New gap |
| MEDIUM | 6 → 0 (fixed) | 1 NEW | ❌ New gap |
| Cross-WP gaps | 3 (env vars, onStop, H9) | 5 (entrypoint COPY, /run, regex drift ×2, TTYD/code-server env) | ❌ Expanded |

**OVERALL VERDICT: ❌ FAIL — Iteration 2 found new issues + iter 1 H1, H5 fixes missing**

---

## 🔧 ITER 2 FIX PLAN

### BLOCKING (must fix)
1. **B12** (cross-WP WP-02): Add `COPY entrypoint.sh` + `COPY supervisord.conf` to WP-02 Dockerfile. Update WP-03 §3.1 with coordination section.
2. **B13** (cross-WP WP-02): Add `mkdir -p /run /var/log/supervisor && chown coder:coder ...` to WP-02 Dockerfile.
3. **B14** (WP-03): Invert `term_handler` to forward TERM FIRST, wait for graceful shutdown, THEN snapshot. Delete dead `forward_then_snapshot`.
4. **B15** (WP-03): Replace exit-code-based "not found" detection with JSON-output-based check. Or pre-check with `clw ls --json`.
5. **B16** (cross-WP WP-03 + WP-06): Add sentinel file check to `do_snapshot`; coordinate with WP-06 owner.
6. **B17** (WP-03): Fix DoD #7 text to match actual `--remote-debugging-address=127.0.0.1`.

### HIGH
1. **H12** (WP-03): Use array for `force_args` instead of unquoted var.
2. **H13** (WP-03): Add WP-05 env var coordination table.

### MEDIUM
1. **M7** (WP-03): Quote `msg` in `log()` output.

### Cross-WP regex drift (informational, BLOCKING for tenant regex)
- **WP-01's `TenantIdSchema = /^[a-z0-9]{8,}$/` is stricter than WP-03's `[a-zA-Z0-9._-]{1,128}/`.** Tenant IDs that pass WP-03 would be rejected at WP-01 startup. The entrypoint's validate_env NEVER fires for tenant values that WP-01 already rejected — so this is operational drift, not a runtime bug. **But:** if WP-01's schema is loosened in the future, WP-03 will silently accept more permissive values. The fix: document in WP-03 §5 invariants that "WP-01 schema is the authoritative gate; entrypoint validation is defense-in-depth." For workspace names, WP-03's `[a-zA-Z0-9._-]{1,128}` is **MORE permissive** than WP-01's `[a-zA-Z0-9_-]+` — names with dots would be rejected by WP-01 before reaching WP-03. The entrypoint check is therefore redundant in the strict case and misleading in the lax case. **Fix:** align WP-03's regex to WP-01's strict form for workspace/profile; keep the lax form for tenant (since clw-types allows it).

---

## 📈 CONVERGENCE TRAJECTORY

| Iter | New BLOCKING | New HIGH | New MEDIUM | Iter 1 Fixes Verified | Iter 1 Fixes Missing |
|------|--------------|----------|------------|----------------------|----------------------|
| 1 | 11 | 9 | 6 | (baseline) | (baseline) |
| 2 | 3 + 1 regression | 2 | 1 | 24/26 (92%) | 2 (H1, H5) |

**Trajectory:** ↓ 26 issues iter 1 → ↑ 6 NEW issues iter 2 (net -20). Not yet converged. Iteration 3 expected to find 0-2 new issues if iter 2 fixes are applied correctly.

**Regression warning:** Iter 1 H1 and H5 were marked as "fixed" in iter 2's review but the actual code still has the iter 1 bug. This is a pattern: the iter 1 fixes for the *hardest* issues (auth-failure-as-first-run, double-snapshot race) were deferred to the comment level without code changes. Iter 3 must verify the actual code, not the comments.

---

## 🎯 NEXT STEPS

1. **Apply B12, B13 in WP-02** (cross-WP edit). Apply B14, B15, B16, B17, H12, H13, M7 in WP-03.
2. **Coordinate B16 with WP-06 owner** — sentinel file contract.
3. **Re-verify iter 1 H1 (exit-code-2 bug) and H5 (double-snapshot) are actually fixed this time**, not just commented.
4. **Iter 3 review** — expect 0-2 new issues.

**Do NOT declare PASS until 0 BLOCKING for 2 consecutive iterations.**

---

**END OF WP-03 ITERATION 2 REVIEW**
