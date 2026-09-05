#!/usr/bin/env bash
# Prove `ci-use-host-toolchain.sh` refuses a channel that is a path-traversal
# primitive, and still accepts every legal spelling of a real one.
#
# WHY THIS FILE EXISTS (B-133). The script interpolates the channel it read from
# `rust-toolchain.toml` into a filesystem path, and appends that path to
# $GITHUB_PATH:
#
#     TC="$HOME/.rustup/toolchains/${CHANNEL}-${HOST_TRIPLE}"
#     echo "$TC/bin" >> "$GITHUB_PATH"
#
# So whoever controls that toml controls the front of PATH for every later step
# in the job. `dependabot-policy.yml` ran this script against a
# `pull_request_target` checkout of PR content, which made "whoever" the PR
# author. Measured before the fix, with a channel of `../../../<checkout>/evil`
# and an `evil-<triple>/bin/{cargo,rustc}` inside the checked-out tree (git
# preserves the executable bit): the script exited 0, printed
# `PATH -> …/evil-<triple>/bin`, and ran the attacker's binary as `rustc`.
#
# 25 workflows call this script, so the guard lives in the script rather than in
# any one lane's YAML.
#
# Run: bash scripts/test_ci_use_host_toolchain.sh
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$HERE/ci-use-host-toolchain.sh"
SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT

fails=0
pass() { echo "  PASS  [$1] $2"; }
fail() { echo "  FAIL  [$1] $2"; fails=$((fails + 1)); }

# Runs the script in a throwaway git repo with the supplied complete TOML text.
# Returns the exit code; leaves the captured output in $out and whatever the
# script appended in $ghpath_content.
run_with_toml() {
  local toml="$1" repo="$SANDBOX/r"
  rm -rf "$repo"; mkdir -p "$repo"
  ( cd "$repo" && git init -q . && git config user.email t@t && git config user.name t )
  printf '%s' "$toml" > "$repo/rust-toolchain.toml"
  ( cd "$repo" && git add -A && git commit -qm fixture )
  : > "$SANDBOX/ghpath"
  # Do not accidentally execute a real host rustc while testing parser
  # acceptance.  An empty HOME makes the later "not installed" branch quick;
  # these cells assert only whether the channel was accepted/refused before
  # PATH could be changed.
  out="$( cd "$repo" && HOME="$SANDBOX/empty-home" GITHUB_PATH="$SANDBOX/ghpath" bash "$SCRIPT" 2>&1 )"
  local rc=$?
  ghpath_content="$(cat "$SANDBOX/ghpath")"
  return $rc
}

run_with_channel() {
  local channel="$1"
  run_with_toml "$(printf '[toolchain]\nchannel = "%s"\n' "$channel")"
}

# A channel that is a traversal must be REFUSED, and — the assertion that
# actually matters — must leave $GITHUB_PATH untouched. Exiting non-zero while
# still having appended the path would be a fix that fixes nothing.
refused() { # $1 stable cell ID, $2 channel, $3 expected message fragment
  local id="$1" channel="$2" want="$3"
  run_with_channel "$channel"; local rc=$?
  if [[ $rc -eq 0 ]]; then
    fail "$id" "refuses '$channel' (exit 0 — the traversal was accepted)"; return
  fi
  if ! grep -Fq "$want" <<<"$out"; then
    fail "$id" "refuses '$channel' (no '$want' in output — may be dying for an unrelated reason)"
    while IFS= read -r line; do printf '        %s\n' "$line"; done <<<"$out"; return
  fi
  if [[ -n "$ghpath_content" ]]; then
    fail "$id" "refuses '$channel' BUT still appended to \$GITHUB_PATH: $ghpath_content"; return
  fi
  pass "$id" "refuses '$channel'"
}

echo "ci-use-host-toolchain channel guard"

# The exploit primitive itself, and the spellings around it.
refused B133-CHANNEL-REJECT-01 "../../../../tmp/evil" "path-traversal primitive"
refused B133-CHANNEL-REJECT-02 "a/b" "path-traversal primitive"
refused B133-CHANNEL-REJECT-03 "\$(id)" "path-traversal primitive"
refused B133-CHANNEL-REJECT-04 "a;b" "path-traversal primitive"
refused B133-CHANNEL-REJECT-05 "a b" "path-traversal primitive"
# `..` is made only of legal characters — dots are legal in `1.91.1` — so the
# character class alone lets it through. It needs its own refusal, and this cell
# is what proves the second `case` is load-bearing rather than decoration.
refused B133-CHANNEL-REJECT-06 ".." "walks out of"
refused B133-CHANNEL-REJECT-07 "x..y" "walks out of"

