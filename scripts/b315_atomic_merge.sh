#!/usr/bin/env bash
# B-315 transaction: exact byte authorities, exclusive lease, signed merge
# commit, normal (non-force) main push, and post-push MERGED/tree proof.
set -euo pipefail
PR=${1:?PR number required}; EXPECTED_HEAD=${2:?captured head required}; DRY_RUN=${3:-0}
REMOTE_LEASE_REF=refs/heads/corelink-backlog-id-merge-lock
REMOTE_LEASE_OID=; REMOTE_LEASE_HELD=0; REMOTE_LEASE_CONFIRMED=0
LOCK_PID=; LOCK_READY=; TMP=
BACKLOG_TMP=
CAPTURED_HEAD=; CAPTURED_BASE=; CAPTURED_MAIN=

cleanup_lease() {
  # owner-safe release: only this exact unique lease object may be removed.
  if [ "$REMOTE_LEASE_HELD" -eq 1 ] && [ "$REMOTE_LEASE_CONFIRMED" -eq 1 ]; then
    if git push --force-with-lease="$REMOTE_LEASE_REF:$REMOTE_LEASE_OID" origin ":$REMOTE_LEASE_REF"; then
      REMOTE_LEASE_HELD=0; REMOTE_LEASE_CONFIRMED=0
    else
      echo "⛔ remote lease cleanup refused (owner/token changed); leaving ref in place." >&2
    fi
  fi
}
release_remote_backlog_lease() { cleanup_lease; }
stop_backlog_allocation_lock() {
  if [ -n "$LOCK_PID" ]; then kill "$LOCK_PID" 2>/dev/null || true; wait "$LOCK_PID" 2>/dev/null || true; LOCK_PID=; fi
  [ -z "$LOCK_READY" ] || rm -f -- "$LOCK_READY"; LOCK_READY=
}
cleanup() {
  local rc=$?; release_remote_backlog_lease || true
  stop_backlog_allocation_lock; [ -z "$BACKLOG_TMP" ] || rm -rf -- "$BACKLOG_TMP"
  return "$rc"
}
trap cleanup EXIT; trap 'exit 130' INT; trap 'exit 143' TERM

start_backlog_allocation_lock() {
common_dir="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
[ -n "$common_dir" ] && [ -d "$common_dir" ] && [ ! -L "$common_dir" ] || { echo "⛔ invalid git common-dir lock authority." >&2; exit 1; }
lock="$common_dir/corelink-backlog-id-allocation.lock"; LOCK_READY="$(mktemp "$common_dir/backlogalloc.ready.XXXXXX")"
python3 scripts/backlog_id_alloc.py --hold-lock "$lock" --common-dir "$common_dir" --ready "$LOCK_READY" </dev/null & LOCK_PID=$!
for _ in $(seq 1 40); do
  if [ -s "$LOCK_READY" ]; then grep -q '^locked ' "$LOCK_READY" && break; echo "⛔ local allocation lock busy/invalid." >&2; exit 1; fi
  kill -0 "$LOCK_PID" 2>/dev/null || { echo "⛔ local allocation lock failed." >&2; exit 1; }; sleep 0.05
done
grep -q '^locked ' "$LOCK_READY" || { echo "⛔ local allocation lock did not become ready." >&2; return 1; }
}
start_backlog_allocation_lock || exit 1
backlog_lock_healthy() { kill -0 "$LOCK_PID" 2>/dev/null && [ -s "$LOCK_READY" ] && grep -q '^locked ' "$LOCK_READY"; }

