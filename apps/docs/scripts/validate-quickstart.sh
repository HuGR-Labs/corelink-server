#!/usr/bin/env bash
# validate-quickstart.sh — verify every `corelink <subcmd>` example in the
# 10-minute quickstart MDX corresponds to a REAL subcommand in
# `crates/corelink-cli/src/main.rs`.
#
# Designed to run WITHOUT a sandbox PAT — it never executes the commands,
# it only parses MDX fenced shell blocks and grep-checks the CLI source.
#
# Exit codes:
#   0  — all referenced subcommands exist in the CLI surface.
#   1  — at least one referenced subcommand was not found (drift!).
#   2  — required inputs missing (MDX file or CLI source absent).
#
# Used by .github/workflows/quickstart-validate.yml as a CI warning
# (continue-on-error so we don't block PRs — drift gets surfaced in the
# annotations panel).

set -euo pipefail

# Resolve repo root regardless of where script is invoked from.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"

MDX="${REPO_ROOT}/apps/docs/docs/tutorials/quickstart-10min.mdx"
CLI_SRC="${REPO_ROOT}/crates/corelink-cli/src/main.rs"

if [[ ! -f "${MDX}" ]]; then
  echo "error: quickstart MDX not found at ${MDX}" >&2
  exit 2
fi
if [[ ! -f "${CLI_SRC}" ]]; then
  echo "error: corelink-cli main.rs not found at ${CLI_SRC}" >&2
  exit 2
fi

echo "validate-quickstart: parsing ${MDX#${REPO_ROOT}/}"
echo "validate-quickstart: cross-checking against ${CLI_SRC#${REPO_ROOT}/}"

# Whitelist of subcommands derived from `crates/corelink-cli/src/main.rs`
# (the `enum Commands` block). Updated when CLI surface changes.
KNOWN_SUBCMDS=(
  "ls" "get" "put" "stat" "bench" "doctor" "version" "config"
  "runbook-drill"
)

# Also accept these well-known CLI flags / aliases that appear in examples.
# (Not subcommands but valid second-tokens after `corelink`.)
KNOWN_TOKENS=(
  "--version" "--help" "-h" "-V"
)

is_known() {
  local cand="$1"
  for k in "${KNOWN_SUBCMDS[@]}"; do
    [[ "${cand}" == "${k}" ]] && return 0
  done
  for k in "${KNOWN_TOKENS[@]}"; do
    [[ "${cand}" == "${k}" ]] && return 0
  done
  return 1
}

# Sanity: every entry in KNOWN_SUBCMDS must appear in the enum source
# (catches stale whitelist if CLI surface is renamed).
for sub in "${KNOWN_SUBCMDS[@]}"; do
  # `subcommand_label` mapping in main.rs uses kebab-case for the public
  # name (e.g., "runbook-drill") — grep for the literal string.
  if ! grep -qE "\"${sub}\"" "${CLI_SRC}"; then
    echo "::warning::known subcommand '${sub}' not found in ${CLI_SRC#${REPO_ROOT}/} — whitelist may be stale" >&2
  fi
done

# Extract every line inside ```bash / ```shell fenced blocks that starts
# with `corelink ` (i.e., a CLI invocation). Strip leading shell
# substitutions / env-prefixes (`DIGEST=$(corelink put …)` etc.).
INVOCATIONS=$(
  awk '
    /^```(bash|sh|shell|console)/ { in_code=1; next }
    /^```/                         { in_code=0; next }
    in_code                        { print }
  ' "${MDX}" \
  | grep -vE '^\s*#' \
  | grep -oE '(^|[[:space:];|`$()])corelink[[:space:]]+[A-Za-z_-]+' \
  | sed -E 's/^[[:space:];|`$()]+//' \
  | sort -u
)

if [[ -z "${INVOCATIONS}" ]]; then
  echo "::warning::no \`corelink <subcmd>\` invocations found in MDX — is the file empty?" >&2
  exit 0
fi

FAIL=0
COUNT=0
while IFS= read -r line; do
  COUNT=$((COUNT + 1))
  # Second token after `corelink`.
  sub=$(printf "%s" "${line}" | awk '{print $2}')
  if is_known "${sub}"; then
    printf "  OK   corelink %s\n" "${sub}"
  else
    printf "  FAIL corelink %s — not in CLI surface\n" "${sub}"
    FAIL=$((FAIL + 1))
  fi
done <<< "${INVOCATIONS}"

echo "validate-quickstart: ${COUNT} unique invocations, ${FAIL} unknown."
if [[ "${FAIL}" -gt 0 ]]; then
  echo "::error::quickstart references ${FAIL} subcommand(s) not present in corelink-cli — fix the MDX or update the CLI." >&2
  exit 1
fi
echo "validate-quickstart: all referenced subcommands are real. PASS."
exit 0
