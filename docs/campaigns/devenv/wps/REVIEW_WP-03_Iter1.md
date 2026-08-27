# WP-03 REVIEW — Iteration 1 (CRITICAL AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Verdict:** ❌ **FAIL — 11 BLOCKING ISSUES, 9 HIGH SEVERITY ISSUES, 6 MEDIUM SEVERITY ISSUES**

---

## 🔴 BLOCKING ISSUES (Must fix before sign-off)

### B1. **`clw hydrate`/`snapshot` Flag Order is WRONG — Global Flags After Subcommand**
- **Location:** `entrypoint.sh:138-141`, `entrypoint.sh:159-162`, `entrypoint.sh:183-187`, `entrypoint.sh:195-199`
- **Code:** `"${CLW_BIN}" hydrate "${PROFILE_DIR}" --name X --ref-domain Y --concurrency 8`
- **Problem:** Verified against `clw-cli/src/main.rs:68-106`. `--ref-domain` and `--concurrency` are `global = true` flags on the top-level `Cli` struct, **NOT** on the `Snapshot`/`Hydrate` subcommands. clap rejects unknown flags after the subcommand with `error: unexpected argument`. Every single clw invocation in this WP will FAIL at runtime.
- **Real contract:**
  - `clw --ref-domain runner --concurrency 8 hydrate <DEST> --name <NAME>` (flag form)
  - OR: `CLW_REF_DOMAIN=runner clw --concurrency 8 hydrate <DEST> --name <NAME>` (env form, the runner entrypoint precedent)
  - For snapshot: `clw --ref-domain runner snapshot <DIR> --name <NAME> --force`
- **Fix:** Move `--ref-domain` and `--concurrency` BEFORE the subcommand. Use `CLW_REF_DOMAIN` env var (the path the existing `corelink-runners/deploy/runner/entrypoint.sh:95-97` already uses, and which is the path the WP-01 `STATIC_ENV_VARS` injected into `envVars` expects to flow through `export`).

### B2. **`xvfb-run -a -s "..."` is Broken — `-s` is for `Xvfb`, Not `xvfb-run`**
- **Location:** `supervisord.conf:284`
- **Code:** `command=xvfb-run -a -s "-screen 0 1920x1080x24 +extension RANDR +extension GLX +extension RENDER" /bin/bash -c "sleep infinity"`
- **Problem:** `xvfb-run -s` is a real flag (it passes the rest to `Xvfb`), BUT the standard `xvfb-run` from Debian `xvfb` package uses `--server-args` (`-a` is fine, `-s` is the short form of `--server-args`, so the syntax is OK actually — but the issue is the SCRIPT BLOCK). More importantly, `xvfb-run` will **reap Xvfb** when its child exits, which is fine here since `sleep infinity` never exits, but if Xvfb crashes, `xvfb-run` exits too and supervisord's `autorestart=true` restarts the whole chain. The real issue: `xvfb-run` was never designed to be supervised; its job is to run a one-shot command. Using it under supervisord is the wrong tool.
- **Fix:** Use `Xvfb` directly as the supervised program (this is what `corelink-runners/deploy/runner` patterns and every `code-server` / `kasm` image do):
  ```ini
  [program:xvfb]
  command=/usr/bin/Xvfb :99 -screen 0 1920x1080x24 +extension RANDR +extension GLX +extension RENDER -ac
  autorestart=true
  startsecs=2
  priority=10
  ```
  Then drop `DISPLAY=:99` env into all dependent programs via `environment=`.

### B3. **Chromium `--enable-features=VaapiVideoDecoder` Doesn't Exist for Headless Linux**
- **Location:** `supervisord.conf:314`
- **Code:** `--enable-features=WebRTC-H264WithOpenH264FFmpeg,VaapiVideoDecoder`
- **Problem:** `VaapiVideoDecoder` is a ChromeOS/Android flag. On Linux `--ozone-platform=headless`, video decode is done by FFmpeg/VDA, not VA-API. The flag is silently ignored OR (worse) trips a Chromium warning that the platform isn't supported. Also `WebRTC-H264WithOpenH264FFmpeg` requires the `libopenh264` codec to be installed, which is NOT in the WP-02 `apt-get install` list.
- **Fix:** Drop both features flags. For headless chromium, the only thing that matters is `--headless=new` (or `--headless=old` for older versions) and `--no-sandbox` (already present). If H.264 is required, install `libopenh264-*` packages in WP-02.

### B4. **`xvfb-run` Priority vs. `chromium` Priority — Race on Startup**
- **Location:** `supervisord.conf:288, 324`
- **Code:** `xvfb priority=10, startsecs=3`; `chromium priority=20, startsecs=5`
- **Problem:** `priority` in supervisord only orders the START SEQUENCE, but `priority` between groups is **the same group** (both default `1`); priority only matters for ordering within a group. More critically: chromium starts as soon as the queue reaches it, NOT after `xvfb` has finished `startsecs=3`. supervisord does NOT do dependency-based waiting — priority is just "the order to fork in". A 5-second-head-start means chromium can launch before xvfb is actually listening on :99.
- **Fix:** Add explicit `depends_on` directives (or use group priorities properly). The correct pattern:
  ```ini
  [program:xvfb]
  priority=10
  startsecs=2
  
  [program:chromium]
  priority=20
  ; Wait for xvfb to be healthy (xvfb_run_checks or explicit dep)
  ```
  OR use the `autorestart` + a `wait_for_x` check in the chromium command (`xvfb-run -a chromium ...` if you must keep xvfb-run).

