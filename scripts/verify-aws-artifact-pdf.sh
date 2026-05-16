#!/usr/bin/env bash
#
# DEBT-003 — AWS Artifact PDF SHA-256 verifier.
#
# Canonical placeholder doc: specs/_compliance/aws-artifact-placeholder.md.
# Ledger:                    specs/_compliance/aws-artifact-pdfs/SHA256SUMS.
#
# Modes:
#
#   --check-empty            (CI / quality-gate) Confirms SHA256SUMS exists
#                            and contains zero non-comment, non-blank lines.
#                            Used at wave-27 land time when no PDF has been
#                            received yet; gates on the placeholder remaining
#                            empty so the operator's first append is visible.
#
#   --verify-ledger          Parse-only well-formed-ness check on every
#                            non-comment, non-blank line of SHA256SUMS.
#                            Each such line must match:
#                              <64-hex> <space>+ <filename> <space>* # fetched=YYYY-MM-DD principal=<arn> kind=<fips|soc2|other>
#                            Used post-receipt to keep the audit trail clean.
#
#   --pdf <path>             Operator-side verification on a freshly fetched
#                            PDF: computes the file's SHA-256 and confirms a
#                            matching row exists in SHA256SUMS. Prints `OK`
#                            with the matching ledger line on success.
#
#   --help                   Print this banner and exit 0.
#
# Exit codes:
#   0 — gate passes.
#   1 — gate fails (violation found; script prints details).
#   2 — invocation error (bad flags, missing files, no sha256 tool, etc.).
#
# Implementation notes:
#   * macOS (BSD) uses `shasum -a 256`; Linux uses `sha256sum`. This script
#     picks whichever is on PATH.
#   * Comment lines (`#…`) and blank lines are skipped in --check-empty
#     counting and in --verify-ledger parsing. They are preserved as ledger
#     header material.
#   * The script never mutates SHA256SUMS — appends are manual git commits
#     by the operator per §6 of the placeholder doc.

set -euo pipefail

# Repo-root detection — script may be invoked from anywhere.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

LEDGER="$REPO_ROOT/specs/_compliance/aws-artifact-pdfs/SHA256SUMS"
PLACEHOLDER_DOC="$REPO_ROOT/specs/_compliance/aws-artifact-placeholder.md"

# Pick the sha256 binary available on this platform.
sha256_tool() {
  if command -v sha256sum >/dev/null 2>&1; then
    echo "sha256sum"
  elif command -v shasum >/dev/null 2>&1; then
    echo "shasum -a 256"
  else
    echo "ERROR: neither sha256sum nor shasum on PATH — cannot compute digests" >&2
    exit 2
  fi
}

compute_sha256() {
  local pdf_path="$1"
  local tool
  tool="$(sha256_tool)"
  # Both tools emit `<hex>  <filename>` — we want only the hex.
  $tool "$pdf_path" | awk '{print $1}'
}

print_help() {
  sed -n '3,46p' "$0" | sed 's/^# \{0,1\}//'
}

# --- Mode dispatch ---

mode=""
pdf_path=""

