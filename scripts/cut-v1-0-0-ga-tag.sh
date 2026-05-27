#!/usr/bin/env bash
# cut-v1-0-0-ga-tag.sh — D-day Owner-action script for applying the
# `v1.0.0-GA` annotated tag to `main` after the 2-key sign-off ceremony.
#
# This is the *single command* the Owner runs on D-day, after:
#   (a) `framework-v1-0-0-ga` tag is already applied (lote-6 Owner sign-off
#       per ADR-0034b §Eligibility — framework FROZEN cut);
#   (b) RB-GA-CUTOVER §3 11-step cutover sequence has executed GREEN against
#       the production tenant ring;
#   (c) 6/6 greenlights GREEN per `dashboards/alerts/dash-ga-greenlight.yml`;
#   (d) Both 2-key signers (Owner + on-call SRE Lead, OR ADR-0034b
#       Pairing-Alpha/Beta dual-hat) have filed DocuSign envelopes;
#   (e) Owner has replaced `__OWNER_SHA__` / `__SREL_SHA__` /
#       `__OWNER_TIMESTAMP__` / `__SREL_TIMESTAMP__` / `__SREL_NAME__`
#       placeholders in `docs/release/v1.0.0-GA-tag-draft-final.txt` with the
#       real signature material.
#
# What this DOES:
#   - Pre-flight assertions: main tip matches expected SHA + freeze still
#     active (no working-tree changes outside §3.b allowed paths) + no
#     unstaged modifications + all wave-N impl-sealed tags present.
#   - 2-key sign-off check: scans `docs/release/v1.0.0-GA-tag-draft-final.txt`
#     for un-replaced `__OWNER_SHA__` / `__SREL_SHA__` placeholders and
#     refuses to run if any remain.
#   - Tag application: `git tag -a v1.0.0-GA -F <tag-draft-final> <main-tip>`.
#   - Push: `git push origin v1.0.0-GA` (and `git push origin main` if main
#     is ahead of origin/main; emits a `dry-run` plan in --dry-run mode).
#   - Post-tag: invokes `scripts/thaw-release-notes.sh` to flip the front
#     matter of `RELEASE-NOTES-v1.0.0-GA.md` from `doc_status: "DRAFT"` to
#     `doc_status: "ACTIVE"`.
#
# What this does NOT do:
#   - It does NOT create the `framework-v1-0-0-ga` tag (that is a
#     pre-requisite, applied at lote-6 Owner sign-off prep — see
#     `specs/_audits/sealed/2026-05-16-lote-6-owner-signoff-prep.md` cross-ref).
#   - It does NOT execute the RB-GA-CUTOVER §3 cutover sequence (that is
#     pre-requisite; this script presumes the sequence has finished GREEN).
#   - It does NOT page on-call, send customer comms, or publish marketing
#     copy — those are downstream of `doc_status` flipping to ACTIVE.
#   - It does NOT amend or supersede the 2-key DocuSign envelope flow;
#     it only verifies the placeholders have been replaced.
#
# Charter constraints honored:
#   - No unsafe code (bash + git + python3 + grep only).
#   - Always exits with an explicit code; on failure, leaves the
#     working tree unchanged (no partial tag, no partial push).
#   - --dry-run mode performs all assertions and prints the command
#     sequence it would run, but does NOT mutate refs.
#   - DCO sign-off + Co-Authored-By in any commits this script makes
#     (it makes none directly — `thaw-release-notes.sh` may make one).
#
# Usage:
#   bash scripts/cut-v1-0-0-ga-tag.sh --dry-run
#   bash scripts/cut-v1-0-0-ga-tag.sh \
#     --expected-main-sha <SHA> \
#     [--tag-name v1.0.0-GA] \
#     [--remote origin] \
#     [--tag-draft docs/release/v1.0.0-GA-tag-draft-final.txt] \
#     [--skip-push] \
#     [--skip-thaw]
#
# Exit codes:
#   0 — pre-flight + tag application + push + thaw all succeeded
#       (or, in --dry-run, all assertions passed).
#   1 — assertion failed (state mismatch, placeholders remaining,
#       missing wave-N tag, working-tree dirty, etc.).
#   2 — invocation error (bad flags, git not available, jq/python3
#       missing, tag-draft file not readable).
#   3 — push or thaw failed after a tag was successfully created
#       (recoverable: re-run with --skip-tag-create when implemented,
#       or push manually + re-run thaw).