### B5. **`trap 'snapshot_all' EXIT` Fires on Supervisord `Stop` — Causes Double Snapshot + Race**
- **Location:** `entrypoint.sh:222-224`
- **Code:** `trap 'snapshot_all' EXIT; trap 'snapshot_all' SIGTERM; trap 'snapshot_all' SIGINT`
- **Problem:** When the container receives SIGTERM, BOTH `SIGTERM` and `EXIT` traps fire (the shell exits after the signal handler runs). That means `snapshot_all` runs TWICE. First time: a 30-second snapshot of /data/chrome + /data/workspace starts. Second time: clw's lock file is held by the first invocation, second invocation may either fail, deadlock, or queue. The `corelink-runners/deploy/runner/entrypoint.sh:251-252` explicitly uses ONE `term_handler` with an idempotent guard (`TERMINATED=1`) and NO `trap EXIT` — that's the SOTA pattern this WP was supposed to mirror.
- **Fix:** Use a single handler with an idempotency guard. The `EXIT` trap should ONLY catch the case where `exec supervisord` itself died (kernel panic on PID 1) — that path needs snapshot, but the signal-handler path also needs it. Consolidate:
  ```bash
  SNAPSHOT_TAKEN=0
  do_snapshot() {
      [[ $SNAPSHOT_TAKEN -eq 1 ]] && return 0
      SNAPSHOT_TAKEN=1
      snapshot_all
  }
  trap 'do_snapshot' EXIT
  trap 'do_snapshot; exit 143' TERM
  trap 'do_snapshot; exit 130' INT
  trap 'do_snapshot' USR1
  ```
  Also: `exec supervisord` is the LAST statement; if supervisord exits 0 (graceful), the EXIT trap fires and snapshots — but supervisord already had to forward SIGTERM to children. Make sure supervisord's `stopsignal=TERM` is set so chromium gets a chance to flush.

### B6. **`clw snapshot --force` on a FRESH Workspace Loses Data — `--force` Means Replace**
- **Location:** `entrypoint.sh:183-187`, `195-199`
- **Problem:** `clw snapshot --force` is documented in `snapshot.rs:39-44` as "Replace an existing workspace name whose content differs (delete-then-write)." If the workspace is fresh, `--force` is a no-op. But if the workspace exists AND the container crashed MID-WRITE (Chromium was killed by OOM during a large workspace write), the previous successful snapshot is a HEAD that may not be consistent. `--force` deletes it then re-writes, which is correct for recovery. **HOWEVER**: the entrypoint is also called for the FIRST-RUN case, where there is no existing snapshot. There's no bug here, but the WP claims "force=true" (line 177) without explaining the semantic. Also, the WP assumes a previous `clw hydrate` call left a manifest — if the container starts fresh AND `hydrate` returned exit 2 (not found), the first `snapshot` writes a NEW name, which is correct.
- **Real bug:** The WP doesn't differentiate between a **first-run fresh container** and a **crash-restart container**. The first should be a normal `clw snapshot`, not `--force` (the server returns 409 in that case, which the entrypoint treats as "fine"). See `snapshot.rs:178-184` for the 409→friendly-error mapping. The entrypoint should NOT pass `--force` on the first run; only on restart-with-existing-snapshot.
- **Fix:** Add a state file (e.g. `/data/.clw-hydrated`) that `hydrate_profile`/`hydrate_workspace` touch on success. `snapshot_all` only passes `--force` if that file exists, signalling that we know we're clobbering an existing snapshot.

### B7. **Chromium Runs as Root Inside `nodaemon` Supervisord — `USER coder` From Dockerfile is Lost**
- **Location:** `supervisord.conf` (every program)
- **Problem:** The Dockerfile (WP-02:160) sets `USER coder`. But supervisord (when started as PID 1) runs every program as the SAME UID as the supervisord process. If the image is started correctly with `USER coder`, supervisord runs as `coder`, and children inherit — that's fine. **But:** `nodaemon=true` and `pidfile=/tmp/supervisord.pid` are paths in `/tmp` which is world-writable. A symlink attack on the pidfile is possible if any other process can write to `/tmp`. Also: chromium's `--user-data-dir=/data/chrome` with `coder` UID + `chromium --no-sandbox` means any RCE in chromium gets file write as coder.
- **Fix:**
  1. Pin the pidfile to a path only `coder` can write: `pidfile=/run/supervisord.pid` (and `mkdir -p /run && chown coder /run` in WP-02).
  2. Add `chmod 700 /data/chrome` (the profile dir) in the Dockerfile.
  3. Add `stopsignal=TERM` to every program so supervisord forwards SIGTERM (currently only SIGKILL is sent by default on supervisord restart).