# ── The negative controls ────────────────────────────────────────────────────
# Without these, `exit 2` on the first line of the script passes every cell
# above. A guard that refuses every channel breaks all 25 callers.
#
# These assert only that the channel is ACCEPTED — the script legitimately exits
# 1 afterwards when that toolchain is not installed on this host, which is a
# different and correct failure. So the assertion is on the message, not the
# exit code: the refusal must not be the reason it stopped.
accepted() { # $1 stable cell ID, $2 channel
  local id="$1" channel="$2"
  run_with_channel "$channel"
  if grep -qE "refusing toolchain channel" <<<"$out"; then
    fail "$id" "accepts '$channel' (it was refused — the guard is too strict and would break real callers)"
    while IFS= read -r line; do printf '        %s\n' "$line"; done <<<"$out"; return
  fi
  pass "$id" "accepts '$channel'"
}

accepted B133-CHANNEL-ACCEPT-01 "1.91.1"                      # the channel this workspace actually pins
accepted B133-CHANNEL-ACCEPT-02 "stable"
accepted B133-CHANNEL-ACCEPT-03 "beta"
accepted B133-CHANNEL-ACCEPT-04 "nightly"
accepted B133-CHANNEL-ACCEPT-05 "nightly-2026-01-01"          # dated nightly
accepted B133-CHANNEL-ACCEPT-06 "1.91.1-x86_64-apple-darwin"  # host-suffixed

# TOML placement is security-relevant.  A line parser could select an
# attacker-controlled `channel` outside `[toolchain]`, or reject a legal
# trailing-comment/whitespace pin and leave the policy job unusable.
accepted_toml() { # $1 stable cell ID, $2 full TOML document
  local id="$1" toml="$2"
  run_with_toml "$toml"
  if grep -qE 'could not parse \[toolchain\] channel|invalid rust-toolchain.toml|refusing toolchain channel' <<<"$out"; then
    fail "$id" "accepts TOML fixture (parser rejected a legal [toolchain].channel)"
    while IFS= read -r line; do printf '        %s\n' "$line"; done <<<"$out"; return
  fi
  pass "$id" "accepts TOML fixture"
}

refused_toml() { # $1 stable cell ID, $2 full TOML document, $3 message fragment
  local id="$1" toml="$2" want="$3"
  run_with_toml "$toml"; local rc=$?
  if [[ $rc -eq 0 ]]; then
    fail "$id" "refuses TOML fixture (exit 0 — attacker channel accepted)"; return
  fi
  if ! grep -Fq "$want" <<<"$out"; then
    fail "$id" "refuses TOML fixture (no '$want' in output)"
    while IFS= read -r line; do printf '        %s\n' "$line"; done <<<"$out"; return
  fi
  if [[ -n "$ghpath_content" ]]; then
    fail "$id" "refuses TOML fixture BUT still appended to \$GITHUB_PATH: $ghpath_content"; return
  fi
  pass "$id" "refuses TOML fixture"
}

accepted_toml B133-TOML-ACCEPT-01 $'[toolchain]\n  channel = "1.91.1" # workspace pin\nprofile = "minimal"\n'
accepted_toml B133-TOML-ACCEPT-02 $'[decoy]\nchannel = "../../attacker"\n[toolchain]\nchannel = "stable"\n'
refused_toml B133-TOML-REJECT-01 $'[toolchain]\nchannel = "../../attacker"\n[decoy]\nchannel = "stable"\n' "path-traversal primitive"
refused_toml B133-TOML-REJECT-02 $'channel = "stable"\n[toolchain]\nprofile = "minimal"\n' "could not parse [toolchain] channel"

echo
if [[ "$fails" -eq 0 ]]; then echo "ci-use-host-toolchain channel guard: all cells passed"; exit 0; fi
echo "ci-use-host-toolchain channel guard: $fails cell(s) failed"; exit 1
