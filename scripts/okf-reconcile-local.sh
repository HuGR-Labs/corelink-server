#!/usr/bin/env bash
# okf-reconcile-local.sh — the OKF self-healing "LLM half", run LOCALLY with your
# Codex CLI auth (your ChatGPT login/subscription — no CI secret or API key).
#
# It detects whether a code change drifted any OKF concept's cited lines (the deterministic
# `okf_reconcile.py` reporter) and, only if drift exists, runs `codex exec` headless on the
# `okf-reconcile` skill to re-anchor the affected concepts to current code (claims re-verified,
# checkpoints advanced), then validates fail-closed.
#
# USAGE:
#   scripts/okf-reconcile-local.sh --check   # report drift only; exit 1 if drift (for hooks/scripts)
#   scripts/okf-reconcile-local.sh --doctor  # verify local/manual activation prerequisites
#   scripts/okf-reconcile-local.sh           # reconcile IN this worktree, validate, leave staged for review
#   scripts/okf-reconcile-local.sh --pr      # reconcile in an ISOLATED worktree off origin/main, validate,
#                                            # commit + open a PR (never touches your working tree)
#
# REQUIRES: `codex` on PATH (you're already logged in); `gh` for --pr. Run from the repo root.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
AGENT_GUARD_DIR="$PWD/scripts"
MODE="${1:-inplace}"

if [[ "$MODE" == "--doctor" ]]; then
  problems=0
  hook_dir="$(git rev-parse --git-path hooks 2>/dev/null || true)"
  if [[ -n "$hook_dir" ]]; then
    echo "OKF local hook path: $hook_dir"
  else
    echo "OKF local hook path: NOT resolvable (configure core.hooksPath or use the repository hooks directory)"
    problems=$((problems + 1))
  fi
  hook_file="${hook_dir:+$hook_dir/post-merge}"
  if [[ -n "$hook_file" && -x "$hook_file" ]]; then
    echo "OKF post-merge hook: executable ($hook_file)"
  else
    echo "OKF post-merge hook: NOT executable or missing ($hook_file)"
    problems=$((problems + 1))
  fi
  if command -v python3 >/dev/null 2>&1; then
    echo "OKF Python: available ($(command -v python3))"
  else
    echo "OKF Python: NOT available (required by reporter/validator)"
    problems=$((problems + 1))
  fi
  if command -v codex >/dev/null 2>&1; then
    echo "OKF Codex CLI: available ($(command -v codex))"
  else
    echo "OKF Codex CLI: NOT available (install/login Codex before reconciling)"
    problems=$((problems + 1))
  fi
  if [[ -x scripts/okf-reconcile-local.sh ]]; then
    echo "OKF wrapper: executable"
  else
    echo "OKF wrapper: NOT executable (chmod +x scripts/okf-reconcile-local.sh)"
    problems=$((problems + 1))
  fi
  if [[ "${2:-}" == "--pr" ]]; then
    if command -v gh >/dev/null 2>&1; then
      echo "OKF gh: available ($(command -v gh))"
    else
      echo "OKF gh: NOT available (required only for --pr)"
      problems=$((problems + 1))
    fi
  fi
  if (( problems )); then
    echo "OKF activation: MANUAL ONLY until the reported prerequisites are fixed"
    exit 1
  fi
  echo "OKF activation: local/manual hook is ready (no automatic installation is promised)"
  exit 0
fi

drift_count() { python3 scripts/okf_reconcile.py --json | python3 -c "import json,sys; print(json.load(sys.stdin)['stale_count'])"; }

snapshot_repo_state() {
  local git_cmd="${OKF_SNAPSHOT_GIT:-git}"
  # The snapshot includes git for-each-ref, but always through the real binary
  # after the agent guard is installed.
  {
    echo HEAD; "$git_cmd" rev-parse HEAD
    echo BRANCH; "$git_cmd" symbolic-ref --quiet --short HEAD || true
    echo REFS; "$git_cmd" for-each-ref --format='%(refname) %(objectname)'
    echo REMOTES; "$git_cmd" remote -v
    echo WORKTREES; "$git_cmd" worktree list --porcelain
  }
}