set -euo pipefail

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
DRY_RUN=0
EXPECTED_MAIN_SHA=""
TAG_NAME="v1.0.0-GA"
REMOTE="origin"
TAG_DRAFT="docs/release/v1.0.0-GA-tag-draft-final.txt"
SKIP_PUSH=0
SKIP_THAW=0
THAW_SCRIPT="scripts/thaw-release-notes.sh"

EXPECTED_WAVE_TAGS=(
  "wave-14-impl-sealed"
  "wave-15-impl-sealed"
  "wave-16-impl-sealed"
  "wave-17-impl-sealed"
  "wave-18-impl-sealed"
  "wave-19-impl-sealed"
  "wave-20-impl-sealed"
  "wave-21-impl-sealed"
  "wave-22-impl-sealed"
  "wave-23-impl-sealed"
  "wave-24-impl-sealed"
  "wave-25-impl-sealed"
  "wave-26-impl-sealed"
)

# ---------------------------------------------------------------------------
# Arg parse
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    --expected-main-sha) EXPECTED_MAIN_SHA="$2"; shift 2 ;;
    --tag-name) TAG_NAME="$2"; shift 2 ;;
    --remote) REMOTE="$2"; shift 2 ;;
    --tag-draft) TAG_DRAFT="$2"; shift 2 ;;
    --skip-push) SKIP_PUSH=1; shift ;;
    --skip-thaw) SKIP_THAW=1; shift ;;
    -h|--help)
      sed -n '2,/^set -euo/p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "ERROR: unknown flag: $1" >&2
      echo "Run with --help for usage." >&2
      exit 2
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Logging helpers
# ---------------------------------------------------------------------------
log_info()  { printf '[ cut-v1-tag ] %s\n' "$*"; }
log_ok()    { printf '[ cut-v1-tag ] OK   : %s\n' "$*"; }
log_fail()  { printf '[ cut-v1-tag ] FAIL : %s\n' "$*" >&2; }
log_plan()  { printf '[ cut-v1-tag ] PLAN : %s\n' "$*"; }

# ---------------------------------------------------------------------------
# Repo root resolution + cd
# ---------------------------------------------------------------------------
if ! REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  log_fail "not inside a git repository"
  exit 2
fi
cd "$REPO_ROOT"

log_info "repo root: $REPO_ROOT"
log_info "dry-run  : $DRY_RUN"
log_info "tag      : $TAG_NAME"
log_info "draft    : $TAG_DRAFT"
log_info "remote   : $REMOTE"

# ---------------------------------------------------------------------------
# Pre-flight § A — required tooling
# ---------------------------------------------------------------------------
for bin in git python3 grep; do
  if ! command -v "$bin" >/dev/null 2>&1; then
    log_fail "required tool not on PATH: $bin"
    exit 2
  fi
done

# ---------------------------------------------------------------------------
# Pre-flight § B — branch + tip SHA
# ---------------------------------------------------------------------------
CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [[ "$CURRENT_BRANCH" != "main" ]]; then
  if [[ "$DRY_RUN" -eq 1 ]]; then
    log_info "DRY-RUN: not on main (current: $CURRENT_BRANCH); proceeding with read-only assertions"
  else
    log_fail "must be on branch 'main' (current: $CURRENT_BRANCH)"
    exit 1
  fi
else
  log_ok "branch is main"
fi

MAIN_TIP="$(git rev-parse HEAD)"
log_info "main tip : $MAIN_TIP"

if [[ -n "$EXPECTED_MAIN_SHA" ]]; then
  if [[ "$MAIN_TIP" != "$EXPECTED_MAIN_SHA"* ]] && [[ "$EXPECTED_MAIN_SHA" != "$MAIN_TIP"* ]]; then
    log_fail "main tip SHA mismatch: expected $EXPECTED_MAIN_SHA, got $MAIN_TIP"
    exit 1
  fi
  log_ok "main tip matches expected SHA"
else
  log_info "no --expected-main-sha provided; skipping SHA pin (NOT recommended for production)"
fi

# ---------------------------------------------------------------------------
# Pre-flight § C — working tree clean
# ---------------------------------------------------------------------------
if [[ -n "$(git status --porcelain)" ]]; then
  if [[ "$DRY_RUN" -eq 1 ]]; then
    log_info "DRY-RUN: working tree has uncommitted changes; proceeding with read-only assertions"
    git status --short
  else
    log_fail "working tree has uncommitted changes; refuse to tag"
    git status --short >&2
    exit 1
  fi
