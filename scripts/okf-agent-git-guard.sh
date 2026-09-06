#!/usr/bin/env bash
# Git/PR command guard used by both the local reconciler and the CI action.
set -uo pipefail

subcommand=""
args=("$@")

deny() {
  if [[ -n "${OKF_GUARD_MARKER-}" ]]; then
    printf '%s\n' "$*" >> "$OKF_GUARD_MARKER" 2>/dev/null || true
  fi
  echo "OKF guard: denied mutating git/PR command: $*" >&2
  exit 97
}

index=0
while (( index < ${#args[@]} )); do
  arg="${args[index]}"
  case "$arg" in
    --no-optional-locks)
      [[ "$index" -eq 0 ]] || deny "git $*"
      index=$((index + 1))
      continue
      ;;
    -*)
      # No global option is needed by the agent.  In particular, do not let
      # -C/--git-dir/--work-tree/-c redirect the target repository or inject
      # config before the exact read-only command checks below.
      deny "git $*"
      ;;
    *)
      subcommand="$arg"
      break
      ;;
  esac
done

readonly_form() {
  local arg
  for arg in "$@"; do
    case "$arg" in
      --output|--output=*|--exec-path|--exec-path=*|--config-env|--config-env=*|\
      --cached|--staged|--index|--intent-to-add|--no-index|--work-tree|--work-tree=*|--git-dir|--git-dir=*|\
      -C|-c|--namespace|--namespace=*)
        return 1 ;;
    esac
  done
  return 0
}

case "$subcommand" in
  commit|fetch|push|pull|clone|init|update-ref|tag|checkout|switch|\
  reset|restore|merge|rebase|cherry-pick|revert|am|apply|clean|gc|prune|reflog|\
  notes|replace|filter-branch|filter-repo|stash|bundle|archive|submodule)
    deny "git $*" ;;
  config)
    # Exact read-only forms only; option substrings must never authorize writes.
    case "${#args[@]}:${args[1]-}" in
      2:--list) ;;
      3:--get|3:--get-regexp) [[ "${args[2]}" != -* ]] || deny "git $*" ;;
      *) deny "git $*" ;;
    esac
    ;;
  remote)
    # `remote -v add` was the old substring-allowlist escape hatch.  Only the
    # two exact listing forms are accepted.
    [[ "${#args[@]}" -eq 2 && "${args[0]}" == "remote" && ( "${args[1]}" == "-v" || "${args[1]}" == "--verbose" ) ]] || deny "git $*"
    ;;
  branch)
    [[ "${#args[@]}" -eq 2 && "${args[0]}" == "branch" && "${args[1]}" == "--show-current" ]] ||
      [[ "${#args[@]}" -eq 2 && "${args[0]}" == "branch" && "${args[1]}" == "--list" ]] || deny "git $*"
    ;;
  worktree)
    [[ "${args[0]-}" == "worktree" && "${args[1]-}" == "list" ]] || deny "git $*"
    for arg in "${args[@]:2}"; do
      [[ "$arg" == "--porcelain" || "$arg" == "-z" || "$arg" == "--verbose" || "$arg" == "-v" ]] || deny "git $*"
    done
    ;;
  symbolic-ref)
    [[ "${#args[@]}" -eq 4 && "${args[1]-}" == "--quiet" && "${args[2]-}" == "--short" && "${args[3]-}" == "HEAD" ]] || deny "git $*"
    ;;
  hash-object)
    [[ "${#args[@]}" -ge 2 ]] || deny "git $*"
    for arg in "${args[@]:1}"; do
      [[ "$arg" != -* ]] || deny "git $*"
    done
    ;;
  status)
    # `git status` refreshes the index unless optional locks are disabled.
    # Keep one exact, side-effect-free form for the wrapper's post-agent check.
    [[ "${#args[@]}" -eq 4 && "${args[0]-}" == "--no-optional-locks" &&
       "${args[1]-}" == "status" && "${args[2]-}" == "--porcelain=v1" &&
       "${args[3]-}" == "--untracked-files=all" ]] || deny "git $*"
    ;;
  rev-parse)
    case "${#args[@]}:${args[1]-}" in
      2:HEAD|2:--show-toplevel|2:--git-common-dir|2:--is-inside-work-tree) ;;
      *) deny "git $*" ;;
    esac
    ;;
  for-each-ref)
    [[ "${#args[@]}" -eq 2 && "${args[1]}" == "--format=%(refname) %(objectname)" ]] || deny "git $*"
    ;;
  ls-files)
    [[ "${#args[@]}" -eq 4 && "${args[1]}" == "--others" && "${args[2]}" == "--exclude-standard" && "${args[3]}" == "-z" ]] || deny "git $*"
    ;;
  diff|log|show|cat-file|ls-tree|merge-base|diff-tree|describe|version)
    readonly_form "${args[@]}" || deny "git $*"
    ;;
  *)
    deny "git $*" ;;
esac

exec "${OKF_REAL_GIT:?}" "${args[@]}"
