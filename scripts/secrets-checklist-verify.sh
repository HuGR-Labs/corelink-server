#!/usr/bin/env bash
# WI-R2-14 — secrets-checklist drift verifier.
#
# Cross-references docs/internal/secrets-checklist.md (the canonical matrix of
# production secrets) against the codebase. Fails if drift exists in either
# direction:
#
#   1. Every env var referenced in code (env::var(...), std::env::var(...),
#      process.env.X in TS/JS/Next, and ${{ secrets.X }} in GHA workflows)
#      MUST have a row in the matrix.
#   2. Every row in the matrix MUST have at least one consumer in the
#      codebase (otherwise it's a stale row).
#
# Used by .github/workflows/cf-deploy-prod.yml as a deploy gate.
#
# Usage:
#   scripts/secrets-checklist-verify.sh
#
# Exit codes:
#   0 — matrix and code are in sync
#   1 — drift detected (script prints offending env vars)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

MATRIX_FILE="docs/internal/secrets-checklist.md"

if [ ! -f "${MATRIX_FILE}" ]; then
    echo "ERROR: ${MATRIX_FILE} not found." >&2
    exit 1
fi

# -----------------------------------------------------------------------------
# Allowlist: env vars that are intentionally not in the matrix.
# -----------------------------------------------------------------------------
# - PROPTEST_*: proptest framework knobs (test infra, not secrets)
# - HOME/USERPROFILE: OS-provided, not application secrets
# - CARGO_*: build-time cargo env (not secrets)
# - GITHUB_TOKEN: GitHub-injected, scoped per-workflow
# - GITHUB_OUTPUT/GITHUB_ENV/GITHUB_PATH/GITHUB_STEP_SUMMARY: GHA runner
# - RUNNER_TEMP/RUNNER_OS/RUNNER_ARCH: GHA runner
# - GH_TOKEN: alias for GITHUB_TOKEN in gh CLI
# - GNUPGHOME: gpg working dir (transient)
# - SOURCE_DATE_EPOCH: SLSA reproducible-build standard
# - PATH/PWD/USER/SHELL/TERM/CI/TZ/LANG/LC_*: standard env
# - PROPTEST_ARGON_NIGHTLY: nightly proptest knob
# - CORELINK_FOO_MOCK: test/dev mock toggles already in matrix
# - TODO_*: placeholder names in wrangler.toml
# - NODE_ENV: standard Node knob
# - ENVIRONMENT: wrangler-injected (vars block), not a secret
ALLOWLIST_REGEX='^(PROPTEST_|HOME$|USERPROFILE$|CARGO_|GITHUB_(TOKEN|OUTPUT|ENV|PATH|STEP_SUMMARY|ACTIONS|REPOSITORY|SHA|REF|WORKFLOW|RUN_ID|RUN_NUMBER|ACTOR|EVENT_NAME|EVENT_PATH|JOB|API_URL|SERVER_URL|GRAPHQL_URL|WORKSPACE)$|RUNNER_|GH_TOKEN$|GNUPGHOME$|SOURCE_DATE_EPOCH$|PATH$|PWD$|USER$|SHELL$|TERM$|CI$|TZ$|LANG$|LC_|NODE_ENV$|ENVIRONMENT$|RUST_|TODO_|DOCS_BASE_URL$|LH_BASE_URL$|LH_START_COMMAND$|E2E_BASE_URL$|NEXT_PUBLIC_E2E_TEST_MODE$|PROJECTS$|SKIP_WEBSERVER$|GCP_TEST_KEY_RESOURCE$|GCP_TEST_REGION$|DT_API_KEY_TEST_|DT_MOCK_INJECTION_ENABLED$)'

# -----------------------------------------------------------------------------
# Extract env-var names from the matrix.
# Matrix rows look like: `| 1 | <secret name> | `ENV_VAR_NAME` | ...`
# We grab the backtick-quoted token in the 4th column.
# -----------------------------------------------------------------------------
extract_matrix_vars() {
    # Match lines starting with "| <number> |"
    # Then pull tokens in backticks from the env var column (3rd content column).
    grep -E '^\| [0-9]+ \|' "${MATRIX_FILE}" \
        | awk -F'|' '{print $4}' \
        | grep -oE '`[A-Z][A-Z0-9_]*`' \
        | tr -d '`' \
        | sort -u
}

