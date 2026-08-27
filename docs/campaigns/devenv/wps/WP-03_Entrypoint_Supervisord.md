# WP-03: entrypoint.sh + supervisord.conf

**Status:** `NOT_STARTED`  
**Owner:** corelink-runners TL  
**Depends On:** WP-02  
**Estimate:** 2 days  
**Priority:** P0 (Critical Path)

---

## 1. Objective

Create the container bootstrap script (`entrypoint.sh`) and process manager configuration (`supervisord.conf`) that:
- Hydrate browser profile + workspace via `clw` on container start
- Launch all required processes under supervisord (xvfb, chromium, x11vnc, noVNC, ttyd, code-server)
- Snapshot browser profile + workspace on graceful shutdown OR crash (SIGTERM/SIGKILL/OOM)
- Handle signals correctly for zero data loss

---

## 2. Scope

### In Scope
- `deploy/cloudflare/entrypoint.sh` — container entrypoint script
- `deploy/cloudflare/supervisord.conf` — supervisord configuration
- Signal handling (trap EXIT, SIGTERM, SIGINT)
- `clw hydrate` on start, `clw snapshot --force` on exit
- Process definitions for all 6 services

### Out of Scope
- `clw` binary build → WP-02
- WebSocket proxy → WP-05
- DO lifecycle hooks (`onStart`/`onStop`) → WP-06
- Billing metering → WP-07

---

## 3. Technical Specification

### 3.1 File Locations
```
corelink-runners/
├── deploy/
│   └── cloudflare/
│       ├── entrypoint.sh          ← MAIN ARTIFACT 1
│       ├── supervisord.conf       ← MAIN ARTIFACT 2
│       └── Dockerfile.runner-devenv   ← WP-02 owner; COPYs entrypoint.sh + supervisord.conf
```

### 3.1.1 Cross-WP Coordination

| Artifact | Defined In | Consumed By | Notes |
|----------|-----------|-------------|-------|
| `entrypoint.sh` | WP-03 | WP-02 Dockerfile `COPY` to `/entrypoint.sh` | chmod 0755; PID 1 in container |
| `supervisord.conf` | WP-03 | WP-02 Dockerfile `COPY` to `/etc/supervisord.conf` | supervisord `-c` path |
| `/run/supervisord.pid` | WP-03 (pidfile) | WP-02 Dockerfile `mkdir -p /run && chown coder:coder /run` | without this, supervisord fails to start |
| Env vars (table below) | WP-03 reads | WP-01 `start()` injects mutable; WP-05 injects auth | missing TTYD_CRED/CODE_SERVER_PASSWORD = ttyd/code-server fail |

### 3.1.2 Environment Variables Contract

| Env Var | Injected By | Required? | Failure Mode if Missing/Wrong |
|---------|-------------|-----------|-------------------------------|
| `CLW_TENANT` | WP-01 (start) | YES | validate_env fails, container exits 1 |
| `CLW_TOKEN` | WP-01 (start) | YES | validate_env fails, container exits 1 |
| `WORKSPACE_NAME` | WP-01 (start) | YES | validate_env fails, container exits 1 |
| `PROFILE_NAME` | WP-01 (start) | YES | validate_env fails, container exits 1 |
| `CLW_ENDPOINT` | WP-01 (STATIC_ENV_VARS) | YES | validate_env fails if non-https |
| `CLW_REF_DOMAIN` | WP-01 (STATIC_ENV_VARS, always "runner") | YES | validate_env fails |
| `TTYD_CRED` | **WP-05** (per-tenant token) | YES | ttyd starts with `-c ""` → no auth (regression of H9 fix) |
| `CODE_SERVER_PASSWORD` | **WP-05** (per-tenant password) | YES | code-server refuses to start (`--auth password` requires non-empty) |
| `RUNNER_TERM_GRACE_SECS` | (optional, default 840) | NO | TERM→KILL escalation window |
| `SUPERVISORD_TERM_GRACE_SECS` | (optional, default 30) | NO | wait for children flush before snapshot |

### 3.2 entrypoint.sh (Exact)