profile_safe_path() {
  local value="$1"
  [[ "$value" == /* ]] || return 1
  case "$value" in
    *'"'*|*\\*|*$'\n'*|*$'\r'*|*$'\t'*) return 1 ;;
  esac
}

make_secure_temp_root() {
  local base="$1" repo_root="$2" git_common="$3" root
  [[ -d "$base" ]] || return 1
  base="$(cd -- "$base" && pwd -P)" || return 1
  case "$base/" in
    "$repo_root/"*|"$git_common/"*) return 1 ;;
  esac
  root="$(mktemp -d "$base/okf-reconcile.XXXXXX")" || return 1
  chmod 700 "$root" || return 1
  printf '%s\n' "$root"
}

prepare_agent_guard() {
  local guard_root="$1" marker_endpoint="$2" real_git="$3"
  mkdir -p "$guard_root/bin" "$guard_root/codex-tmp"
  ln -s "$AGENT_GUARD_DIR/okf-agent-git-guard.sh" "$guard_root/bin/git"
  ln -s "$AGENT_GUARD_DIR/okf-agent-gh-guard.sh" "$guard_root/bin/gh"
  export OKF_GUARD_MARKER="$marker_endpoint" OKF_REAL_GIT="$real_git"
  export PATH="$guard_root/bin:$PATH"
}

run_codex_read_only() {
  local codex_bin="$1" guard_root="$2" repo_root="$3"
  local profile="$guard_root/profile"
  if [[ "$(uname -s)" == "Darwin" && -x /usr/bin/sandbox-exec ]]; then
    # Seatbelt is deny-by-default.  The only writable paths are the two doc
    # trees and the private temp directory supplied as TMPDIR to Codex.
    profile_safe_path "$repo_root" || { echo "ERROR: unsafe repository path" >&2; return 98; }
    profile_safe_path "$guard_root" || { echo "ERROR: unsafe guard path" >&2; return 98; }
    cat > "$profile" <<EOF
(version 1)
(deny default)
(allow process-exec)
(allow process-fork)
(allow sysctl-read)
(allow file-read*)
(allow file-write* (subpath "$repo_root/docs/knowledge"))
(allow file-write* (subpath "$repo_root/docs/internal/okf-wiki"))
(allow file-write* (subpath "$guard_root/codex-tmp"))
(allow file-read* (subpath "/dev"))
(allow file-write* (subpath "/dev"))
(deny network-outbound)
(deny network-inbound)
EOF
    TMPDIR="$guard_root/codex-tmp" /usr/bin/sandbox-exec -p "$(<"$profile")" "$codex_bin" exec \
      --ephemeral \
      --ignore-user-config \
      --sandbox workspace-write \
      -
  elif [[ "$(uname -s)" == "Linux" ]]; then
    local bwrap
    bwrap="$(command -v bwrap || true)"
    [[ -x "$bwrap" ]] || { echo "ERROR: bubblewrap unavailable; refusing Codex reconciliation." >&2; return 98; }
    profile_safe_path "$repo_root" || { echo "ERROR: unsafe repository path" >&2; return 98; }
    profile_safe_path "$guard_root" || { echo "ERROR: unsafe guard path" >&2; return 98; }
    TMPDIR="$guard_root/codex-tmp" "$bwrap" --die-with-parent --unshare-net --ro-bind / / \
      --bind "$repo_root/docs/knowledge" "$repo_root/docs/knowledge" \
      --bind "$repo_root/docs/internal/okf-wiki" "$repo_root/docs/internal/okf-wiki" \
      --bind "$guard_root/codex-tmp" "$guard_root/codex-tmp" --dev /dev --proc /proc \
      "$codex_bin" exec --ephemeral --ignore-user-config --sandbox workspace-write -
  else
    echo "ERROR: unsupported host OS; refusing Codex reconciliation." >&2
    return 98
  fi
}

# ── --check: fast, no agent, no cost ──────────────────────────────────────────
STALE="$(drift_count)"
if [[ "$MODE" == "--check" ]]; then
  echo "OKF drift: stale_count=$STALE"
  if [[ "$STALE" == "0" ]]; then
    exit 0
  fi
  python3 scripts/okf_reconcile.py
  exit 1
fi
if [[ "$STALE" == "0" ]]; then
  echo "OKF: 0 drift — nothing to reconcile."; exit 0
fi
command -v codex >/dev/null || { echo "ERROR: 'codex' CLI not on PATH (run 'codex login' first)."; exit 2; }

# Quoted heredoc, not a single-quoted string: the prompt below contains an
# apostrophe ("file's anchor"), which terminated the old PROMPT='...' early and
# left the remainder to be parsed as shell — `bash -n` rejected the whole file,
# so this script could never run. A heredoc cannot be broken by prose.
PROMPT=$(cat <<'OKF_PROMPT'
You are reconciling the OKF architecture wiki after a code change, using the repo skill
.claude/skills/okf-reconcile/SKILL.md. The deterministic reporter has the worklist:
run `python3 scripts/okf_reconcile.py` to see which concepts drifted and which cited ranges moved.
For EACH stale concept: open its cited code at current HEAD, re-anchor its `# Citations` ranges (and
inline path:line) to the CURRENT lines, and CONFIRM the claim still holds — if the code changed the
behavior, UPDATE the claim to the current truth HONESTLY (never a blind line-bump). Advance each
touched concept checkpoint_sha to the current HEAD SHA (the C5b anti-phantom check needs a real body
edit alongside it — your cite/claim edit satisfies it), and for a concept that carries `source_blobs`,
advance each re-authored file's anchor to `git hash-object <path>` — that anchor is what C5 compares
against, and NEVER delete one to clear a red (C4c refuses it). Then `python3 scripts/okf_index.py`.
HARD CONSTRAINTS: edit ONLY docs/knowledge/** + docs/internal/okf-wiki/** + docs/knowledge/index.md;
NEVER edit code (crates/worker/apps/scripts/migrations) — you document it, not change it. Every cite
must resolve to a real path:line that performs the claim; every cited file in source_files. Read-only git
commands and deterministic helpers are allowed. Do NOT commit, push, mutate refs/remotes/worktrees, invoke
gh, or open a PR.
STOP when `python3 scripts/validate_okf.py` is 0 stale / 0 drift for the concepts you fixed.
OKF_PROMPT
)

run_agent_and_validate() {
  echo ">> $STALE concept(s) drifted — running the okf-reconcile skill (Codex, local auth)…"
  local state_before state_after agent_rc guard_root codex_bin git_common repo_root real_git
  local saved_path saved_snapshot_git state_root marker_fifo marker_fd marker_log marker_pid marker_end all_changes
  repo_root="$(git rev-parse --show-toplevel)"
  git_common="$(git rev-parse --git-common-dir)"
  git_common="$(cd -- "$git_common" && pwd -P)"
  state_root="$(make_secure_temp_root "${TMPDIR:-/tmp}" "$repo_root" "$git_common")" || {
    echo "ERROR: TMPDIR is unavailable or unsafe; refusing Codex reconciliation." >&2
    return 98
  }
  guard_root="$(make_secure_temp_root "${TMPDIR:-/tmp}" "$repo_root" "$git_common")" || {
    rm -rf "$state_root"
    echo "ERROR: unable to create private Codex temp directory." >&2
    return 98
  }
  state_before="$state_root/state-before"; state_after="$state_root/state-after"
  # Keep the marker endpoint alongside the protected snapshot, never in a
  # sandbox-writable directory.  Once opened, the FIFO is unlinked and the
  # guard receives only the inherited descriptor path, so the agent cannot
  # replace or unlink the endpoint by pathname.
  marker_fifo="$state_root/mutation.fifo"; marker_log="$state_root/mutations.log"
  marker_end="okf-end-$RANDOM-$RANDOM"
  mkfifo "$marker_fifo"
  exec {marker_fd}<>"$marker_fifo"
  rm -f "$marker_fifo"
  marker_endpoint="/dev/fd/$marker_fd"
  saved_path="$PATH"
  saved_snapshot_git="${OKF_SNAPSHOT_GIT-}"
  trap 'if [[ -n "${marker_pid-}" ]]; then kill "$marker_pid" 2>/dev/null || true; fi; if [[ -n "${marker_fd-}" ]]; then exec {marker_fd}>&- 2>/dev/null || true; fi; PATH="$saved_path"; export PATH; if [[ -n "$saved_snapshot_git" ]]; then export OKF_SNAPSHOT_GIT="$saved_snapshot_git"; else unset OKF_SNAPSHOT_GIT; fi; unset OKF_GUARD_MARKER OKF_REAL_GIT; rm -rf "$state_root" "$guard_root"' RETURN
  codex_bin="$(command -v codex)"
  real_git="$(command -v git)"
  prepare_agent_guard "$guard_root" "$marker_endpoint" "$real_git"
  export OKF_SNAPSHOT_GIT="$real_git"
  (
    while IFS= read -r line <&"$marker_fd"; do
      [[ "$line" == "$marker_end" ]] && exit 0
      printf '%s\n' "$line" >> "$marker_log"
    done
  ) &
  marker_pid=$!
  snapshot_repo_state > "$state_before"
  export PYTHONDONTWRITEBYTECODE=1
  agent_rc=0
  printf '%s\n' "$PROMPT" | run_codex_read_only "$codex_bin" "$guard_root" "$repo_root" || agent_rc=$?
  printf '%s\n' "$marker_end" >&"$marker_fd"
  wait "$marker_pid" || true
  snapshot_repo_state > "$state_after"
  if ! diff -u "$state_before" "$state_after" >/dev/null; then
    echo "ERROR: Codex mutated HEAD, refs, remotes, or worktrees — aborting (fail-closed)."
    diff -u "$state_before" "$state_after" || true
    return 1
  fi
  if [[ -s "$marker_log" ]]; then
    echo "ERROR: Codex attempted a forbidden git/PR operation — aborting (fail-closed)."
    cat "$marker_log"
    return 1
  fi
  # The Codex isolation ends here.  Proprietary index/validation/PR gates must
  # use the operator's original PATH and environment, never the agent guard.
  restore_proprietary_environment() {
    PATH="$saved_path"; export PATH
    if [[ -n "$saved_snapshot_git" ]]; then export OKF_SNAPSHOT_GIT="$saved_snapshot_git"; else unset OKF_SNAPSHOT_GIT; fi
    unset OKF_GUARD_MARKER OKF_REAL_GIT
  }
  restore_proprietary_environment
  if [[ "$(command -v git)" != "$real_git" ]]; then
    echo "ERROR: proprietary gates still resolve the Codex git guard" >&2
    return 1
  fi
  if (( agent_rc != 0 )); then
    echo "ERROR: Codex exited $agent_rc — aborting (fail-closed)."
    return "$agent_rc"
  fi
  python3 scripts/okf_index.py
  # Safety net: refuse tracked OR untracked changes outside the doc trees.
  # `git diff --name-only` alone misses newly created files.
  local outside=()
  all_changes="$state_root/all-changes"
  if ! { git diff --name-only -z HEAD && git ls-files --others --exclude-standard -z; } > "$all_changes"; then
    echo "ERROR: unable to inspect agent file changes — aborting (fail-closed)." >&2
    return 1
  fi
  while IFS= read -r -d '' path; do
    case "$path" in
      docs/knowledge/*|docs/internal/okf-wiki/*) ;;
      *) outside+=("$path") ;;
    esac
  done < "$all_changes"
  if (( ${#outside[@]} )); then
    echo "ERROR: agent edited files outside OKF docs — aborting."
    printf '%s\n' "${outside[@]}"
    exit 1
  fi
  python3 scripts/validate_okf.py
  bash tests/okf/run_fixtures.sh
}

# ── --pr: isolated worktree, non-destructive, opens a PR ──────────────────────
if [[ "$MODE" == "--pr" ]]; then
  command -v gh >/dev/null || { echo "ERROR: 'gh' CLI needed for --pr."; exit 2; }
  git fetch origin -q
  TS="$(date +%Y%m%d-%H%M%S)"; WT=".claude/worktrees/okf-reconcile-$TS"; BR="auto/okf-reconcile-local-$TS"
  git worktree add -q "$WT" -b "$BR" origin/main
  pushd "$WT" >/dev/null
  trap 'popd >/dev/null 2>&1 || true; git worktree remove --force "$WT" 2>/dev/null || true' EXIT
  if [[ "$(drift_count)" == "0" ]]; then echo "drift cleared on origin/main — nothing to do."; exit 0; fi
  run_agent_and_validate
  if [[ -z "$(git --no-optional-locks status --porcelain=v1 --untracked-files=all)" ]]; then
    echo "agent made no changes — nothing to PR."; exit 0
  fi
  HEAD_SHA="$(git -C "$(git rev-parse --show-toplevel)" rev-parse origin/main)"
  git add -A
  git commit -q -m "docs(okf): auto-reconcile concept citations after $HEAD_SHA" \
    -m "Local self-healing run (okf-reconcile skill, your CLI auth): drifted cited lines re-anchored to current code, claims re-verified, checkpoints advanced. validate + fixtures green." \
    -m "$(printf 'Signed-off-by: %s <%s>' "$(git config user.name)" "$(git config user.email)")" \
    -m "Co-Authored-By: Codex <noreply@openai.com>"
  git push -q -u origin "$BR"
  gh pr create --base main --head "$BR" \
    --title "docs(okf): local auto-reconcile $TS" \
    --body "Self-healing OKF reconcile run locally with the Codex CLI (your ChatGPT auth — no API key). Pre-validated green (validate_okf + fixtures). Review the diff for claim-correctness and merge.

Generated with Codex."
  echo ">> PR opened. (isolated worktree auto-removed)"
  exit 0
fi

# ── default: reconcile in place, leave staged for your review ─────────────────
run_agent_and_validate
echo ""
echo ">> OKF reconciled in place + validated GREEN. Review and commit:"
echo "     git diff            # inspect the reconciled concepts"
echo "     git add -A && git commit -m 'docs(okf): reconcile concept citations'"