meta="$(gh pr view "$PR" --json headRefOid,baseRefOid,baseRefName -q '"\(.headRefOid) \(.baseRefOid) \(.baseRefName)"' 2>/dev/null || true)"; read -r bk_head bk_base bk_base_name <<<"$meta"
main_sha_before="$(git ls-remote origin refs/heads/main 2>/dev/null | awk 'NR==1 {print $1}')"
[ "$bk_head" = "$EXPECTED_HEAD" ] && [ -n "$bk_base" ] && [ "$bk_base_name" = main ] && [ -n "$main_sha_before" ] || { echo "⛔ exact base/head/main authority changed." >&2; exit 1; }
CAPTURED_HEAD="$bk_head"; CAPTURED_BASE="$bk_base"; CAPTURED_MAIN="$main_sha_before"
[ "$bk_head" = "$CAPTURED_HEAD" ] && [ "$CAPTURED_BASE" = "$CAPTURED_MAIN" ] || { echo "⛔ candidate base/head is stale." >&2; exit 1; }
git fetch --no-tags origin "$bk_base" "$bk_head" >/dev/null 2>&1 || { echo "⛔ immutable base/head objects unavailable locally." >&2; exit 1; }
TMP="$(mktemp -d /tmp/b315-atomic.XXXXXX)"; BACKLOG_TMP="$TMP"
fetch_backlog() { gh api -H 'Accept: application/vnd.github.raw' "repos/{owner}/{repo}/contents/BACKLOG.md?ref=$1" >"$2" 2>/dev/null; }
fetch_backlog "$bk_head" "$TMP/candidate.md" || { echo "⛔ candidate BACKLOG unavailable." >&2; exit 1; }
fetch_backlog "$bk_base" "$TMP/base.md" || { echo "⛔ base BACKLOG unavailable." >&2; exit 1; }
fetch_backlog "$main_sha_before" "$TMP/main-before.md" || { echo "⛔ main BACKLOG unavailable." >&2; exit 1; }
python3 scripts/backlog_id_alloc.py --main "$TMP/main-before.md" --candidate "$TMP/candidate.md" --base "$TMP/base.md" || { echo "⛔ allocation refused." >&2; exit 1; }
main_sha_after="$(git ls-remote origin refs/heads/main 2>/dev/null | awk 'NR==1 {print $1}')"; pr_head_after="$(gh pr view "$PR" --json headRefOid --jq .headRefOid 2>/dev/null || true)"
[ "$main_sha_after" = "$main_sha_before" ] && [ "$pr_head_after" = "$bk_head" ] || { echo "⛔ main/head moved during allocation." >&2; exit 1; }
fetch_backlog "$main_sha_after" "$TMP/main-after.md" || { echo "⛔ main revalidation unavailable." >&2; exit 1; }; cmp -s "$TMP/main-before.md" "$TMP/main-after.md" || { echo "⛔ main BACKLOG moved during allocation." >&2; exit 1; }
python3 scripts/backlog_id_alloc.py --main "$TMP/main-after.md" --candidate "$TMP/candidate.md" --base "$TMP/base.md" >/dev/null || { echo "⛔ allocation became stale." >&2; exit 1; }