```bash
#!/bin/bash
# deploy/cloudflare/entrypoint.sh
#
# CoreLink Runner DevEnv Container Entrypoint
# - Hydrates browser profile + workspace via clw on start
# - Starts all services via supervisord
# - Snapshots profile + workspace on exit (graceful or crash)
# - Runs as non-root user 'coder'

set -euo pipefail
set -E  # propagate ERR to functions; combined with the ERR trap below

# ======================================================================
# CONFIGURATION (injected via Container envVars at spawn)
# ======================================================================
# REQUIRED vars have NO default — validate_env below rejects empty values so a
# missing-injection (WP-01/WP-05 bug) fails fast with a clear message instead
# of silently hydrating "default" and the wrong tenant's data. The
# "https://corelink-api.humangr.com" default on CLW_ENDPOINT is intentional —
# it is the canonical CAS endpoint and matches WP-01's STATIC_ENV_VARS; the
# validator still rejects non-https values below (N9 fix).
CLW_ENDPOINT="${CLW_ENDPOINT:-https://corelink-api.humangr.com}"
WORKSPACE_NAME="${WORKSPACE_NAME:-}"
PROFILE_NAME="${PROFILE_NAME:-}"
CLW_TENANT="${CLW_TENANT:-}"
CLW_TOKEN="${CLW_TOKEN:-}"
CLW_REF_DOMAIN="${CLW_REF_DOMAIN:-runner}"
# Per-tenant auth for ttyd + code-server. Injected by WP-05. Empty = service
# fails to start (ttyd disables auth with -c "" in 1.7.7; code-server refuses
# --auth password with no PASSWORD env). N11 fix: validate below.
TTYD_CRED="${TTYD_CRED:-}"
CODE_SERVER_PASSWORD="${CODE_SERVER_PASSWORD:-}"

# Export for clw subprocesses
export CLW_ENDPOINT CLW_TENANT CLW_TOKEN CLW_REF_DOMAIN

# ======================================================================
# CONSTANTS
# ======================================================================
readonly PROFILE_DIR="/data/chrome"
readonly WORKSPACE_DIR="/data/workspace"
readonly CLW_BIN="/usr/local/bin/clw"
readonly LOG_PREFIX="[devenv-entrypoint]"

# ======================================================================
# LOGGING
# ======================================================================
log() {
    local msg
    msg="$*"
    msg="${msg//\"/\\\"}"
    echo "$(date -Is) level=info component=devenv-entrypoint msg=\"${msg}\""
}

error() {
    local msg
    msg="$*"
    msg="${msg//\"/\\\"}"
    echo "$(date -Is) level=error component=devenv-entrypoint msg=\"${msg}\"" >&2
}

# ======================================================================
# VALIDATION
# ======================================================================
validate_env() {
    local missing=()

    [[ -z "${CLW_TENANT}" ]] && missing+=("CLW_TENANT")
    [[ -z "${CLW_TOKEN}" ]] && missing+=("CLW_TOKEN")
    [[ -z "${WORKSPACE_NAME}" ]] && missing+=("WORKSPACE_NAME")
    [[ -z "${PROFILE_NAME}" ]] && missing+=("PROFILE_NAME")
    # N11: ttyd -c "" disables auth in 1.7.7; code-server with --auth password
    # refuses empty PASSWORD. Reject early so the failure points at WP-05, not
    # supervisord's crash loop.
    [[ -z "${TTYD_CRED}" ]] && missing+=("TTYD_CRED")
    [[ -z "${CODE_SERVER_PASSWORD}" ]] && missing+=("CODE_SERVER_PASSWORD")

    if [[ ${#missing[@]} -gt 0 ]]; then
        error "Missing required environment variables: ${missing[*]}"
        return 1
    fi

    # CLW_ENDPOINT must be https:// (matches clw-types::Config + defense-in-depth
    # against path injection in the AC URL).
    if [[ ! "${CLW_ENDPOINT}" =~ ^https:// ]]; then
        error "CLW_ENDPOINT must use https:// (got: ${CLW_ENDPOINT})"
        return 1
    fi

    # Workspace / profile names: align with WP-01's Zod schema
    # (WorkspaceNameSchema = /^[a-zA-Z0-9_-]+$/) — strict, no dots, no length
    # cap (Zod's .min(1).max(128) enforces 1..128). N2/N5 fix: previous
    # `[a-zA-Z0-9._-]{1,128}` was laxer than WP-01, so this entrypoint check
    # would have silently accepted names WP-01 already rejected (defense-in-
    # depth becomes misleading drift). Aligned to WP-01.
    #
    # Tenant name: stays lax ([a-zA-Z0-9._-]{1,128}) — clw-types' canonical
    # validator allows dots for tenant IDs (e.g. `acme.corp`). WP-01's
    # TenantIdSchema = /^[a-z0-9]{8,}$/ is even stricter, so any tenant that
    # reaches this entrypoint already passed WP-01 — this check is redundant
    # defense-in-depth, not the authoritative gate.
    local ws_re='^[a-zA-Z0-9_-]{1,128}$'
    if [[ ! "${WORKSPACE_NAME}" =~ ${ws_re} ]]; then
        error "WORKSPACE_NAME invalid (must match ${ws_re}, align with WP-01): ${WORKSPACE_NAME}"
        return 1
    fi
    if [[ ! "${PROFILE_NAME}" =~ ${ws_re} ]]; then
        error "PROFILE_NAME invalid (must match ${ws_re}, align with WP-01): ${PROFILE_NAME}"
        return 1
    fi
    local tenant_re='^[a-zA-Z0-9._-]{1,128}$'
    if [[ ! "${CLW_TENANT}" =~ ${tenant_re} || "${CLW_TENANT}" == "." || "${CLW_TENANT}" == ".." ]]; then
        error "CLW_TENANT invalid (must match ${tenant_re} and not be . or ..): ${CLW_TENANT}"
        return 1
    fi
    if [[ "${CLW_TENANT}" == .* || "${CLW_TENANT}" == *. ]]; then
        error "CLW_TENANT must not start or end with '.': ${CLW_TENANT}"
        return 1
    fi

    # Verify clw binary exists and is executable
    if [[ ! -x "${CLW_BIN}" ]]; then
        error "clw binary not found or not executable at ${CLW_BIN}"
        return 1
    fi

    # Verify data directories exist and are writable; tighten perms to 0700 so
    # the profile (cookies, OAuth tokens) and workspace are not world-readable
    # inside the container's user namespace.
    for dir in "${PROFILE_DIR}" "${WORKSPACE_DIR}"; do
        if [[ ! -d "${dir}" ]]; then
            error "Data directory missing: ${dir}"
            return 1
        fi
        if [[ ! -w "${dir}" ]]; then
            error "Data directory not writable: ${dir}"
            return 1
        fi
        chmod 700 "${dir}" 2>/dev/null || true
    done

    log "Environment validation passed"
    return 0
}

# ======================================================================
# CLW OPERATIONS
# ======================================================================
# Exit-code-2-as-not-found is UNSAFE: clw exits 2 on ANY error (auth, network,
# HTTP 5xx) per clw-cli/src/main.rs:287-291. Instead, we use `clw ls --json`
# to check whether a named ref EXISTS first, and only call `hydrate` when
# the lookup succeeds. On a successful ls, the ref's manifest digest is
# passed to `hydrate --manifest-digest` to skip the name→ref round-trip and
# to surface any subsequent download error as a real failure (exit 2 = real
# error, not "not found").
clw_ref_exists() {
    local name="$1"
    local ls_out
    if ! ls_out="$("${CLW_BIN}" --ref-domain "${CLW_REF_DOMAIN}" --json \
            ls --name "${name}" 2>&1)"; then
        error "clw ls --name ${name} failed: ${ls_out}"
        return 2
    fi
    # clw ls --json emits a JSON array (possibly empty). A non-empty array
    # means the ref exists. Defensive: anything we can't parse, treat as
    # "exists" so we err toward attempting hydrate and seeing the real error.
    if [[ "${ls_out}" == "[]" || -z "${ls_out// /}" ]]; then
        return 1
    fi
    return 0
}

hydrate_profile() {
    log "Hydrating browser profile: ${PROFILE_NAME}"

    # N4 fix: clw_ref_exists returns 1 (no ref) or 2 (ls failed: auth/network/5xx).
    # Exit 0 = exists. Distinguish so a token-revoked or CAS-unreachable error
    # fails loudly instead of silently starting with a blank profile (the iter 1
    # B15 regression path). Without this branch, a revoked token on every
    # container start would wipe the user's session with no error.
    local ref_rc=0
    clw_ref_exists "${PROFILE_NAME}" || ref_rc=$?
    if [[ ${ref_rc} -eq 1 ]]; then
        log "No existing browser profile (first run), continuing with fresh profile"
        return 0
    fi
    if [[ ${ref_rc} -ne 0 ]]; then
        error "clw ls failed for profile (rc=${ref_rc}); refusing to fall through to blank-hydrate (would clobber user session). Aborting container start."
        return "${ref_rc}"
    fi

    if "${CLW_BIN}" --ref-domain "${CLW_REF_DOMAIN}" --concurrency 8 \
        hydrate "${PROFILE_DIR}" --name "${PROFILE_NAME}"; then
        log "Browser profile hydrated successfully"
        : > "${PROFILE_DIR}/.clw-hydrated-version"
        return 0
    else
        local exit_code=$?
        error "Failed to hydrate browser profile (exit code: ${exit_code})"
        return ${exit_code}
    fi
}

hydrate_workspace() {
    log "Hydrating workspace: ${WORKSPACE_NAME}"

    # N4 fix: see hydrate_profile. Same rc-distinction.
    local ref_rc=0
    clw_ref_exists "${WORKSPACE_NAME}" || ref_rc=$?
    if [[ ${ref_rc} -eq 1 ]]; then
        log "No existing workspace (first run), starting fresh"
        return 0
    fi
    if [[ ${ref_rc} -ne 0 ]]; then
        error "clw ls failed for workspace (rc=${ref_rc}); refusing to fall through to blank-hydrate. Aborting container start."
        return "${ref_rc}"
    fi

    if "${CLW_BIN}" --ref-domain "${CLW_REF_DOMAIN}" --concurrency 8 \
        hydrate "${WORKSPACE_DIR}" --name "${WORKSPACE_NAME}"; then
        log "Workspace hydrated successfully"
        : > "${WORKSPACE_DIR}/.clw-hydrated-version"
        return 0
    else
        local exit_code=$?
        error "Failed to hydrate workspace (exit code: ${exit_code})"
        return ${exit_code}
    fi
}

snapshot_all() {
    log "Snapshot: browser profile + workspace"

    # --force replaces an existing name with different content (delete-then-
    # write per clw_cli snapshot.rs:39-44). Only pass it on restart-with-
    # existing-snapshot (the sentinel written by hydrate_*). A first-run fresh
    # container must NOT pass --force, or clw will treat a clean write as a
    # replacement and may 409 if the ref store is create-only.
    local -a force_args=()
    if [[ -f "${PROFILE_DIR}/.clw-hydrated-version" || -f "${WORKSPACE_DIR}/.clw-hydrated-version" ]]; then
        force_args=(--force)
    fi

    local profile_ok=0
    local workspace_ok=0

    # Snapshot browser profile
    if "${CLW_BIN}" --ref-domain "${CLW_REF_DOMAIN}" --concurrency 8 \
        snapshot "${PROFILE_DIR}" --name "${PROFILE_NAME}" "${force_args[@]}"; then
        log "Browser profile snapshot completed"
        profile_ok=1
    else
        error "Browser profile snapshot FAILED"
    fi

    # Snapshot workspace
    if "${CLW_BIN}" --ref-domain "${CLW_REF_DOMAIN}" --concurrency 8 \
        snapshot "${WORKSPACE_DIR}" --name "${WORKSPACE_NAME}" "${force_args[@]}"; then
        log "Workspace snapshot completed"
        workspace_ok=1
    else
        error "Workspace snapshot FAILED"
    fi

    # Return success if at least one succeeded (partial snapshot better than none)
    if [[ ${profile_ok} -eq 1 || ${workspace_ok} -eq 1 ]]; then
        return 0
    else
        error "Both snapshots failed"
        return 1
    fi
}

# ======================================================================
# SIGNAL HANDLING
# ======================================================================
# Idempotent snapshot handler. A trapped signal fires both the named trap AND
# the EXIT trap (the shell exits after the handler), so without a guard
# snapshot_all would run twice. The guard also defends against stacked SIGTERM
# (a second SIGTERM during snapshot drain must not re-enter the handler).
#
# PID 1 trap delivery (corelink-runners incident 2026-08-23): the kernel does
# NOT deliver a signal whose disposition is DEFAULT to PID 1. This script
# stays PID 1 (we do NOT `exec supervisord`) and installs a real trap here so
# SIGTERM/SIGINT/EXIT all reach the handler. See the corelink-runners
# entrypoint.sh for the exact pattern this mirrors.
SNAPSHOT_TAKEN=0
# Sentinel written by WP-06 onStop when it has already snapshotted. If
# present, the entrypoint skips its own snapshot_all to avoid clw's
# per-tenant lock-file race. WP-06 must remove the file at the start of
# onStart so a subsequent container start sees a clean state.
ONSTOP_SNAPSHOT_SENTINEL="/data/.clw-snapshotted-by-onstop"
do_snapshot() {
    if [[ ${SNAPSHOT_TAKEN} -eq 1 ]]; then
        log "snapshot already in progress or done; skipping duplicate"
        return 0
    fi
    if [[ -f "${ONSTOP_SNAPSHOT_SENTINEL}" ]]; then
        log "onStop already snapshotted (sentinel ${ONSTOP_SNAPSHOT_SENTINEL}); entrypoint skipping to avoid clw lock contention"
        SNAPSHOT_TAKEN=1
        return 0
    fi
    SNAPSHOT_TAKEN=1
    snapshot_all
}

# Forward TERM/INT to the supervisord child FIRST so it can shut its children
# down gracefully (chromium flushes dirty buffers, x11vnc closes sockets, etc.)
# BEFORE we snapshot. Without this ordering, snapshot_all fires while the kids
# are still writing to /data/chrome and we capture a torn profile. The
# dead `forward_then_snapshot` helper that previously existed here conflated
# this intent with implementation; the live handler is term_handler below.
SUPERVISORD_PID=""

# Bounded TERM→KILL escalation. CF Containers give PID 1 "up to 15 minutes to
# exit after SIGTERM" and then send SIGKILL to the container. The grace is
# overridable for the signal-discipline test only.
RUNNER_TERM_GRACE_SECS="${RUNNER_TERM_GRACE_SECS:-840}"
# Per-program graceful shutdown window BEFORE we snapshot. Matches the longest
# stopwaitsecs across all 6 supervisord programs (chromium=30s for buffer
# flush) with a small margin.
SUPERVISORD_TERM_GRACE_SECS="${SUPERVISORD_TERM_GRACE_SECS:-30}"
term_handler() {
    local sig="$1"
    if [[ -n "${SUPERVISORD_PID}" ]] && kill -0 "${SUPERVISORD_PID}" 2>/dev/null; then
        log "TERM: forwarding SIG${sig} to supervisord (pid=${SUPERVISORD_PID}) — waiting up to ${SUPERVISORD_TERM_GRACE_SECS}s for graceful child shutdown before snapshot"
        kill -TERM "${SUPERVISORD_PID}" 2>/dev/null || true
        local waited=0
        while kill -0 "${SUPERVISORD_PID}" 2>/dev/null && [[ ${waited} -lt ${SUPERVISORD_TERM_GRACE_SECS} ]]; do
            sleep 1
            waited=$((waited + 1))
        done
    fi
    do_snapshot
    if [[ -n "${SUPERVISORD_PID}" ]] && kill -0 "${SUPERVISORD_PID}" 2>/dev/null; then
        log "TERM: escalating to SIGKILL in ${RUNNER_TERM_GRACE_SECS}s if supervisord does not exit"
        (
            for _ in $(seq 1 "${RUNNER_TERM_GRACE_SECS}"); do
                kill -0 "${SUPERVISORD_PID}" 2>/dev/null || exit 0
                sleep 1
            done
            if kill -0 "${SUPERVISORD_PID}" 2>/dev/null; then
                log "TERM: supervisord (pid=${SUPERVISORD_PID}) still alive ${RUNNER_TERM_GRACE_SECS}s after SIGTERM — escalating to SIGKILL"
                kill -KILL "${SUPERVISORD_PID}" 2>/dev/null || true
            fi
        ) &
    fi
}

trap 'term_handler TERM' TERM
trap 'term_handler INT' INT
trap 'do_snapshot' EXIT
trap 'do_snapshot; log "manual snapshot via SIGUSR1"' USR1
trap 'error "uncaught ERR at line $LINENO"; do_snapshot' ERR

# ======================================================================
# MAIN
# ======================================================================
main() {
    log "=== CoreLink Runner DevEnv Container Starting ==="
    log "Workspace: ${WORKSPACE_NAME} | Profile: ${PROFILE_NAME}"
    log "Tenant: ${CLW_TENANT} | RefDomain: ${CLW_REF_DOMAIN}"
    
    # 1. Validate environment
    if ! validate_env; then
        error "Environment validation failed, exiting"
        exit 1
    fi
    
    # 2. Hydrate browser profile (non-fatal if no existing snapshot)
    if ! hydrate_profile; then
        error "Browser profile hydration failed, exiting"
        exit 1
    fi
    
    # 3. Hydrate workspace (non-fatal if no existing snapshot)
    if ! hydrate_workspace; then
        error "Workspace hydration failed, exiting"
        exit 1
    fi
    
    # 4. Start supervisord (manages all processes) — bash stays PID 1 so the
    #    EXIT/SIGTERM traps above can actually fire (the `exec` pattern would
    #    destroy them). Supervisord is run in the background and reaped by a
    #    `wait` loop that handles the signal-interrupted-wait case.
    log "Starting supervisord..."
    supervisord -c /etc/supervisord.conf &
    SUPERVISORD_PID=$!
    log "supervisord started (pid=${SUPERVISORD_PID})"

    # Reap the supervisord child. A trapped signal INTERRUPTS `wait`, which
    # then returns >128 while the child is still alive. Re-wait until the
    # child is genuinely gone, or PID 1 would fall through and exit while
    # supervisord still holds the box.
    local rc=0
    while :; do
        wait "${SUPERVISORD_PID}" 2>/dev/null || rc=$?
        [[ ${rc} -le 128 ]] && break
        kill -0 "${SUPERVISORD_PID}" 2>/dev/null || break
        rc=0
    done
    log "supervisord exited (rc=${rc})"
    return ${rc}
}

main "$@"
```