### B8. **Supervisord `nodaemon=true` + `exec` Race — PID 1 May Be `bash`, Not Supervisord**
- **Location:** `entrypoint.sh:257`
- **Code:** `exec supervisord -c /etc/supervisord.conf`
- **Problem:** `exec` replaces the bash process with supervisord, which is correct for making supervisord PID 1. **But:** the signal traps `EXIT`/`TERM`/`INT` are set on the BASH process, not on supervisord. When `exec` happens, the bash traps are DESTROYED. So if supervisord dies (which triggers the bash process to be replaced, no longer exists), the `EXIT` trap never fires. More importantly, if supervisord receives SIGTERM from the platform, supervisord handles it (default: forward to children, then exit), and the kernel-reaped `entrypoint.sh` bash process is GONE. There is no PID 1 to trap on.
- **Real pattern (corelink-runners/deploy/runner/entrypoint.sh):** The bash script stays PID 1, supervises its own child via a watchdog loop, and forwards signals. The `exec supervisord` pattern is fundamentally incompatible with `trap '...' EXIT`.
- **Fix:** Remove `exec supervisord`. Keep bash as PID 1. Either:
  - Option A: Run supervisord in background, then `wait $!` in the bash loop, with `trap 'snapshot_all; kill -TERM $SUPERVISORD_PID' EXIT TERM INT`. This is the pattern that lets traps actually fire.
  - Option B: Don't use supervisord. Run each process in a bash background loop with pid tracking, and signal-forward on TERM. This is the pattern `corelink-runners/deploy/runner/entrypoint.sh:222-261` already uses and works.

