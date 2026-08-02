#!/usr/bin/env bash
#
# Run the whole TLA+ suite and report the truth about it.
#
# Replaces 45 hand-maintained serial workflow steps sharing one 30-minute budget.
# That structure is why `tla_check` had 0 successes in 195 runs: the first spec in
# the list does not terminate, so it ate the entire budget and the other 46 were
# skipped on every single run — the gate looked scheduled and verified nothing.
#
# Three properties this runner has that the step-list did not:
#
#   1. Per-spec timeout. One non-terminating spec cannot starve the rest.
#   2. Discovery. Every `*.tla` with a matching `*.cfg` runs; adding a spec to the
#      directory cannot silently escape the gate because someone forgot a step.
#   3. An honest, self-invalidating quarantine (specs/tla/QUARANTINE.md).
#      A quarantined spec that starts PASSING fails the build, so the list cannot
#      rot into permanent debt; a non-quarantined spec that fails also fails the
#      build, so nothing is skipped quietly.
#
# Usage: run_tla_suite.sh [--timeout SECONDS] [--workers N] [--only SPEC]
# Env:   TLC_JAR (required)   — the SHA-256-pinned jar (ADR-0042 §A1)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# EVERY directory that holds .tla specs. There is more than one, which is easy to
# miss and was missed: `specs/03_architecture/tla+/runbooks/` holds 4 specs that
# no discovery scan of `specs/tla/` will ever see. Before this list existed they
# were reachable only through `tla_runbooks_check`, a separate weekly workflow —
# so "the suite discovers everything" was true of one directory and quietly false
# of the repo. If a third location ever appears, it belongs here; a `find` over
# the whole tree is the wrong instrument, because it would also sweep up vendored
# or example specs that are not ours to gate on.
SPEC_DIRS=(
  "${REPO_ROOT}/specs/tla"
  "${REPO_ROOT}/specs/03_architecture/tla+/runbooks"
)
QUARANTINE_FILE="${REPO_ROOT}/specs/tla/QUARANTINE.md"
TIMEOUT_SECS=300
# Quarantined specs still run — that is how a stale quarantine is detected — but
# they get a smaller budget. Two of them provably do not terminate, so giving them
# the full per-spec timeout would burn it on every run to re-learn something
# already measured and written down. A shorter budget still catches the case that
# matters: a quarantined spec that has been FIXED and now completes quickly.
QUARANTINE_TIMEOUT_SECS=60
WORKERS=2
ONLY=""

while [ $# -gt 0 ]; do
  case "$1" in
    --timeout)            TIMEOUT_SECS="$2"; shift 2 ;;
    --quarantine-timeout) QUARANTINE_TIMEOUT_SECS="$2"; shift 2 ;;
    --workers)            WORKERS="$2"; shift 2 ;;
    --only)               ONLY="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [ -z "${TLC_JAR:-}" ] || [ ! -f "${TLC_JAR}" ]; then
  echo "::error::TLC_JAR is unset or missing — refusing to run an UNVERIFIED tla2tools.jar." >&2
  echo "The SHA-256 pin (ADR-0042 §A1) is the only thing standing between CI and an" >&2
  echo "arbitrary binary from a mutable upstream tag. Set TLC_JAR to the verified jar." >&2
  exit 1
fi

# `timeout` is GNU coreutils; macOS ships it as gtimeout when coreutils is installed.
TIMEOUT_BIN="timeout"
command -v timeout >/dev/null 2>&1 || TIMEOUT_BIN="gtimeout"
if ! command -v "$TIMEOUT_BIN" >/dev/null 2>&1; then
  echo "::error::neither 'timeout' nor 'gtimeout' found — cannot bound a spec run." >&2
  echo "Without a per-spec timeout one non-terminating spec starves the suite, which" >&2
  echo "is the exact failure this runner exists to prevent. Install coreutils." >&2
  exit 1
fi

# Quarantine list: the spec name in the first cell of any table row whose name
# matches a real spec file. Parsed leniently on purpose — the prose around it is
# for humans and must be free to change without breaking the gate.
quarantined() {
  grep -oE '^\| `[a-z0-9_]+`' "$QUARANTINE_FILE" 2>/dev/null \
    | tr -d '|` ' | grep -Fxq "$1"
}

pass=(); fail=(); quarantine_ok=(); quarantine_stale=()

echo "TLA+ suite — jar $(basename "$TLC_JAR"), ${WORKERS} workers, ${TIMEOUT_SECS}s per spec"
echo

# TLC writes its fingerprint/state metadata into `states/` next to the model
# unless told otherwise, and that directory is large. The self-hosted mac fleet
# reuses one workspace across runs, so left in the repo it accumulates per spec
# per run on a host that has already hit ENOSPC once. Redirect it to a
# run-scoped temp dir and drop it on the way out.
METADIR="$(mktemp -d "${TMPDIR:-/tmp}/tla-suite-meta.XXXXXX")"
trap 'rm -rf "$METADIR"' EXIT INT TERM