### 3.3 supervisord.conf (Exact)

```ini
# deploy/cloudflare/supervisord.conf
# CoreLink Runner DevEnv - Process Manager Configuration
# Manages: xvfb, chromium, x11vnc, noVNC, ttyd, code-server

[supervisord]
nodaemon=true
logfile=/dev/stdout
logfile_maxbytes=0
loglevel=info
pidfile=/run/supervisord.pid
minfds=4096
minprocs=256

# ──────────────────────────────────────────────────────────────────────
# PROGRAM: xvfb (Virtual Framebuffer)
# ──────────────────────────────────────────────────────────────────────
# Direct Xvfb (NOT `xvfb-run` — the latter is a one-shot wrapper for a child
# command, not a long-running supervisor target; using it under supervisord
# makes Xvfb a CHILD of a process that is itself being supervised, and any
# supervisord signal to the wrapper leaks the SIGKILL bypass to Xvfb). Direct
# Xvfb is the pattern used by every code-server / kasm / Selkies image.
[program:xvfb]
command=/usr/bin/Xvfb :99 -screen 0 1920x1080x24 +extension RANDR +extension GLX +extension RENDER -ac
autorestart=true
startsecs=10
startretries=5
priority=10
stopsignal=TERM
stopwaitsecs=15
stdout_logfile=/dev/stdout
stdout_logfile_maxbytes=0
stderr_logfile=/dev/stderr
stderr_logfile_maxbytes=0
environment=DISPLAY=":99",HOME="/home/coder"
user=coder

# ──────────────────────────────────────────────────────────────────────
# PROGRAM: chromium (Browser for Claude/Codex)
# ──────────────────────────────────────────────────────────────────────
# Headless mode for container. `VaapiVideoDecoder` is a ChromeOS/Android flag
# and is silently ignored (or warns) on Linux; `WebRTC-H264WithOpenH264FFmpeg`
# requires libopenh264-* packages which are NOT installed (WP-02) — drop both.
# `--no-sandbox` is required because we run as non-root with `--user-data-dir`
# writable; the trade-off is documented in the WP-02 risk register (R-H8).
[program:chromium]
command=chromium \
  --headless=new \
  --user-data-dir=/data/chrome \
  --password-store=basic \
  --no-first-run \
  --no-default-browser-check \
  --window-size=1280,720 \
  --force-device-scale-factor=1 \
  --disable-background-networking \
  --disable-sync \
  --disable-extensions-http-throttling \
  --disable-component-extensions-with-background-pages \
  --disable-background-timer-throttling \
  --disable-renderer-backgrounding \
  --disable-features=TranslateUI,BlinkGenPropertyTrees \
  --remote-debugging-port=9222 \
  --remote-debugging-address=127.0.0.1 \
  --disable-gpu-sandbox \
  --no-sandbox \
  --disable-setuid-sandbox \
  --disable-dev-shm-usage \
  --memory-pressure-off \
  --max_old_space_size=4096
autorestart=true
startsecs=15
startretries=3
priority=20
stopsignal=TERM
stopwaitsecs=30
stdout_logfile=/dev/stdout
stdout_logfile_maxbytes=0
stderr_logfile=/dev/stderr
stderr_logfile_maxbytes=0
environment=DISPLAY=":99",HOME="/home/coder",XDG_RUNTIME_DIR="/tmp/runtime-coder"
user=coder

# ──────────────────────────────────────────────────────────────────────
# PROGRAM: x11vnc (VNC Server for xvfb display)
# ──────────────────────────────────────────────────────────────────────
# Lens 3: Frame cap (~24 FPS) via `-wait 40 -defer 20` + Tight encoding
# to eliminate 40 MB/s bandwidth saturation over WAN.
[program:x11vnc]
command=x11vnc -display :99 -forever -shared -rfbport 5900 -noxdamage -noxfixes -noxrecord -nolookup -passwd "" -wait 40 -defer 20 -listen 127.0.0.1 -localhost
autorestart=true
startsecs=5
startretries=3
priority=30
stopsignal=TERM
stopwaitsecs=15
stdout_logfile=/dev/stdout
stdout_logfile_maxbytes=0
stderr_logfile=/dev/stderr
stderr_logfile_maxbytes=0
environment=DISPLAY=":99",HOME="/home/coder"
user=coder

# ──────────────────────────────────────────────────────────────────────
# PROGRAM: noVNC (WebSocket → VNC Proxy via websockify)
# ──────────────────────────────────────────────────────────────────────
[program:novnc]
command=websockify --web /usr/share/novnc --heartbeat 30 6080 localhost:5900
autorestart=true
startsecs=5
startretries=3
priority=40
stopsignal=TERM
stopwaitsecs=10
stdout_logfile=/dev/stdout
stdout_logfile_maxbytes=0
stderr_logfile=/dev/stderr
stderr_logfile_maxbytes=0
user=coder

# ──────────────────────────────────────────────────────────────────────
# PROGRAM: ttyd (Terminal over WebSocket)
# ──────────────────────────────────────────────────────────────────────
# `-c <token>` is a ttyd 1.7.x credential flag — the WebSocket URL must
# include `?token=<token>`. The token is generated by RunnerDevEnvDO and
# injected via TTYD_CRED env (WP-05). NO `-W` (no-auth) on a public bind
# means anyone with the URL gets a shell (H9).
[program:ttyd]
command=ttyd -p 7681 -c ${TTYD_CRED} --writable --cwd /data/workspace --title-fixed "CoreLink DevEnv" --term xterm-256color bash -l
autorestart=true
startsecs=5
startretries=3
priority=40
stopsignal=TERM
stopwaitsecs=10
stdout_logfile=/dev/stdout
stdout_logfile_maxbytes=0
stderr_logfile=/dev/stderr
stderr_logfile_maxbytes=0
environment=HOME="/home/coder",TERM="xterm-256color",SHELL="/bin/bash"
user=coder

# ──────────────────────────────────────────────────────────────────────
# PROGRAM: exec-server (Internal Command Execution on port 9090)
# ──────────────────────────────────────────────────────────────────────
# Reaches inside the container for clw operations, health probes, and
# directory creation via WP-06 / WP-04. Bound to 0.0.0.0:9090.
[program:exec-server]
command=/usr/local/bin/exec-server
autorestart=true
startsecs=1
startretries=5
priority=1
stopsignal=TERM
stopwaitsecs=5
stdout_logfile=/dev/stdout
stdout_logfile_maxbytes=0
stderr_logfile=/dev/stderr
stderr_logfile_maxbytes=0
environment=HOME="/home/coder",TOOLCHAIN_DIR="/data/workspace",PORT="9090"
user=coder

# ──────────────────────────────────────────────────────────────────────
# PROGRAM: code-server (VS Code in Browser)
# ──────────────────────────────────────────────────────────────────────
# `code-server --auth password` (H7) — the password is generated by
# RunnerDevEnvDO and injected via CODE_SERVER_PASSWORD env (WP-05). The
# WebSocket proxy is the only auth gate; this is defense-in-depth. The
# password file is also written for code-server's --user-data-dir compat.
[program:code-server]
command=code-server --bind-addr 0.0.0.0:8080 --auth password --disable-telemetry --disable-workspace-trust /data/workspace
autorestart=true
startsecs=5
startretries=3
priority=40
stopsignal=TERM
stopwaitsecs=15
stdout_logfile=/dev/stdout
stdout_logfile_maxbytes=0
stderr_logfile=/dev/stderr
stderr_logfile_maxbytes=0
environment=HOME="/home/coder",SHELL="/bin/bash",XDG_CONFIG_HOME="/home/coder/.config",PASSWORD="${CODE_SERVER_PASSWORD}"
user=coder

# ──────────────────────────────────────────────────────────────────────
# EVENT LISTENERS (Optional: for health checks / logging)
# ──────────────────────────────────────────────────────────────────────

# [eventlistener:health_logger]
# command=python3 /usr/local/bin/health_logger.py
# events=PROCESS_STATE_START,PROCESS_STATE_STOP,PROCESS_STATE_FATAL
```

