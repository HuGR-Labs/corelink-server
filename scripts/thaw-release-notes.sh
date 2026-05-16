#!/usr/bin/env bash
# thaw-release-notes.sh — Post-v1.0.0-GA-tag release-notes front matter
# flip from `doc_status: "DRAFT"` to `doc_status: "ACTIVE"`.
#
# This script is invoked by `scripts/cut-v1-0-0-ga-tag.sh` immediately
# after the `v1.0.0-GA` annotated tag is pushed to origin (which is the
# canonical publication gate per RELEASE-NOTES-v1.0.0-GA.md front matter
# `publication_gate` field). It can also be invoked standalone if the
# Owner skipped --skip-thaw on the cutover script.
#
# What this DOES:
#   - Reads `RELEASE-NOTES-v1.0.0-GA.md` and asserts the front matter
#     currently says `doc_status: "DRAFT"`.
#   - Rewrites that line to `doc_status: "ACTIVE"`.
#   - Sets `release_date` field from "TBD (...)" to today's UTC date.
#   - Optionally strips the `> **DRAFT.** ...` banner block from the
#     body (between the first `>` block after front matter and the
#     next `---` separator). Behind --strip-banner (default ON).
#   - In --dry-run, prints the diff that would be applied but does not
#     write the file.
#
# What this does NOT do:
#   - Does not publish to docs site / marketing channels — those are
#     downstream of `doc_status: ACTIVE` flipping and run on their own
#     CI cadence.
#   - Does not commit the change — the operator decides when to commit
#     (post-cutover commit batch is typically a single "post-GA-tag
#     release-notes thaw + audit landing" commit).
#   - Does not flip any other doc_status fields anywhere else in the
#     corpus. Other thaws are out of scope for this script.
#
# Charter constraints honored:
#   - No unsafe code (bash + python3 only).
#   - Idempotent: re-running on an already-ACTIVE file is a no-op exit 0.
#   - Always exits with an explicit code; never leaves the file in a
#     partial state.
#
# Usage:
#   bash scripts/thaw-release-notes.sh --dry-run
#   bash scripts/thaw-release-notes.sh
#   bash scripts/thaw-release-notes.sh --release-notes RELEASE-NOTES-v1.0.0-GA.md
#   bash scripts/thaw-release-notes.sh --no-strip-banner
#   bash scripts/thaw-release-notes.sh --release-date 2026-05-16
#
# Exit codes:
#   0 — file already ACTIVE (no-op) OR thaw applied successfully OR
#       --dry-run completed.
#   1 — file in unexpected state (neither DRAFT nor ACTIVE; missing
#       front matter; missing doc_status field).
#   2 — invocation error (file not readable, python3 missing).

set -euo pipefail

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
DRY_RUN=0
RELEASE_NOTES="RELEASE-NOTES-v1.0.0-GA.md"
STRIP_BANNER=1
RELEASE_DATE=""

# ---------------------------------------------------------------------------
# Arg parse
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    --release-notes) RELEASE_NOTES="$2"; shift 2 ;;
    --no-strip-banner) STRIP_BANNER=0; shift ;;
    --strip-banner) STRIP_BANNER=1; shift ;;
    --release-date) RELEASE_DATE="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,/^set -euo/p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "ERROR: unknown flag: $1" >&2
      exit 2
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Logging helpers
# ---------------------------------------------------------------------------
log_info() { printf '[ thaw-release-notes ] %s\n' "$*"; }
log_ok()   { printf '[ thaw-release-notes ] OK  : %s\n' "$*"; }
log_fail() { printf '[ thaw-release-notes ] FAIL: %s\n' "$*" >&2; }

# ---------------------------------------------------------------------------
# Repo root resolution + cd
# ---------------------------------------------------------------------------
if ! REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  log_fail "not inside a git repository"
  exit 2
fi
cd "$REPO_ROOT"

if [[ ! -r "$RELEASE_NOTES" ]]; then
  log_fail "release notes not readable: $RELEASE_NOTES"
  exit 2
fi
log_info "file       : $RELEASE_NOTES"
log_info "dry-run    : $DRY_RUN"
log_info "strip-banner: $STRIP_BANNER"

if [[ -z "$RELEASE_DATE" ]]; then
  RELEASE_DATE="$(date -u +%Y-%m-%d)"
fi
log_info "release-date: $RELEASE_DATE"

# ---------------------------------------------------------------------------
# Required tooling
# ---------------------------------------------------------------------------
if ! command -v python3 >/dev/null 2>&1; then
  log_fail "python3 not on PATH"
  exit 2
fi