for spec_dir in "${SPEC_DIRS[@]}"; do
 if [ ! -d "$spec_dir" ]; then
   # A configured directory that has vanished means SPEC_DIRS and the repo have
   # drifted. Silently skipping it would shrink the suite without shrinking the
   # count anyone reads, so treat it as a failure.
   echo "  MISSING-DIR  ${spec_dir}"
   fail+=("${spec_dir} (configured in SPEC_DIRS but does not exist)")
   continue
 fi
 cd "$spec_dir" || exit 1
 echo "── ${spec_dir#"${REPO_ROOT}"/}"

 # Traces from a PREVIOUS run are stale by definition and must not be mistaken
 # for this run's evidence. The discovery loop skips them by name as well, so
 # this is belt-and-braces: it keeps the count honest even if that skip is ever
 # removed, and it stops them piling up in a reused workspace.
 rm -f ./*_TTrace_*.tla ./*_TTrace_*.bin
 for tla in *.tla; do
  [ -e "$tla" ] || continue   # empty dir: the glob stays literal
  name="${tla%.tla}"
  [ -n "$ONLY" ] && [ "$ONLY" != "$name" ] && continue
  # TLC drops `<spec>_TTrace_<epoch>.tla` beside the model when an invariant is
  # violated. They are artifacts OF a failure, not specs to check — and they are
  # generated with a .cfg, so without this they would be discovered and run as if
  # they were part of the suite, inflating the verified count with echoes of
  # failures. .gitignore keeps them out of commits; this keeps them out of a
  # local run's results.
  case "$name" in *_TTrace_*) continue ;; esac
  if [ ! -f "${name}.cfg" ]; then
    # A spec with no config is not "skipped" — it is unverified, and silent
    # unverified specs are how a suite drifts into theatre.
    echo "  MISSING-CFG  ${name}"
    fail+=("${name} (no .cfg — cannot be checked)")
    continue
  fi

  if quarantined "$name"; then budget="$QUARANTINE_TIMEOUT_SECS"; else budget="$TIMEOUT_SECS"; fi

  start=$(date +%s)
  out="$("$TIMEOUT_BIN" "$budget" java -cp "$TLC_JAR" tlc2.TLC \
        -config "${name}.cfg" -workers "$WORKERS" -fp 32 -checkpoint 0 \
        -metadir "${METADIR}/${name}" "$tla" 2>&1)"
  rc=$?
  dur=$(( $(date +%s) - start ))

  if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q "Model checking completed. No error has been found"; then
    ok=true
  else
    ok=false
  fi

  if quarantined "$name"; then
    if $ok; then
      # A quarantined spec that now passes means QUARANTINE.md is lying about
      # what this suite verifies. Fail loudly and make someone delete the row.
      echo "  STALE-QUARANTINE  ${name}  (${dur}s) — passes now"
      quarantine_stale+=("$name")
    else
      echo "  quarantined       ${name}  (${dur}s)"
      quarantine_ok+=("$name")
    fi
  else
    if $ok; then
      echo "  PASS              ${name}  (${dur}s)"
      pass+=("$name")
    else
      echo "  FAIL              ${name}  (${dur}s)"
      printf '%s\n' "$out" | grep -iE "^Error|is violated|Semantic errors|Attempted to check" | head -3 | sed 's/^/                    /'
      [ "$rc" -eq 124 ] && echo "                    (timed out after ${budget}s)"
      fail+=("$name")
    fi
  fi
 done
done

echo
echo "──────────────────────────────────────────────────────────"
echo "  verified:     ${#pass[@]}"
echo "  quarantined:  ${#quarantine_ok[@]}   (specs/tla/QUARANTINE.md)"
echo "  failed:       ${#fail[@]}"
[ "${#quarantine_stale[@]}" -gt 0 ] && echo "  stale quarantine: ${#quarantine_stale[@]}"
echo "──────────────────────────────────────────────────────────"

status=0

if [ "${#fail[@]}" -gt 0 ]; then
  echo
  echo "::error::${#fail[@]} spec(s) failed and are NOT quarantined:"
  for f in "${fail[@]}"; do echo "  - $f"; done
  echo "Either fix the spec, or add it to specs/tla/QUARANTINE.md WITH the measured"
  echo "reason. Do not delete the spec and do not loosen an invariant to get green."
  status=1
fi

if [ "${#quarantine_stale[@]}" -gt 0 ]; then
  echo
  echo "::error::${#quarantine_stale[@]} quarantined spec(s) now PASS:"
  for f in "${quarantine_stale[@]}"; do echo "  - $f"; done
  echo "Remove them from specs/tla/QUARANTINE.md. A quarantine that outlives its"
  echo "reason overstates what is unverified just as badly as a missing one"
  echo "understates it — both make this suite's output untrustworthy."
  status=1
fi

if [ "${#pass[@]}" -eq 0 ] && [ -z "$ONLY" ]; then
  # Guard against the failure mode this whole runner is a reaction to: a suite
  # that reports success because it checked nothing at all.
  echo
  echo "::error::zero specs verified — refusing to report success."
  status=1
fi

exit "$status"