---

## 4. Acceptance Criteria (DoD)

| # | Criterion | Verification Method |
|---|-----------|---------------------|
| 1 | `entrypoint.sh` executable and runs without syntax errors | `bash -n entrypoint.sh` passes; `./entrypoint.sh` starts |
| 2 | Validates all required env vars before proceeding | Missing `CLW_TENANT` → exits with clear error message |
| 3 | `clw hydrate` called for both profile and workspace on start | Logs show "Hydrating browser profile" + "Hydrating workspace" |
| 4 | `clw snapshot --force` called on EXIT/SIGTERM/SIGINT | `trap` commands present; manual `docker kill` triggers snapshot |
| 5 | `supervisord.conf` defines all 7 required programs | `exec-server`, `xvfb`, `chromium`, `x11vnc`, `novnc`, `ttyd`, `code-server` present |
| 6 | Chromium runs with `--password-store=basic` (no libsecret) | Config shows flag; no libsecret errors in logs |
| 7 | Chromium runs headless with remote debugging on 9222 | `--remote-debugging-port=9222 --remote-debugging-address=127.0.0.1` (loopback-only; access via in-container exec-server on port 9090, or via WP-05 if debugging UI is needed) |
| 8 | noVNC proxies to x11vnc:5900 on port 6080 | `websockify 6080 localhost:5900` |
| 9 | ttyd serves terminal on port 7681 with working dir `/data/workspace` | Config shows `-p 7681 --cwd /data/workspace` |
| 10 | code-server serves on port 8080 with auth=password | Config shows `--bind-addr 0.0.0.0:8080 --auth password`; password injected by WP-05 via `CODE_SERVER_PASSWORD` env |
| 11 | All processes run as non-root user `coder` | `USER coder` in Dockerfile; supervisord inherits |
| 12 | Signal traps cover EXIT, SIGTERM, SIGINT | `trap 'snapshot_all' EXIT SIGTERM SIGINT` present |
| 13 | `snapshot_all` runs `clw snapshot --force` for both dirs | Function calls both with `--force --ref-domain runner` |