if [[ $# -eq 0 ]]; then
  echo "ERROR: no mode specified. Use --help for usage." >&2
  exit 2
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check-empty)
      mode="check-empty"
      shift
      ;;
    --verify-ledger)
      mode="verify-ledger"
      shift
      ;;
    --pdf)
      mode="pdf"
      if [[ $# -lt 2 ]]; then
        echo "ERROR: --pdf requires a path argument." >&2
        exit 2
      fi
      pdf_path="$2"
      shift 2
      ;;
    --help|-h)
      print_help
      exit 0
      ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      echo "Use --help for usage." >&2
      exit 2
      ;;
  esac
done

if [[ -z "$mode" ]]; then
  echo "ERROR: no mode specified. Use --help for usage." >&2
  exit 2
fi

# All modes require the ledger file to exist.
if [[ ! -f "$LEDGER" ]]; then
  echo "ERROR: ledger file missing: $LEDGER" >&2
  echo "       Expected initial header-only file. Re-create per" >&2
  echo "       specs/_compliance/aws-artifact-placeholder.md §4.2." >&2
  exit 2
fi

# --- --check-empty -------------------------------------------------------

if [[ "$mode" == "check-empty" ]]; then
  # Count non-comment, non-blank lines.
  # `grep -v` exits 1 if every line is filtered out — tolerate via `|| true`,
  # since "all comments" is exactly the valid empty-state we want to accept.
  body_lines="$( { grep -v -E '^[[:space:]]*(#|$)' "$LEDGER" || true; } | wc -l | tr -d ' ')"
  if [[ "$body_lines" -eq 0 ]]; then
    echo "OK: SHA256SUMS is in initial empty state (header-only)."
    echo "    Ledger: $LEDGER"
    echo "    Placeholder doc: $PLACEHOLDER_DOC"
    exit 0
  else
    echo "FAIL: SHA256SUMS contains $body_lines non-header line(s); expected 0 for --check-empty mode." >&2
    echo "      If a PDF has been received, use --verify-ledger instead." >&2
    exit 1
  fi
fi

# --- --verify-ledger -----------------------------------------------------

# A valid ledger line is:
#   <64-hex>  <filename> [<whitespace>+]# fetched=YYYY-MM-DD principal=<arn> kind=<fips|soc2|other>
#
# We use a permissive but explicit regex anchored to the structure.

LEDGER_LINE_RE='^[0-9a-f]{64}[[:space:]]+[^[:space:]#][^#]*#[[:space:]]*fetched=[0-9]{4}-[0-9]{2}-[0-9]{2}[[:space:]]+principal=[^[:space:]]+[[:space:]]+kind=(fips|soc2|other)[[:space:]]*$'

if [[ "$mode" == "verify-ledger" ]]; then
  fail_count=0
  line_no=0
  body_count=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    line_no=$((line_no + 1))
    # Skip blanks + comment lines.
    if [[ -z "${line// /}" ]] || [[ "$line" =~ ^[[:space:]]*# ]]; then
      continue
    fi
    body_count=$((body_count + 1))
    if [[ ! "$line" =~ $LEDGER_LINE_RE ]]; then
      echo "FAIL: line $line_no malformed:" >&2
      echo "      $line" >&2
      echo "      Expected format:" >&2
      echo "        <64-hex>  <filename>  # fetched=YYYY-MM-DD principal=<arn> kind=<fips|soc2|other>" >&2
      fail_count=$((fail_count + 1))
    fi
  done < "$LEDGER"

  if [[ "$fail_count" -gt 0 ]]; then
    echo "FAIL: $fail_count malformed line(s) in $LEDGER" >&2
    exit 1
  fi
  echo "OK: ledger well-formed ($body_count receipt line(s) verified)."
  echo "    Ledger: $LEDGER"
  exit 0
fi

# --- --pdf <path> --------------------------------------------------------

if [[ "$mode" == "pdf" ]]; then
  if [[ ! -f "$pdf_path" ]]; then
    echo "ERROR: PDF not found: $pdf_path" >&2
    exit 2
  fi
  pdf_sha="$(compute_sha256 "$pdf_path")"
  echo "Computed SHA-256: $pdf_sha"
  echo "Looking up in:    $LEDGER"

  # Find a non-comment line starting with the matching hex digest.
  # Both greps may legitimately filter every line to nothing — tolerate via `|| true`.
  match="$( { grep -v -E '^[[:space:]]*#' "$LEDGER" || true; } | { grep -E "^${pdf_sha}[[:space:]]" || true; } )"
  if [[ -n "$match" ]]; then
    echo "OK: PDF matches a ledger row."
    echo "    Matching line: $match"
    exit 0
  fi
  echo "FAIL: PDF SHA-256 not found in ledger." >&2
  echo "      If this is a freshly fetched PDF, follow the §6 closure" >&2
  echo "      protocol in $PLACEHOLDER_DOC to record it before verifying." >&2
  exit 1
fi

# Unreachable.
echo "ERROR: internal mode dispatch failure ($mode)" >&2
exit 2