### B9. **Missing `stopsignal=TERM` + `stopwaitsecs` on Supervisord Programs — SIGKILL by Default**
- **Location:** `supervisord.conf` (all programs)
- **Problem:** Supervisord defaults to `stopsignal=TERM` and `stopwaitsecs=10`. The WP shows `autorestart=true` but no `stopsignal`/`stopwaitsecs` config. That means on a graceful stop:
  - supervisord sends SIGTERM
  - waits 10s
  - sends SIGKILL
  - 10 seconds is NOT enough for `clw snapshot` of a 1GB profile (the WP's "north star" of zero data loss)
- **Fix:** Add to every program: `stopsignal=TERM` and `stopwaitsecs=120` (2 min — chromium needs ~30s to flush dirty buffers, then snapshot needs ~60s for 1GB profile at 16MB/s upload).
  Also: in the entrypoint's snapshot trap, set a **higher** ceiling — 300s — to handle the worst case before the platform sends SIGKILL. See `corelink-runners/deploy/runner/entrypoint.sh:211` for the `RUNNER_TERM_GRACE_SECS` pattern (840s = 14 min, just under the 15-min platform ceiling).

### B10. **Webhook-TLS / Outbound Network Egress for `clw hydrate` Not Constrained to `corelink-api.humangr.com`**
- **Location:** `entrypoint.sh` (no `allowedHosts` in container)
- **Problem:** WP-01 sets `allowedHosts = ["corelink-api.humangr.com", "*.cloudflarestorage.com", "*.r2.cloudflarestorage.com"]` on the Container class. That allows egress to those domains. **But:** this WP's entrypoint does not enforce it. If someone disables the container-level allowlist (e.g. via wrangler config drift), `clw` can exfiltrate the profile to ANY endpoint. The entrypoint should **defense-in-depth** by running `clw` with an env that pins the endpoint.
- **Fix:** Already partially correct via `CLW_ENDPOINT` env var (WP-01 injects it; entrypoint reads it). But the entrypoint should ALSO refuse to start if `CLW_ENDPOINT` is `http://` (non-TLS) — this is what `clw-types::Config::cas_url` assumes (`https://` required by `validate_tenant` callers). Add to `validate_env`:
  ```bash
  if [[ ! "${CLW_ENDPOINT}" =~ ^https:// ]]; then
      error "CLW_ENDPOINT must use https://"
      return 1
  fi
  ```

### B11. **`workspaceName`/`profileName` Are Not Validated Against `clw-types` Schema — Tenant-Mismatch + Path-Traversal Risk**
- **Location:** `entrypoint.sh:66-67`, `validate_env`
- **Problem:** The entrypoint reads `WORKSPACE_NAME`/`PROFILE_NAME` from env and passes them to clw. WP-01 has `WorkspaceNameSchema = /^[a-zA-Z0-9_-]+$/` (Zod) which is the same regex the runner uses, but the entrypoint does NOT validate. A tenant that injects `WORKSPACE_NAME=../../etc` (if DO config is bypassed) gets path-traversal into the snapshot store. clw-types validates workspace name at hydrate time (`validate_workspace_name` equivalent), but the entrypoint should pre-validate to fail fast with a clear message.
- **Fix:** Add to `validate_env`:
  ```bash
  if [[ ! "${WORKSPACE_NAME}" =~ ^[a-zA-Z0-9_-]{1,128}$ ]]; then
      error "WORKSPACE_NAME invalid: must match ^[a-zA-Z0-9_-]{1,128}\$"
      return 1
  fi
  if [[ ! "${PROFILE_NAME}" =~ ^[a-zA-Z0-9_-]{1,128}$ ]]; then
      error "PROFILE_NAME invalid: must match ^[a-zA-Z0-9_-]{1,128}\$"
      return 1
  fi
  ```
  Same for `CLW_TENANT` (use `clw_types::validate_tenant` semantics: `[A-Za-z0-9._-]{1,128}`).

---

## 🟠 HIGH SEVERITY ISSUES

### H1. **No `--manifest-digest` Fallback for Hydrate — First-Run vs. Re-Hydrate Path Is Fragile**
- **Location:** `entrypoint.sh:135-154`
- **Problem:** `clw hydrate` exit 2 is treated as "not found, first run, OK" (lines 146-150). But `clw_cli` exit 2 also covers malformed args, permission errors, etc. Mapping exit 2 → "first run" is wrong for any other cause. The real "not found" signal from clw is the `--json` output: `{"root": null, ...}` (or the absence of a `RefRecord` in the AC). The entrypoint should call `clw status --name X --json` first and only call `hydrate` if a manifest exists, OR use `--manifest-digest` after a `ls --json` to get the hex.
- **Fix:** Add a pre-check: `clw --ref-domain runner ls --json` to enumerate the tenant's refs, look up the name, and only `hydrate` if present. If not present, log and proceed with empty dest (which clw requires to be empty per `hydrate.rs:32`).

### H2. **`snapshot_all` Race with Concurrent `clw hydrate` in `xvfb` Failure-Recovery Restart**
- **Location:** `entrypoint.sh:176-213`
- **Problem:** If `chromium` crashes and supervisord `autorestart=true` triggers a restart, supervisord sends SIGTERM to chromium, then SIGKILL after `stopwaitsecs`. If the entrypoint receives SIGTERM (platform stop) AT THE SAME TIME as chromium is being restarted, the snapshot in `do_snapshot` runs concurrently with the chromium restart. The snapshot may capture a half-flushed profile. There's no coordination between the supervisord child restart and the entrypoint-level snapshot.
- **Fix:** Add a `stopsignal=TERM` to the chromium program AND make `snapshot_all` use `fuser -k /data/chrome` (or `kill -STOP` on chromium PID) to freeze the profile before snapshotting. Or: snapshot inside the chromium `stopsignal` handler via a `pre-stop` script.

### H3. **No `chmod 700 /data/chrome` in Entrypoint — Profile World-Readable**
- **Location:** `entrypoint.sh` (no chmod on /data/chrome)
- **Problem:** WP-02 creates `/data/chrome` with `chown coder:coder` (line 152-153 of WP-02). But the MODE is whatever the base image's `/data` had (typically 755). Chromium's profile contains cookies, local storage, and OAuth tokens for the user's logged-in sessions. World-readable profile = local-user credential theft.
- **Fix:** Add `chmod 700 /data/chrome /data/workspace` in `validate_env` (after the writability check) or in the Dockerfile (WP-02).

### H4. **Supervisord `autorestart=true` + `startsecs=3-5` — Crash-Loop Detection Bypass**
- **Location:** `supervisord.conf` (every program)
- **Problem:** supervisord has `startretries=3` (correct), but no `autorestart.maxretries` per the WP. After 3 failures in `startsecs`, supervisord marks the process as `FATAL` and stops restarting. **But** the `startsecs=3` for xvfb is too low: on a cold container with a busy CPU, Xvfb can take 5-10s to come up. The first 3 attempts will all fail `startsecs` and the program goes `FATAL`. The supervisord log will say "FATAL" but the entrypoint trap `snapshot_all EXIT` never sees the per-program crash (supervisord keeps running).
- **Fix:** Bump `startsecs` to 10-15 for xvfb and chromium, 5 for others. Add `exitcodes=0,2` so supervisord does NOT mark a clean exit as failure.

### H5. **No `clw` Lock-File Handling — Concurrent `hydrate` + `snapshot` from WP-06 OnStop**
- **Location:** `entrypoint.sh` + WP-06 (`onStop` → `clw snapshot`)
- **Problem:** WP-06's `onStop` is documented as "MUST: snapshot profile + workspace." If the entrypoint's `snapshot_all` (via EXIT trap) AND WP-06's `onStop` snapshot both run, they contend on clw's per-tenant lock file (typical for any CAS tool that uses an advisory lock). One will fail with "lock held by another process." The current entrypoint doesn't coordinate.
- **Fix:** Pick ONE snapshot path (entrypoint or `onStop` hook, not both). Recommended: keep snapshot in `onStop` (WP-06, which has access to the DO state and can record the result), and have the entrypoint's `snapshot_all` be a FALLBACK for the case where `onStop` didn't run (e.g. kernel panic). Use a sentinel file `/data/.clw-snapshot-in-progress` that the entrypoint sets + checks.

### H6. **`x11vnc -passwd ""` Disables Auth — VNC Server Open to Anyone on the Container Network**
- **Location:** `supervisord.conf:335`
- **Code:** `x11vnc -passwd ""`
- **Problem:** The container is on an internal CF network, but `x11vnc` binds to all interfaces (no `-listen localhost` or `-listen 127.0.0.1`). The `websockify` proxies on 6080, so x11vnc on 5900 only needs to be reachable by localhost. With `-passwd ""`, anyone who can reach port 5900 on the container gets the desktop. The container's network namespace is shared with supervisord's children unless WP-02's Dockerfile adds `EXPOSE` + an internal-only network.
- **Fix:** Add `-listen 127.0.0.1` and `-localhost` to the x11vnc command. Verify `x11vnc -help` shows the right flag (modern x11vnc uses `-listen 127.0.0.1` or `-listen localhost`).

### H7. **`code-server --auth none` + `--bind-addr 0.0.0.0:8080` — No Auth on Editor**
- **Location:** `supervisord.conf:379`
- **Code:** `code-server --bind-addr 0.0.0.0:8080 --auth none`
- **Problem:** `code-server --auth none` means anyone who reaches port 8080 in the container gets the editor (with file system access as `coder`). The WebSocket proxy in WP-05 is the gatekeeper, but **if WP-05 has a bug**, the editor is wide open. The WP should DEFENSE-IN-DEPTH: code-server should require auth, and WP-05 should provide the credential.
- **Fix:** Change to `code-server --bind-addr 0.0.0.0:8080 --auth password` and have WP-05 inject the password via env var or config file. (Or use the new `--github-auth` for a real OAuth flow if WP-09 Dashboard provides it.)

### H8. **No `CHROME_DEVEL_SANDBOX` / `--user-namespace-sandbox` for Chromium Despite `--no-sandbox`**
- **Location:** `supervisord.conf:298-329`
- **Problem:** `--no-sandbox --disable-setuid-sandbox` removes ALL sandboxing because we run as non-root but with `--user-data-dir=/data/chrome` writable by us. The standard mitigation in Linux containers is the **user namespace sandbox** (`--user-namespace-sandbox` + `/proc/self/ns/user` mapping). The WP's flag list does NOT include this; it falls back to "no sandbox."
- **Fix:** If the container runtime supports it (Docker default YES, CF Containers uncertain), use `--user-namespace-sandbox`. If not, document the trade-off (no sandbox → RCE = file write as coder) and add seccomp / apparmor in the Dockerfile (WP-02).

### H9. **`ttyd -W` Means Writable — No Auth, Anyone With WS URL Gets Shell**
- **Location:** `supervisord.conf:364`
- **Code:** `ttyd -p 7681 -W --writable --cwd /data/workspace`
- **Problem:** `-W` (no auth) on ttyd means any WebSocket connection to port 7681 gets a root-ish bash shell. The WP-05 WebSocket proxy is the gatekeeper, but ttyd doesn't have a built-in token mechanism (the password flag is `-c credential`). Combined with the public bind (`0.0.0.0`), this is a full shell.
- **Fix:** Add `-c <random-token>` and have WP-05 / WP-09 inject the token from DO state. Verify ttyd 1.7.x supports `-c` (it does).

---

## 🟡 MEDIUM SEVERITY ISSUES

### M1. **No `set -o errtrace` / `set -E` — Function Errors Not Propagating**
- **Location:** `entrypoint.sh:61`
- **Code:** `set -euo pipefail`
- **Problem:** `set -e` exits on the first error, but ERR signals are NOT inherited by functions in subshells or commands. If `hydrate_profile` fails inside a subshell (e.g. `$()` in a future edit), the failure is silent. `set -E` propagates ERR to functions; combined with a `trap ... ERR` handler, it gives full error coverage.
- **Fix:** Add `set -E` and a `trap 'error "trace: line $LINENO, exit $?"' ERR`.

### M2. **Log Format Is Not Machine-Parsable — No `jq`-able JSON**
- **Location:** `entrypoint.sh:87-93`
- **Code:** `log() { echo "${LOG_PREFIX} $(date -Is) $*"; }`
- **Problem:** Logs are human-readable but the platform (CF Workers + `wrangler tail`) parses JSON for log search. The runner entrypoint (`corelink-runners/deploy/runner/entrypoint.sh:75`) emits structured JSON-like key=value lines that survive grep/jq. This entrypoint emits un-parseable prefixed lines.
- **Fix:** Either (a) emit JSON to stdout, or (b) emit a stable `key=value key=value` format with a clear schema. Recommended:
  ```bash
  log() { echo "$(date -Is) level=info component=devenv-entrypoint msg=$*"; }
  ```

### M3. **Supervisord `childlogdir=/tmp/supervisor_logs` — Cleared on Container Restart**
- **Location:** `supervisord.conf:276`
- **Problem:** `/tmp` is cleared on every container restart (CF Containers use ephemeral filesystems). This is fine for a dev env, but the WP doesn't document it. If a debugging session needs post-mortem logs from a crash, the logs are gone. The `stdout_logfile=/dev/stdout` redirect already covers the live stream (CF captures stdout), so this is mostly OK, but `childlogdir` is redundant + misleading.
- **Fix:** Either remove `childlogdir` (since `stdout_logfile` and `stderr_logfile` are already set to `/dev/stdout`/`/dev/stderr`), or set it to a stable path and add a `logfile_maxbytes=10MB` cap.

### M4. **`ps1` Escape Sequences in Supervisord `environment=PS1=...` Are Double-Escaped — Wrong**
- **Location:** `supervisord.conf:373`
- **Code:** `PS1="\\[\\033[1;32m\\]devenv\\[\\033[0m\\] \\[\\033[1;34m\\]\\w\\[\\033[0m\\] \\$ "`
- **Problem:** The `\\033` and `\\[` in the config file are passed through supervisord's INI parser verbatim. bash sees `\[` (single backslash + bracket) which is the bash PS1 escape for non-printing chars, but `033` is interpreted as octal by bash. The intended form is `\[\033[1;32m\]devenv\[\033[0m\]`. The double-escape makes the colors render wrong (or as literal text).
- **Fix:** Either:
  - Set the prompt via a `.bashrc` snippet in WP-02's Dockerfile (cleanest):
    ```bash
    RUN echo 'PS1="\[\033[1;32m\]devenv\[\033[0m\] \[\033[1;34m\]\w\[\033[0m\] \$ "' >> /home/coder/.bashrc
    ```
  - OR keep the env var but single-escape: `PS1="\[\033[1;32m\]devenv\[\033[0m\]"` — supervisord INI does NOT process backslash escapes.

### M5. **No `--user-data-dir` Conflict Check on First Run**
- **Location:** `supervisord.conf:300`
- **Code:** `chromium --user-data-dir=/data/chrome`
- **Problem:** If `hydrate_profile` failed (network down, server unreachable) AND the entrypoint proceeded (per the "non-fatal if exit 2" branch), `/data/chrome` may be empty or partially populated. Chromium then sees a non-empty dir with `Preferences` and `Cookies` from a previous user (cross-tenant leak on a SHARED container instance, though CF Containers give each DO a fresh container). More importantly: if the dir has a `LOCK` file from a crashed previous chromium, chromium exits immediately.
- **Fix:** In `hydrate_profile`, after success, write a sentinel file `/data/chrome/.clw-hydrated-version` with the manifest digest. Chromium startup should check for and remove stale `.lock` files (or use `chromium --user-data-dir=/data/chrome --enable-crashpad`).

### M6. **No `WPA_SUPPLICANT`/DNS Override — Container Uses CF DNS by Default**
- **Location:** `supervisord.conf` (no DNS config)
- **Problem:** `corelink-api.humangr.com` resolution works inside CF Containers (it's an internal host). But if `CLW_ENDPOINT` is ever pointed at a different host (for staging/canary), DNS may fail. No explicit `dns=corelink-api.humangr.com` or `/etc/hosts` entry. Not a blocker (the endpoint is set via env), but a robustness gap.
- **Fix:** Add a `dns=...` env in supervisord or a `dns=` line in the [supervisord] section if CF Containers supports it. Otherwise document the dependency.

---

## 📋 DoD Gap Analysis

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | `entrypoint.sh` executable, `bash -n` passes | ⚠️ Partial | Syntax may pass, but flags invalid (B1) |
| 2 | Validates required env vars | ❌ Fail | Misses `CLW_ENDPOINT` https check, workspace name regex (B10, B11) |
| 3 | `clw hydrate` for both profile + workspace | ❌ Fail | Flags in wrong position (B1) — clw will reject |
| 4 | `clw snapshot --force` on EXIT/SIGTERM/SIGINT | ⚠️ Partial | Traps set, but `exec` discards them (B8); double-snapshot (B5) |
| 5 | All 6 programs in supervisord | ✅ Pass | All present |
| 6 | Chromium `--password-store=basic` | ✅ Pass | Line 301 |
| 7 | Chromium headless + remote-debugging on 9222 | ⚠️ Partial | Port 9222 exposed but not in `requiredPorts` (WP-01), no auth on devtools |
| 8 | noVNC → x11vnc:5900 on 6080 | ⚠️ Partial | Works, but x11vnc has no auth (H6) |
| 9 | ttyd on 7681, cwd `/data/workspace` | ⚠️ Partial | Works, but no auth (H9) |
| 10 | code-server on 8080, auth none | ❌ Fail | `auth none` is a HIGH risk (H7) |
| 11 | All processes run as `coder` | ⚠️ Partial | USER coder in Dockerfile, but supervisord doesn't re-check |
| 12 | Signal traps cover EXIT, SIGTERM, SIGINT | ⚠️ Partial | Set, but `exec` discards them (B8) |
| 13 | `snapshot_all` calls `clw snapshot --force` for both | ❌ Fail | Flags in wrong position (B1); fires twice (B5); `--force` always (B6) |

**DoD Score: 3/13 PASS, 5/13 PARTIAL, 5/13 FAIL**

---

## 📋 Invariants Verification

| Invariant | Enforced in Code? | Verdict |
|-----------|-------------------|---------|
| I1: entrypoint.sh exits non-zero if env var missing | ✅ Yes, validate_env | **ENFORCED** |
| I2: clw hydrate never fails the container (exit 2 = no snapshot = OK) | ❌ Exit 2 also covers other errors (H1) | **PARTIAL** |
| I3: snapshot_all executes on ALL exit paths | ❌ `exec supervisord` discards traps (B8) | **NOT ENFORCED** |
| I4: snapshot_all attempts BOTH profile and workspace | ✅ Yes, code does both | **ENFORCED** |
| I5: supervisord manages exactly 6 processes | ✅ Yes (5 in the conf, wait — code-server is the 6th, but `eventlistener:health_logger` is commented out) | **ENFORCED** |
| I6: All processes run under coder (UID 1000) | ⚠️ Inherits from Dockerfile USER, but no explicit `user=coder` in supervisord | **PARTIAL** |
| I7: Chromium `--password-store=basic` always set | ✅ Yes, line 301 | **ENFORCED** |
| I8: Ports 6080, 7681, 8080, 5900, 9222 used as specified | ⚠️ Port 9222 not in `requiredPorts` (WP-01 mismatch) | **PARTIAL** |
| I9: `exec supervisord` is final command (PID 1) | ❌ The `exec` discards traps (B8) | **CONTRADICTED** |

**Invariants Enforced: 4/9 (44%)** — INSUFFICIENT

---

## 📋 Quality Standards Verification

| Standard | Met? | Evidence |
|----------|------|----------|
| Shell Safety: `set -euo pipefail`, quoted variables | ✅ Pass | Line 61, all vars quoted |
| Error Handling: validation returns explicit codes | ✅ Pass | `validate_env` returns 0/1 |
| Signal Correctness: trap on EXIT covers all | ❌ Fail | `exec` breaks this (B8); double-snapshot (B5) |
| Logging: structured prefix `[devenv-entrypoint] ISO8601` | ⚠️ Partial | Format is right, but not machine-parseable (M2) |
| Non-Root: all processes as `coder` | ⚠️ Partial | Inherited, not explicit (H-missing) |
| Idempotency: snapshot_all safe to call multiple times | ❌ Fail | No idempotency guard (B5) |

**Quality Standards: 2/6 MET** — INSUFFICIENT

---

## 📋 Self-Check Points Analysis

### Self-Check 1: Signal Coverage Completeness
- [x] `trap 'snapshot_all' EXIT` — present (line 222) — BUT does not fire after `exec` (B8)
- [x] `trap 'snapshot_all' SIGTERM` — present (line 223) — BUT no idempotency (B5)
- [x] `trap 'snapshot_all' SIGINT` — present (line 224)
- [ ] SIGKILL: documented as known limitation — **Yes, line 473**
- [ ] OOM: documented as known limitation — **Yes, line 474**

**Verdict: 3/5 PASS, 2/5 DOCUMENTED LIMITATIONS**

### Self-Check 2: clw Integration Correctness
- [ ] `hydrate` uses `--ref-domain runner` — ❌ **Flag in wrong position (B1)**
- [ ] `snapshot` uses `--force --ref-domain runner` — ❌ **Flag in wrong position (B1)**
- [ ] `--concurrency 8` matches `RunnerDevEnvDO.envVars` default — ⚠️ **Flag position (B1)**
- [ ] Exit code 2 from `hydrate` = "not found" = non-fatal — ⚠️ **But exit 2 = other errors too (H1)**
- [ ] Profile dir = `/data/chrome`, Workspace dir = `/data/workspace` — ✅ Pass

**Verdict: 1/5 PASS, 4/5 FAIL/PARTIAL**

### Self-Check 3: Supervisord Process Dependencies
- [ ] `xvfb` priority=10 — ✅ Pass
- [ ] `chromium` priority=20 — ✅ Pass
- [ ] `x11vnc` priority=30 — ✅ Pass
- [ ] `novnc/ttyd/code-server` priority=40 — ✅ Pass
- [ ] `startsecs` allow proper startup sequencing — ❌ **xvfb=3 too low (H4); no actual dep waiting (B4)**
- [ ] `autorestart=true` on all — ✅ Pass

**Verdict: 4/6 PASS, 2/6 FAIL**

---

## 📊 SCORECARD

| Category | Score | Required | Gap |
|----------|-------|----------|-----|
| Blocking Issues | 11 | 0 | **-11** |
| High Issues | 9 | 0 | **-9** |
| Medium Issues | 6 | 0 | **-6** |
| DoD Pass Rate | 23% | 100% | **-77%** |
| Invariants Enforced | 44% | 100% | **-56%** |
| Quality Standards | 33% | 100% | **-67%** |
| Self-Check Pass | 47% | 100% | **-53%** |

**OVERALL VERDICT: ❌ FAIL — Requires major rework before sign-off**

---

## 🔧 FIX PRIORITY

### Must Fix (Blockers — none negotiable)
1. **B1** — Move `--ref-domain` and `--concurrency` BEFORE subcommand
2. **B2** — Replace `xvfb-run` with direct `Xvfb`
3. **B3** — Drop `--enable-features=VaapiVideoDecoder` (Linux headless)
4. **B5** — Idempotent snapshot handler (no double-snapshot)
5. **B7** — Pidfile in `/run/`, `stopsignal=TERM`, `stopwaitsecs=120`, `chmod 700 /data/chrome`
6. **B8** — Don't `exec supervisord`; keep bash as PID 1 with traps
7. **B9** — Add `stopsignal=TERM` + `stopwaitsecs=120` to all programs
8. **B10** — Validate `CLW_ENDPOINT` is `https://`
9. **B11** — Validate `WORKSPACE_NAME`/`PROFILE_NAME`/`CLW_TENANT` against clw-types schema

### Should Fix (High)
1. **H1** — Pre-check with `clw ls --json` before `hydrate`
2. **H4** — Bump `startsecs` to 10-15 for xvfb/chromium
3. **H5** — Pick ONE snapshot path (entrypoint OR `onStop`)
4. **H6** — `x11vnc -listen 127.0.0.1 -localhost`
5. **H7** — `code-server --auth password` with token injection
6. **H8** — Document `--no-sandbox` trade-off; add seccomp in WP-02
7. **H9** — `ttyd -c <token>` with token injection

### Nice to Fix (Medium)
1. **M1** — `set -E` + `trap '...' ERR`
2. **M2** — Structured logging (`key=value` or JSON)
3. **M3** — Remove `childlogdir` (redundant with stdout)
4. **M4** — Fix PS1 escape sequences (or move to `.bashrc`)
5. **M5** — Sentinel file `/data/chrome/.clw-hydrated-version`
6. **M6** — Document DNS dependency (or add `dns=` line)

---

## 🔗 CROSS-WP COORDINATION NEEDS

1. **WP-01 ↔ WP-03:** `RunnerDevEnvDO.envVars` injects `CLW_TENANT`, `CLW_TOKEN`, `WORKSPACE_NAME`, `PROFILE_NAME`, `CLW_REF_DOMAIN` (= "runner"), `CLW_ENDPOINT`. The entrypoint MUST read these env vars without prefixing. Currently OK, but `enableInternet: true` is the network gate — confirm WP-01's `allowedHosts` includes only the CAS/AC endpoints (it does).

2. **WP-02 ↔ WP-03:** Dockerfile creates `/data/chrome` and `/data/workspace` as `coder:coder` (line 152-153). The entrypoint must `chmod 700` (B7/H3). The Dockerfile also needs `mkdir -p /run && chown coder /run` for the supervisord pidfile.

3. **WP-04 ↔ WP-03:** WP-04 implements `clw`-via-`exec()` from the DO. The entrypoint does NOT call clw via DO — it calls clw directly inside the container. This is the **correct** split (entrypoint handles the container's own hydration, DO handles on-demand operations). But the entrypoint's `clw` calls should be in a separate script (`/usr/local/bin/clw-hydrate`) that WP-04 can also call, to avoid duplication.

4. **WP-05 ↔ WP-03:** WP-05's WebSocket proxy is the only auth gate for ports 6080/7681/8080. The entrypoint must NOT add a second auth (defense-in-depth via `--auth password` etc.) WITHOUT coordinating with WP-05 — otherwise the proxy breaks. See H7, H9.

5. **WP-06 ↔ WP-03:** `onStop` snapshots. The entrypoint's `snapshot_all` ALSO snapshots. They MUST coordinate (H5). Recommended: entrypoint snapshots ONLY if `onStop` didn't (use sentinel file at `/data/.clw-snapshotted-by-onstop`).

6. **WP-07 ↔ WP-03:** Billing meters `clw hydrate` + `clw snapshot` time + bytes. The entrypoint's calls count toward the bill. Document this.

---

## NEXT STEPS

1. **Apply all BLOCKING fixes** (B1-B11) — non-negotiable
2. **Apply all HIGH fixes** (H1-H9) — required for security
3. **Apply at least 50% of MEDIUM fixes** (M1, M2, M4 minimum)
4. **Coordinate with WP-01, WP-04, WP-05, WP-06** on the cross-WP items above
5. **Re-verify all 13 DoD items pass**
6. **Re-verify all 9 invariants enforced in code**
7. **Re-verify all 6 quality standards met**
8. **Proceed to Iteration 2 review**

**Do NOT proceed to WP-04 until WP-03 is fixed and passes review.**

---

**END OF WP-03 ITERATION 1 REVIEW**