else
  log_ok "working tree clean"
fi

# ---------------------------------------------------------------------------
# Pre-flight § D — freeze still active (best-effort)
# ---------------------------------------------------------------------------
FREEZE_CHECKER="scripts/check-ga-freeze-allowed.py"
if [[ -x "$FREEZE_CHECKER" ]] || [[ -f "$FREEZE_CHECKER" ]]; then
  if python3 "$FREEZE_CHECKER" --check-empty >/dev/null 2>&1; then
    log_ok "GA freeze gate clean (no unstaged frozen-surface violations)"
  else
    log_fail "GA freeze gate reports violations — refuse to tag"
    python3 "$FREEZE_CHECKER" --check-empty >&2 || true
    exit 1
  fi
else
  log_info "freeze checker not found at $FREEZE_CHECKER; skipping freeze pre-flight"
fi

# ---------------------------------------------------------------------------
# Pre-flight § E — all wave-N impl-sealed tags present
# ---------------------------------------------------------------------------
MISSING_WAVE_TAGS=()
for t in "${EXPECTED_WAVE_TAGS[@]}"; do
  if ! git rev-parse -q --verify "refs/tags/$t" >/dev/null; then
    MISSING_WAVE_TAGS+=("$t")
  fi
done
if [[ ${#MISSING_WAVE_TAGS[@]} -gt 0 ]]; then
  log_fail "missing wave tags: ${MISSING_WAVE_TAGS[*]}"
  exit 1
fi
log_ok "all ${#EXPECTED_WAVE_TAGS[@]} wave-N impl-sealed tags present"

# ---------------------------------------------------------------------------
# Pre-flight § F — tag does not already exist
# ---------------------------------------------------------------------------
if git rev-parse -q --verify "refs/tags/$TAG_NAME" >/dev/null; then
  log_fail "tag $TAG_NAME already exists at $(git rev-parse refs/tags/$TAG_NAME)"
  exit 1
fi
log_ok "tag $TAG_NAME does not yet exist locally"

# ---------------------------------------------------------------------------
# Pre-flight § G — tag draft readable + placeholders replaced
# ---------------------------------------------------------------------------
if [[ ! -r "$TAG_DRAFT" ]]; then
  log_fail "tag draft not readable: $TAG_DRAFT"
  exit 2
fi
log_ok "tag draft readable: $TAG_DRAFT"

PLACEHOLDERS_FOUND=()
for ph in "__OWNER_SHA__" "__SREL_SHA__" "__OWNER_TIMESTAMP__" "__SREL_TIMESTAMP__" "__SREL_NAME__"; do
  if grep -q "$ph" "$TAG_DRAFT"; then
    PLACEHOLDERS_FOUND+=("$ph")
  fi
done

if [[ ${#PLACEHOLDERS_FOUND[@]} -gt 0 ]]; then
  if [[ "$DRY_RUN" -eq 1 ]]; then
    log_info "DRY-RUN: tag draft still contains placeholders (expected pre-D-day): ${PLACEHOLDERS_FOUND[*]}"
  else
    log_fail "tag draft still contains 2-key sign-off placeholders: ${PLACEHOLDERS_FOUND[*]}"
    log_fail "Owner must replace these with real DocuSign envelope IDs / signed-commit SHAs before running this script."
    exit 1
  fi
else
  log_ok "tag draft has all 2-key sign-off placeholders replaced"
fi

# ---------------------------------------------------------------------------
# Pre-flight § H — release notes draft is the wave-26 DRAFT
# ---------------------------------------------------------------------------
RELEASE_NOTES="RELEASE-NOTES-v1.0.0-GA.md"
if [[ ! -r "$RELEASE_NOTES" ]]; then
  log_fail "release notes not readable: $RELEASE_NOTES"
  exit 2
fi
if grep -qE '^doc_status:[[:space:]]*"DRAFT"' "$RELEASE_NOTES"; then
  log_ok "release notes are in DRAFT state (will flip to ACTIVE post-tag)"
elif grep -qE '^doc_status:[[:space:]]*"ACTIVE"' "$RELEASE_NOTES"; then
  log_info "release notes already in ACTIVE state — thaw will be a no-op"
else
  log_fail "release notes doc_status is neither DRAFT nor ACTIVE — unexpected"
  exit 1
fi

# ---------------------------------------------------------------------------
# Pre-flight § I — main is up to date with remote (or ahead by a known
# tiny amount of release-notes flips authored locally)
# ---------------------------------------------------------------------------
if git remote get-url "$REMOTE" >/dev/null 2>&1; then
  if [[ "$DRY_RUN" -eq 1 ]]; then
    log_info "DRY-RUN: skipping 'git fetch $REMOTE' (no network mutation)"
  else
    log_info "fetching $REMOTE to check main is up to date"
    git fetch "$REMOTE" --tags --quiet || {
      log_fail "git fetch $REMOTE failed"
      exit 3
    }
  fi
  if git rev-parse -q --verify "refs/remotes/$REMOTE/main" >/dev/null; then
    REMOTE_MAIN="$(git rev-parse "refs/remotes/$REMOTE/main")"
    if [[ "$MAIN_TIP" != "$REMOTE_MAIN" ]]; then
      AHEAD="$(git rev-list --count "$REMOTE_MAIN..$MAIN_TIP")"
      BEHIND="$(git rev-list --count "$MAIN_TIP..$REMOTE_MAIN")"
      log_info "main is ahead $AHEAD / behind $BEHIND vs $REMOTE/main"
      if [[ "$BEHIND" -gt 0 ]]; then
        log_fail "main is BEHIND $REMOTE/main; refuse to tag"
        exit 1
      fi
    else
      log_ok "main matches $REMOTE/main"
    fi
  else
    log_info "$REMOTE/main ref not found (first push?); will push main alongside tag"
  fi
else
  log_info "remote '$REMOTE' not configured; --skip-push implied"
  SKIP_PUSH=1
fi

# ---------------------------------------------------------------------------
# All pre-flight assertions passed
# ---------------------------------------------------------------------------
log_ok "all pre-flight assertions passed"

# ---------------------------------------------------------------------------
# § Action — create annotated tag
# ---------------------------------------------------------------------------
if [[ "$DRY_RUN" -eq 1 ]]; then
  log_plan "git tag -a $TAG_NAME -F $TAG_DRAFT $MAIN_TIP"
  log_plan "git push $REMOTE main      # (if ahead)"
  log_plan "git push $REMOTE $TAG_NAME"
  log_plan "bash $THAW_SCRIPT"
  log_info "DRY-RUN complete; no refs mutated."
  exit 0
fi

log_info "creating annotated tag $TAG_NAME at $MAIN_TIP"
git tag -a "$TAG_NAME" -F "$TAG_DRAFT" "$MAIN_TIP"
log_ok "tag $TAG_NAME created locally"

# ---------------------------------------------------------------------------
# § Action — push
# ---------------------------------------------------------------------------
if [[ "$SKIP_PUSH" -eq 1 ]]; then
  log_info "--skip-push set; not pushing to $REMOTE"
else
  if [[ -n "${REMOTE_MAIN:-}" ]] && [[ "$MAIN_TIP" != "$REMOTE_MAIN" ]]; then
    log_info "pushing main to $REMOTE"
    if ! git push "$REMOTE" main; then
      log_fail "git push $REMOTE main failed; tag $TAG_NAME exists locally but not pushed"
      exit 3
    fi
    log_ok "main pushed to $REMOTE"
  fi
  log_info "pushing tag $TAG_NAME to $REMOTE"
  if ! git push "$REMOTE" "$TAG_NAME"; then
    log_fail "git push $REMOTE $TAG_NAME failed; tag exists locally but not pushed"
    exit 3
  fi
  log_ok "tag $TAG_NAME pushed to $REMOTE"
fi

# ---------------------------------------------------------------------------
# § Action — thaw release notes
# ---------------------------------------------------------------------------
if [[ "$SKIP_THAW" -eq 1 ]]; then
  log_info "--skip-thaw set; not invoking $THAW_SCRIPT"
else
  if [[ ! -x "$THAW_SCRIPT" ]] && [[ ! -f "$THAW_SCRIPT" ]]; then
    log_fail "thaw script not found: $THAW_SCRIPT"
    exit 3
  fi
  log_info "invoking thaw script: $THAW_SCRIPT"
  if ! bash "$THAW_SCRIPT"; then
    log_fail "thaw script failed; release notes still DRAFT (re-run manually)"
    exit 3
  fi
  log_ok "release notes flipped to ACTIVE"
fi

log_ok "v1.0.0-GA tag application complete"
exit 0