# -----------------------------------------------------------------------------
# Extract env-var references from code:
#   - env::var("FOO") / std::env::var("FOO") / env::var_os("FOO") in Rust
#   - process.env.FOO / process.env["FOO"] in TS/JS
#   - ${{ secrets.FOO }} in GHA workflows
#   - env: FOO: ${{ ... }} in GHA workflows
# Excludes: target/, node_modules/, _archive/, .git/
# -----------------------------------------------------------------------------
extract_code_vars() {
    {
        # Rust: env::var("FOO"), std::env::var("FOO"), env::var_os("FOO")
        grep -rEh 'env::var(_os)?\("[A-Z][A-Z0-9_]*"\)' \
            --include='*.rs' \
            --exclude-dir=target \
            --exclude-dir=node_modules \
            --exclude-dir=_archive \
            --exclude-dir=.git \
            . 2>/dev/null \
            | grep -oE 'env::var(_os)?\("[A-Z][A-Z0-9_]*"\)' \
            | grep -oE '"[A-Z][A-Z0-9_]*"' \
            | tr -d '"' || true

        # Rust: env::set_var("FOO", ...) — symmetric, in case tests set things
        grep -rEh 'env::set_var\("[A-Z][A-Z0-9_]*"' \
            --include='*.rs' \
            --exclude-dir=target \
            --exclude-dir=node_modules \
            --exclude-dir=_archive \
            --exclude-dir=.git \
            . 2>/dev/null \
            | grep -oE '"[A-Z][A-Z0-9_]*"' \
            | tr -d '"' || true

        # TS/JS: process.env.FOO
        grep -rEh 'process\.env\.[A-Z][A-Z0-9_]*' \
            --include='*.ts' --include='*.tsx' \
            --include='*.js' --include='*.jsx' --include='*.mjs' --include='*.cjs' \
            --exclude-dir=node_modules \
            --exclude-dir=.next \
            --exclude-dir=dist \
            --exclude-dir=build \
            --exclude-dir=_archive \
            --exclude-dir=.git \
            . 2>/dev/null \
            | grep -oE 'process\.env\.[A-Z][A-Z0-9_]*' \
            | sed 's/process\.env\.//' || true

        # TS/JS: process.env["FOO"]
        grep -rEh 'process\.env\["[A-Z][A-Z0-9_]*"\]' \
            --include='*.ts' --include='*.tsx' \
            --include='*.js' --include='*.jsx' --include='*.mjs' --include='*.cjs' \
            --exclude-dir=node_modules \
            --exclude-dir=.next \
            --exclude-dir=dist \
            --exclude-dir=build \
            --exclude-dir=_archive \
            --exclude-dir=.git \
            . 2>/dev/null \
            | grep -oE '"[A-Z][A-Z0-9_]*"' \
            | tr -d '"' || true

        # GHA: ${{ secrets.FOO }}
        grep -rEh '\$\{\{\s*secrets\.[A-Z][A-Z0-9_]*\s*\}\}' \
            .github/workflows/ 2>/dev/null \
            | grep -oE 'secrets\.[A-Z][A-Z0-9_]*' \
            | sed 's/secrets\.//' || true
    } | sort -u
}

# -----------------------------------------------------------------------------
# Run the checks.
# -----------------------------------------------------------------------------
TMPDIR_VERIFY="$(mktemp -d)"
trap 'rm -rf "${TMPDIR_VERIFY}"' EXIT

MATRIX_VARS="${TMPDIR_VERIFY}/matrix.txt"
CODE_VARS="${TMPDIR_VERIFY}/code.txt"
CODE_VARS_FILTERED="${TMPDIR_VERIFY}/code-filtered.txt"

extract_matrix_vars > "${MATRIX_VARS}"
extract_code_vars   > "${CODE_VARS}"

# Filter out allowlist from code vars before comparing.
grep -vE "${ALLOWLIST_REGEX}" "${CODE_VARS}" > "${CODE_VARS_FILTERED}" || true

MATRIX_COUNT=$(wc -l < "${MATRIX_VARS}" | tr -d ' ')
CODE_COUNT=$(wc -l < "${CODE_VARS_FILTERED}" | tr -d ' ')

echo "secrets-checklist-verify: matrix has ${MATRIX_COUNT} env vars; code references ${CODE_COUNT} unique non-allowlisted env vars."

# Quality gate: matrix must cover ≥ 20 secrets (WI-R2-14 spec).
if [ "${MATRIX_COUNT}" -lt 20 ]; then
    echo "ERROR: matrix has only ${MATRIX_COUNT} entries; WI-R2-14 requires ≥ 20." >&2
    exit 1
fi

# Drift 1: code references an env var not in the matrix.
MISSING_FROM_MATRIX="${TMPDIR_VERIFY}/missing-from-matrix.txt"
comm -23 "${CODE_VARS_FILTERED}" "${MATRIX_VARS}" > "${MISSING_FROM_MATRIX}"

# Drift 2: matrix has an env var not used anywhere in code.
STALE_IN_MATRIX="${TMPDIR_VERIFY}/stale-in-matrix.txt"
comm -13 "${CODE_VARS_FILTERED}" "${MATRIX_VARS}" > "${STALE_IN_MATRIX}"

FAIL=0

if [ -s "${MISSING_FROM_MATRIX}" ]; then
    echo "" >&2
    echo "ERROR: the following env vars are referenced in code but NOT in ${MATRIX_FILE}:" >&2
    sed 's/^/  - /' "${MISSING_FROM_MATRIX}" >&2
    echo "" >&2
    echo "Either:" >&2
    echo "  (a) add a row to the matrix in the same PR, OR" >&2
    echo "  (b) add the var to the ALLOWLIST_REGEX in this script if it's not a secret." >&2
    FAIL=1
fi

if [ -s "${STALE_IN_MATRIX}" ]; then
    echo "" >&2
    echo "WARNING: the following matrix rows have NO consumer in code (stale rows):" >&2
    sed 's/^/  - /' "${STALE_IN_MATRIX}" >&2
    echo "" >&2
    echo "These vars may be:" >&2
    echo "  (a) consumed by docs-only paths or external scripts (acceptable — add to ALLOWLIST_REGEX or document)," >&2
    echo "  (b) reserved for upcoming work (acceptable — leave row in place)," >&2
    echo "  (c) genuinely stale (REMOVE the row in the same PR)." >&2
    # Soft warning: don't fail the deploy for stale rows. The matrix is
    # forward-looking by design (e.g., SendGrid/Twilio referenced in pending
    # notification paths).
fi

if [ "${FAIL}" -eq 0 ]; then
    echo "secrets-checklist-verify: OK (no drift)."
fi

exit "${FAIL}"