head_tree="$(gh api "repos/{owner}/{repo}/git/commits/$bk_head" --jq .tree.sha 2>/dev/null || true)"; [ -n "$head_tree" ] || { echo "⛔ candidate tree unavailable." >&2; exit 1; }
main_sha_final="$(git ls-remote origin refs/heads/main 2>/dev/null | awk 'NR==1 {print $1}')"; pr_head_final="$(gh pr view "$PR" --json headRefOid --jq .headRefOid 2>/dev/null || true)"
[ "$main_sha_final" = "$main_sha_before" ] || { echo "⛔ main moved at push boundary; retry." >&2; exit 1; }; [ "$pr_head_final" = "$bk_head" ] || { echo "⛔ candidate force-pushed at push boundary; retry." >&2; exit 1; }
commit_msg="Merge PR #$PR: $(gh pr view "$PR" --json title --jq .title 2>/dev/null || echo candidate)"; commit_msg="$commit_msg\n\nSigned-off-by: ${GIT_AUTHOR_NAME:-$(git config user.name)} <${GIT_AUTHOR_EMAIL:-$(git config user.email)}>"
merge_oid="$(printf '%b\n' "$commit_msg" | GIT_AUTHOR_NAME="${GIT_AUTHOR_NAME:-$(git config user.name)}" GIT_AUTHOR_EMAIL="${GIT_AUTHOR_EMAIL:-$(git config user.email)}" GIT_COMMITTER_NAME="${GIT_COMMITTER_NAME:-$(git config user.name)}" GIT_COMMITTER_EMAIL="${GIT_COMMITTER_EMAIL:-$(git config user.email)}" git -c gpg.format=ssh -c user.signingkey="${GIT_SIGNING_KEY:-$(git config user.signingkey)}" commit-tree "$head_tree" -p "$bk_base" -p "$bk_head" -S)" || { echo "⛔ signed merge commit creation failed." >&2; exit 1; }
[ "$(git show -s --format=%T "$merge_oid")" = "$head_tree" ] || { echo "⛔ merge tree mismatch." >&2; exit 1; }; [ "$(git show -s --format=%P "$merge_oid")" = "$bk_base $bk_head" ] || { echo "⛔ merge parent mismatch." >&2; exit 1; }; git show -s --format=%B "$merge_oid" | grep -q '^Signed-off-by:' || { echo "⛔ merge commit lacks DCO." >&2; exit 1; }; git cat-file -p "$merge_oid" | grep -q '^gpgsig ' || { echo "⛔ merge commit lacks a cryptographic signature." >&2; exit 1; }
echo "▶ atomic push $merge_oid -> refs/heads/main (expected old $main_sha_before)"; if [ "$DRY_RUN" -eq 1 ]; then echo "⏸ dry-run: not pushed."; exit 0; fi
observed="$(git ls-remote origin "$REMOTE_LEASE_REF" 2>/dev/null | awk 'NR==1 {print $1}')"; [ -z "$observed" ] || { echo "⛔ stale remote lease observed ($observed); refusing to break or take over automatically." >&2; echo "   recovery: git push --force-with-lease=$REMOTE_LEASE_REF:$observed origin :$REMOTE_LEASE_REF" >&2; echo "   refusing to break it automatically; obtain independent owner/admin recovery." >&2; exit 1; }
REMOTE_LEASE_TOKEN="$(python3 -c 'import secrets; print(secrets.token_hex(24))')"; empty_tree="$(git mktree </dev/null)"; payload="B-315 merge lease token=$REMOTE_LEASE_TOKEN main=$main_sha_before"; REMOTE_LEASE_OID="$(printf '%s\n\nSigned-off-by: %s <%s>\n' "$payload" "$(git config user.name)" "$(git config user.email)" | git commit-tree "$empty_tree")"
git push origin "$REMOTE_LEASE_OID:$REMOTE_LEASE_REF" || { observed="$(git ls-remote origin "$REMOTE_LEASE_REF" 2>/dev/null | awk 'NR==1 {print $1}')"; echo "⛔ lease acquisition lost race; observed=${observed:-unavailable}; no takeover." >&2; echo "   recovery: git push --force-with-lease=$REMOTE_LEASE_REF:${observed:-unknown} origin :$REMOTE_LEASE_REF" >&2; exit 1; }; REMOTE_LEASE_HELD=1; observed="$(git ls-remote origin "$REMOTE_LEASE_REF" 2>/dev/null | awk 'NR==1 {print $1}')"; [ "$observed" = "$REMOTE_LEASE_OID" ] || { echo "⛔ lease replacement detected; refusing push." >&2; echo "   recovery packet: ref=$REMOTE_LEASE_REF observed_oid=${observed:-unavailable} expected_oid=$REMOTE_LEASE_OID; owner-safe conditional release only." >&2; exit 1; }; REMOTE_LEASE_CONFIRMED=1
# SIGKILL cannot run cleanup; the fcntl lock is released by the kernel and any
# surviving remote lease is intentionally handled as stale on the next run.
backlog_lock_healthy || { echo "⛔ local lock lost." >&2; exit 1; }; git push origin "$merge_oid:refs/heads/main" || { echo "⛔ atomic main push rejected; no merged claim." >&2; exit 1; }
post_state=UNKNOWN; main_oid=
for _ in 1 2 3 4 5 6; do post_state="$(gh pr view "$PR" --json state --jq .state 2>/dev/null || echo UNKNOWN)"; main_oid="$(git ls-remote origin refs/heads/main 2>/dev/null | awk 'NR==1 {print $1}')"; [ "$post_state" = MERGED ] && [ "$main_oid" = "$merge_oid" ] && break; sleep 2; done
[ "$post_state" = MERGED ] || { echo "⛔ PR #$PR is not MERGED; no false merged claim." >&2; exit 1; }; [ "$main_oid" = "$merge_oid" ] || { echo "⛔ resulting main OID mismatch." >&2; exit 1; }; actual_tree="$(gh api "repos/{owner}/{repo}/git/commits/$main_oid" --jq .tree.sha 2>/dev/null || true)"; [ "$actual_tree" = "$head_tree" ] || { echo "⛔ resulting main tree mismatch." >&2; exit 1; }
echo "✅ PR #$PR MERGED atomically: main=$main_oid tree=$actual_tree parents=$bk_base,$bk_head"