---

## 5. Invariants

| Invariant | Description |
|-----------|-------------|
| **I1** | `entrypoint.sh` exits non-zero if any required env var missing |
| **I2** | `clw hydrate` never fails the container start (exit code 2 = no snapshot = OK) |
| **I3** | `snapshot_all` executes on ALL exit paths (normal, SIGTERM, SIGINT, crash) |
| **I4** | `snapshot_all` attempts BOTH profile and workspace snapshots |
| **I5** | supervisord manages exactly 6 processes (no more, no less) |
| **I6** | All processes run under `coder` user (UID 1000) |
| **I7** | Chromium `--password-store=basic` always set (no libsecret dependency) |
| **I8** | Ports 6080, 7681, 8080, 5900, 9222 used as specified (no conflicts) |
| **I9** | `exec supervisord` is final command (PID 1 in container) |

---

## 6. Quality Standards (SOTA)

| Standard | Requirement |
|----------|-------------|
| **Shell Safety** | `set -euo pipefail` at top; all variables quoted; no unquoted expansions |
| **Error Handling** | Validation function returns explicit exit codes; `main` exits on failure |
| **Signal Correctness** | `trap` on EXIT covers all termination paths; SIGTERM/SIGINT explicit |
| **Logging** | Structured prefix `[devenv-entrypoint] ISO8601` for all output |
| **Non-Root** | All processes inherit `coder` user; no `sudo` or privilege escalation |
| **Idempotency** | `snapshot_all` safe to call multiple times (clw handles `--force`) |

