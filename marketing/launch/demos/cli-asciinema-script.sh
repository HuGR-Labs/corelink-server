#!/usr/bin/env bash
# marketing/launch/demos/cli-asciinema-script.sh
#
# Asciinema-recordable script that exercises every CLI command shown in
# 5-MIN-DEEPDIVE.md §2 + §3.1 and 60-SEC-ELEVATOR.md Beat 3. Designed to
# be run against a SANDBOX tenant — DO NOT POINT AT PRODUCTION.
#
# Usage (record):
#   asciinema rec -c "./cli-asciinema-script.sh" -t "CoreLink — 5min demo" \
#       --idle-time-limit 1.5 \
#       marketing/launch/demos/casts/5min-demo.cast
#
# Usage (dry-run, no recording):
#   ./cli-asciinema-script.sh --dry-run
#
# Pre-conditions:
#   - `corelink` CLI 1.0.0+ on $PATH
#   - $CORELINK_PAT exported with a valid SANDBOX PAT
#   - working directory writable
#   - jq installed (for JSON parsing in step 3)
#
# Every command below has been cross-checked against the canonical CLI
# subcommand surface in `crates/corelink-cli/src/main.rs` (Cli + Commands
# enums). The 7 user-facing subcommands are: ls, get, put, stat, bench,
# doctor, version. Plus: config (no PAT required) and runbook-drill
# (offline, no PAT required).
#
# Cross-refs:
#   - 5-MIN-DEEPDIVE.md  §2 + §3.1 — pace + voiceover
#   - 60-SEC-ELEVATOR.md Beat 3    — short cousin
#   - crates/corelink-cli/src/main.rs — canonical CLI surface
#   - apps/docs/docs/tutorials/quickstart-10min.mdx — public-facing tutorial

set -u
# -e intentionally omitted: this is a demo, not a CI script. We want the
# `doctor` and `bench` commands to keep going even if a check fails so
# the viewer sees what failure looks like.

# ── Cosmetic helpers ────────────────────────────────────────────────────

DEMO_PAUSE_FAST="0.6"
DEMO_PAUSE_MED="1.4"
DEMO_PAUSE_SLOW="2.6"

DRY_RUN=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --help|-h)
      echo "Usage: $0 [--dry-run]"
      echo "  --dry-run   Echo commands without executing them."
      exit 0
      ;;
  esac
done

# Print a "human typing" prompt line so the asciinema cast looks like a
# real terminal session. We print the prompt + command, sleep, then run.
typeline() {
  local prompt="\$ "
  local cmd="$*"
  printf '%s' "$prompt"
  # Simulated typing (~50 wpm) — character-by-character with a 30ms gap.
  # When recording, asciinema captures this exact cadence.
  for (( i=0; i<${#cmd}; i++ )); do
    printf '%s' "${cmd:$i:1}"
    sleep 0.03
  done
  printf '\n'
  sleep "$DEMO_PAUSE_FAST"
  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "(dry-run: would execute: $cmd)"
  else
    bash -c "$cmd" || true
  fi
  sleep "$DEMO_PAUSE_MED"
}

# Print a narrator comment (preceded by `# `) — viewers learn to read these.
narrate() {
  printf '\n# %s\n' "$*"
  sleep "$DEMO_PAUSE_FAST"
}

# Hard pause for a beat — used between major sections.
beat() {
  sleep "$DEMO_PAUSE_SLOW"
}

# ── Pre-flight ──────────────────────────────────────────────────────────

if [[ "$DRY_RUN" -eq 0 ]]; then
  if ! command -v corelink >/dev/null 2>&1; then
    echo "error: corelink CLI not on PATH; install per quickstart-10min.mdx Step 0" >&2
    exit 2
  fi
  if [[ -z "${CORELINK_PAT:-}" ]]; then
    echo "error: CORELINK_PAT not set; export a sandbox PAT before recording" >&2
    exit 2
  fi
  if ! command -v jq >/dev/null 2>&1; then
    echo "error: jq required (for parsing put --output=json in step 3)" >&2
    exit 2
  fi
fi

clear
printf '\n'
narrate "CoreLink CLI demo — sandbox tenant, ~3 minutes of commands."
narrate "Every command below is real; nothing scripted around the CLI."
beat

# ── Step 1 — version + doctor ───────────────────────────────────────────

narrate "Step 1 — verify install + 8 readiness checks."

typeline 'corelink version'

typeline 'corelink doctor'

beat

# ── Step 2 — configure tenant default ───────────────────────────────────

narrate "Step 2 — set tenant default + show config (PAT auto-redacted)."

# Tenant ID is read from doctor output in the live demo; for the script
# we let the operator set it via the env var DEMO_TENANT (falls back to
# a placeholder so the dry-run doesn't crash).
DEMO_TENANT="${DEMO_TENANT:-sandbox-7f3a}"

typeline "corelink config set defaults.tenant_id ${DEMO_TENANT}"

typeline 'corelink config list'

beat

# ── Step 3 — first put + stat ───────────────────────────────────────────

narrate "Step 3 — first content-addressable write."

# Make a sample artifact with a fresh timestamp so the digest is unique
# per recording — important if you re-record against the same tenant.
typeline 'echo "hello, corelink — $(date -u +%FT%TZ)" > /tmp/hello.txt'

typeline 'corelink put /tmp/hello.txt'

# Capture digest for the rest of the demo. We use --output json so the
# pipe to jq is exact; the JSON schema is documented at
# docs/cli/json-output-schema.md.
typeline 'corelink put /tmp/hello.txt --output json | tee /tmp/put.json'

if [[ "$DRY_RUN" -eq 0 ]]; then
  DIGEST=$(jq -r .digest /tmp/put.json 2>/dev/null || echo "af1c3e9bDEMO")
else
  DIGEST="af1c3e9bDRYRUN"
fi
export DIGEST

narrate "Captured digest: $DIGEST"

typeline "corelink stat ${DIGEST}"

beat

# ── Step 4 — retrieve "from another machine" ────────────────────────────

narrate "Step 4 — retrieve by digest + client-side BLAKE3 verify."

typeline "corelink get ${DIGEST} --output /tmp/restored.txt"

typeline 'diff -q /tmp/hello.txt /tmp/restored.txt && echo "MATCH (no drift)"'

beat

# ── Step 5 — audit / list ───────────────────────────────────────────────

narrate "Step 5 — list the tenant's recent CAS entries (audit surface)."

typeline "corelink ls --tenant ${DEMO_TENANT} --limit 5"

# JSON form for SIEM piping. Listed in 5-MIN-DEEPDIVE.md §4 cross-ref.
typeline "corelink ls --tenant ${DEMO_TENANT} --limit 5 --output json"

beat

# ── Step 6 — micro-benchmark (optional, depending on demo length) ───────

narrate "Step 6 — round-trip micro-benchmark vs the cluster."

# Default flavour: full write+read round-trip (100 ops). Subcommand
# accepts --write, --read, --full as mutually exclusive; default when
# none given is the full flavour. See main.rs Commands::Bench.
typeline 'corelink bench --full'

beat

# ── Step 7 — version reprint (signs-off the cast) ───────────────────────

narrate "Done. corelink.humangr.com for the 10-minute tutorial."

typeline 'corelink version'

beat

# ── End ─────────────────────────────────────────────────────────────────

printf '\n'
narrate "Cast complete. Save as marketing/launch/demos/casts/<name>.cast"
sleep 1