# ---------------------------------------------------------------------------
# Pre-flight: assert state
# ---------------------------------------------------------------------------
CURRENT_STATE="$(python3 - "$RELEASE_NOTES" <<'PY'
import re, sys, pathlib
p = pathlib.Path(sys.argv[1])
text = p.read_text(encoding="utf-8")
m = re.search(r'^doc_status:[ \t]*"([^"]+)"', text, flags=re.M)
if not m:
    print("MISSING")
    sys.exit(0)
print(m.group(1))
PY
)"

case "$CURRENT_STATE" in
  DRAFT)
    log_ok "current doc_status: DRAFT (will flip to ACTIVE)"
    ;;
  ACTIVE)
    log_ok "current doc_status: ACTIVE (already thawed; no-op)"
    exit 0
    ;;
  MISSING)
    log_fail "no doc_status field found in front matter"
    exit 1
    ;;
  *)
    log_fail "unexpected doc_status value: $CURRENT_STATE"
    exit 1
    ;;
esac

# ---------------------------------------------------------------------------
# Compute the new file content (in-memory; write only if not --dry-run)
# ---------------------------------------------------------------------------
NEW_CONTENT="$(STRIP_BANNER=$STRIP_BANNER RELEASE_DATE=$RELEASE_DATE python3 - "$RELEASE_NOTES" <<'PY'
import os, re, sys, pathlib

p = pathlib.Path(sys.argv[1])
text = p.read_text(encoding="utf-8")
strip_banner = os.environ["STRIP_BANNER"] == "1"
release_date = os.environ["RELEASE_DATE"]

# 1) doc_status: DRAFT -> ACTIVE
text = re.sub(
    r'^doc_status:[ \t]*"DRAFT"',
    'doc_status: "ACTIVE"',
    text,
    count=1,
    flags=re.M,
)

# 2) release_date: "TBD (...)" -> today UTC
text = re.sub(
    r'^release_date:[ \t]*"TBD[^"]*"',
    f'release_date: "{release_date}"',
    text,
    count=1,
    flags=re.M,
)

# 3) Optionally strip the > **DRAFT.** ...> banner block.
if strip_banner:
    # The banner is the contiguous block of lines starting with "> " that
    # appears between the closing "---" of front matter and the first
    # heading (# CoreLink ...). We strip the FIRST such > block whose first
    # word is "**DRAFT.**".
    pattern = re.compile(
        r'(\n# [^\n]*\n\n)'        # group 1: heading + blank line
        r'(> \*\*DRAFT\.\*\*.*?\n)' # group 2: banner first line
        r'((?:>[^\n]*\n)+)'         # group 3: rest of banner
        r'(\n---\n)',               # group 4: ---
        flags=re.DOTALL,
    )
    text = pattern.sub(r'\1\4', text, count=1)

sys.stdout.write(text)
PY
)"

# ---------------------------------------------------------------------------
# Diff preview / apply
# ---------------------------------------------------------------------------
TMP_OUT="$(mktemp -t thaw-release-notes.XXXXXX)"
trap 'rm -f "$TMP_OUT"' EXIT
# Preserve trailing newline if present in original
if [[ -n "$(tail -c1 "$RELEASE_NOTES")" ]]; then
  printf '%s' "$NEW_CONTENT" > "$TMP_OUT"
else
  printf '%s\n' "$NEW_CONTENT" > "$TMP_OUT"
fi

DIFF_RC=0
diff -u "$RELEASE_NOTES" "$TMP_OUT" >/dev/null 2>&1 || DIFF_RC=$?
if [[ "$DIFF_RC" -ne 0 ]]; then
  log_info "diff preview (first 40 lines):"
  diff -u "$RELEASE_NOTES" "$TMP_OUT" | sed -n '1,40p' || true
else
  log_info "no textual diff produced (unexpected for DRAFT-state input)"
fi

if [[ "$DRY_RUN" -eq 1 ]]; then
  log_info "DRY-RUN: not writing $RELEASE_NOTES"
  exit 0
fi

cp "$TMP_OUT" "$RELEASE_NOTES"
log_ok "release notes flipped DRAFT -> ACTIVE"

# Verify the resulting state
POST_STATE="$(python3 - "$RELEASE_NOTES" <<'PY'
import re, sys, pathlib
text = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
m = re.search(r'^doc_status:[ \t]*"([^"]+)"', text, flags=re.M)
print(m.group(1) if m else "MISSING")
PY
)"
if [[ "$POST_STATE" != "ACTIVE" ]]; then
  log_fail "post-write verification failed: doc_status=$POST_STATE"
  exit 1
fi
log_ok "post-write verification: doc_status=ACTIVE"
exit 0