---

## 7. Completeness Checklist

- [ ] `entrypoint.sh` created with exact content above
- [ ] `supervisord.conf` created with exact content above
- [ ] Both files copied to image in Dockerfile (WP-02)
- [ ] `entrypoint.sh` executable (`chmod +x`)
- [ ] `bash -n entrypoint.sh` passes (syntax check)
- [ ] `supervisord -c supervisord.conf --validate` passes (config validation)
- [ ] Container starts all 6 processes in correct order (xvfb → chromium → x11vnc → noVNC/ttyd/code-server)
- [ ] Manual `docker kill` triggers snapshot (verify logs show snapshot messages)
- [ ] Code review completed by corelink-runners TL

---

## 8. Self-Check Points (Agent Evaluation)

### Self-Check 1: Signal Coverage Completeness
> **Question:** Does the signal handling cover ALL ways the container can terminate?
> 
> **Verification:**
> - [ ] `trap 'snapshot_all' EXIT` — covers normal exit, `exit`, `return` from main
> - [ ] `trap 'snapshot_all' SIGTERM` — covers `docker stop`, `docker kill -s TERM`
> - [ ] `trap 'snapshot_all' SIGINT` — covers `docker kill -s INT`, Ctrl+C
> - [ ] What about SIGKILL (`docker kill -9`)? **Cannot trap** — document as known limitation
> - [ ] What about OOM kill? **Cannot trap** — kernel kills process instantly; document as known limitation

### Self-Check 2: clw Integration Correctness
> **Question:** Are `clw hydrate` and `clw snapshot` called with correct arguments matching the frozen contract?
> 
> **Verification:**
> - [ ] `hydrate` uses `--ref-domain runner` (not user)
> - [ ] `snapshot` uses `--force --ref-domain runner`
> - [ ] `--concurrency 8` matches `RunnerDevEnvDO.envVars` default
> - [ ] Exit code 2 from `hydrate` = "not found" = non-fatal (first run)
> - [ ] Profile dir = `/data/chrome`, Workspace dir = `/data/workspace` (matches Dockerfile)

### Self-Check 3: Supervisord Process Dependencies
> **Question:** Do process start priorities ensure correct dependency ordering?
> 
> **Verification:**
> - [ ] `xvfb` priority=10 (starts first, provides :99 display)
> - [ ] `chromium` priority=20 (needs xvfb display :99)
> - [ ] `x11vnc` priority=30 (needs chromium running on :99)
> - [ ] `novnc`/`ttyd`/`code-server` priority=40 (independent, need x11vnc for noVNC only)
> - [ ] `startsecs` values allow proper startup sequencing (xvfb=3, chromium=5, x11vnc=3)
> - [ ] `autorestart=true` on all (self-healing)

---

## 9. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| SIGKILL/OOM kills before snapshot | Medium | High | Document as known limitation; `clw` dedupe minimizes lost work; user can manual snapshot via `SIGUSR1` |
| Chromium crashes in headless mode | Low | Medium | `--disable-gpu-sandbox --no-sandbox` flags; supervisord `autorestart=true` |
| x11vnc fails to connect to xvfb | Low | High | `startsecs=3` + `wait 10` in x11vnc; health check in WP-06 |
| supervisord deadlock on shutdown | Very Low | Medium | `exec supervisord` ensures PID 1; signals propagate correctly |

---

## 10. Sign-Off

| Role | Name | Signature | Date |
|------|------|-----------|------|
| Author | | | |
| Reviewer (corelink-runners TL) | | | |
| Approver (TechLead) | | | |

---

**END OF WP-03**