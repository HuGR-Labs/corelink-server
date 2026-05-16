#!/usr/bin/env bash
#
# DEBT-003 — AWS Artifact PDF SHA-256 recorder (Owner one-command tool).
#
# Wave-28 step-4 collapses the post-browser-download tail of the DEBT-003
# closure protocol (§6 of specs/_compliance/aws-artifact-placeholder.md) —
# previously steps e/f/g/h (shasum → append ledger → flip BYOK matrix
# TBD-on-receipt → commit) — into a single Owner-side invocation.
#
# The Owner still performs the browser-bound prefix manually (auth into
# AWS Console, navigate Artifact, accept terms, download PDF). That part is
# fundamentally not scriptable without IAM-bound CLI credentials, which the
# Owner intentionally does not have provisioned for this report family
# (the AWS Artifact FIPS 140-3 KMS Validation Report requires a per-session
# click-through agreement acceptance).
#
# Usage:
#
#   bash scripts/admin/record-aws-artifact-pdf.sh \
#     <pdf-path> "<report-name>"
#
# Arguments:
#
#   <pdf-path>       Absolute or relative path to the freshly downloaded PDF.
#                    Typical: ~/Downloads/aws-artifact-fips-140-3.pdf
#
#   <report-name>    Human-readable report name, used in the ledger metadata
#                    comment block. Quote it. Examples:
#                      "FIPS 140-3 KMS Validation Report"
#                      "SOC 2 Type II — AWS KMS"
#
# Optional flags:
#
#   --kind <fips|soc2|other>   Override the kind= trailer (default: fips).
#   --principal <arn>          Override the principal= trailer
#                              (default: arn:aws:iam::UNKNOWN:user/$USER).
#   --no-matrix-update         Skip the BYOK matrix TBD-on-receipt flip step
#                              (use if recording a renewal that does not
#                              touch the matrix placeholder).
#   --help                     Print this banner and exit 0.
#
# What this script does (in order):
#
#   1. Validates the PDF path exists and is a regular file.
#   2. Computes SHA-256 of the PDF (shasum -a 256 on macOS,
#      sha256sum on Linux — same fallback strategy as verify-aws-artifact-pdf.sh).
#   3. Computes file size (in bytes, via wc -c).
#   4. Detects whether this SHA is already in the ledger. If yes, exits 0
#      idempotently with a note (no double-append, no double-commit).
#   5. Appends a 2-line metadata comment block + 1 ledger line to
#      specs/_compliance/aws-artifact-pdfs/SHA256SUMS, format:
#
#        # recorded=YYYY-MM-DD report="<report-name>" size=<bytes> by=<downloader>
#        <sha>  <filename>  # fetched=YYYY-MM-DD principal=<arn> kind=<kind>
#
#      The ledger line matches the regex enforced by
#      verify-aws-artifact-pdf.sh --verify-ledger exactly.
#
#   6. If the BYOK matrix still contains `sha256:TBD-on-receipt`, replaces
#      ONE occurrence (the first match) with `sha256:<hex>`. The replacement
#      target is discovered via grep — the script does not hard-code the
#      matrix path; it scans `specs/_compliance/*.md` and replaces in the
#      first file that has the token. If multiple files have it, the script
#      exits 2 with the list (Owner picks via --no-matrix-update + manual
#      edit). If no file has it, the script logs and proceeds (renewal case).
#
#   7. Runs `bash scripts/verify-aws-artifact-pdf.sh --verify-ledger` to
#      confirm the appended line is well-formed. Exits 1 if not (rollback
#      is manual; script prints the line it appended so Owner can `git
#      diff` and decide).
#
#   8. Prints a copy-pasteable `git add` + `git commit` command block.
#      Does NOT run git itself — the Owner reviews the diff before commit.
#
# Exit codes:
#   0 — append + (optional) matrix flip + verify all green.
#   1 — verify-ledger fails after append (likely script bug; rollback manual).
#   2 — invocation error or precondition violated (bad path, missing tool,
#       ambiguous matrix discovery, etc.).
#
# DCO: Co-Authored-By line below.
# Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

LEDGER="$REPO_ROOT/specs/_compliance/aws-artifact-pdfs/SHA256SUMS"
VERIFIER="$REPO_ROOT/scripts/verify-aws-artifact-pdf.sh"
COMPLIANCE_DIR="$REPO_ROOT/specs/_compliance"

print_help() {
  sed -n '3,73p' "$0" | sed 's/^# \{0,1\}//'
}

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
  $tool "$pdf_path" | awk '{print $1}'
}

# Defaults.
kind="fips"
principal="arn:aws:iam::UNKNOWN:user/${USER:-unknown}"
update_matrix=1
pdf_path=""
report_name=""

# --- Arg parse ------------------------------------------------------------

if [[ $# -eq 0 ]]; then
  echo "ERROR: missing arguments. Use --help for usage." >&2
  exit 2
fi

positional=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h)
      print_help
      exit 0
      ;;
    --kind)
      if [[ $# -lt 2 ]]; then echo "ERROR: --kind requires a value." >&2; exit 2; fi
      kind="$2"
      shift 2
      ;;
    --principal)
      if [[ $# -lt 2 ]]; then echo "ERROR: --principal requires a value." >&2; exit 2; fi
      principal="$2"
      shift 2
      ;;
    --no-matrix-update)
      update_matrix=0
      shift
      ;;
    --*)
      echo "ERROR: unknown flag: $1" >&2
      echo "Use --help for usage." >&2
      exit 2
      ;;
    *)
      positional+=("$1")
      shift
      ;;
  esac
done

if [[ ${#positional[@]} -lt 2 ]]; then
  echo "ERROR: required arguments missing: <pdf-path> <report-name>" >&2
  echo "Use --help for usage." >&2
  exit 2
fi

pdf_path="${positional[0]}"
report_name="${positional[1]}"

case "$kind" in
  fips|soc2|other) ;;
  *)
    echo "ERROR: --kind must be one of: fips, soc2, other (got: $kind)" >&2
    exit 2
    ;;
esac

# --- Preflight ------------------------------------------------------------

if [[ ! -f "$pdf_path" ]]; then
  echo "ERROR: PDF not found at: $pdf_path" >&2
  exit 2
fi
if [[ ! -f "$LEDGER" ]]; then
  echo "ERROR: ledger missing at: $LEDGER" >&2
  echo "       Re-create per specs/_compliance/aws-artifact-placeholder.md §4.2." >&2
  exit 2
fi
if [[ ! -x "$VERIFIER" ]] && [[ ! -f "$VERIFIER" ]]; then
  echo "ERROR: verifier missing at: $VERIFIER" >&2
  exit 2
fi

# --- Compute digest + size ------------------------------------------------

sha="$(compute_sha256 "$pdf_path")"
size_bytes="$(wc -c < "$pdf_path" | tr -d ' ')"
filename="$(basename "$pdf_path")"
today="$(date -u +%Y-%m-%d)"
downloader="${USER:-unknown}"

echo "PDF:        $pdf_path"
echo "SHA-256:    $sha"
echo "Size:       $size_bytes bytes"
echo "Filename:   $filename"
echo "Report:     $report_name"
echo "Kind:       $kind"
echo "Principal:  $principal"
echo "Date (UTC): $today"
echo "Downloader: $downloader"
echo ""

# --- Idempotency: skip if already in ledger -------------------------------

if grep -q -E "^${sha}[[:space:]]" "$LEDGER"; then
  echo "NOTE: SHA-256 already present in ledger — no append (idempotent)."
  echo "      Existing line:"
  grep -E "^${sha}[[:space:]]" "$LEDGER" | sed 's/^/        /'
  echo ""
  echo "Running --verify-ledger anyway to confirm clean state…"
  bash "$VERIFIER" --verify-ledger
  exit 0
fi

# --- Append to ledger -----------------------------------------------------

echo "Appending to ledger…"
# Two-line block: a metadata comment, then the canonical receipt row.
{
  echo ""
  echo "# recorded=${today} report=\"${report_name}\" size=${size_bytes} by=${downloader}"
  echo "${sha}  ${filename}  # fetched=${today} principal=${principal} kind=${kind}"
} >> "$LEDGER"

echo "  + ${sha}  ${filename}  # fetched=${today} principal=${principal} kind=${kind}"
echo ""

# --- BYOK matrix TBD-on-receipt flip --------------------------------------

matrix_touched=""
if [[ "$update_matrix" -eq 1 ]]; then
  echo "Scanning $COMPLIANCE_DIR for sha256:TBD-on-receipt…"
  # Find compliance .md files that still carry the placeholder. We restrict
  # to the matrix-class document to avoid touching debt-register prose
  # references that intentionally mention the token verbatim.
  candidates=()
  while IFS= read -r f; do
    candidates+=("$f")
  done < <(grep -l "sha256:TBD-on-receipt" "$COMPLIANCE_DIR"/*.md 2>/dev/null | grep -E '(BYOK|byok|attestation-matrix|MATRIX)' || true)

  if [[ ${#candidates[@]} -eq 0 ]]; then
    echo "  No matrix candidate file contains sha256:TBD-on-receipt — skipping flip."
    echo "  (Renewal case: matrix already references a real digest.)"
  elif [[ ${#candidates[@]} -gt 1 ]]; then
    echo "ERROR: multiple matrix-class files contain sha256:TBD-on-receipt:" >&2
    for f in "${candidates[@]}"; do echo "    $f" >&2; done
    echo "  Disambiguate manually or re-run with --no-matrix-update." >&2
    echo "  Note: ledger append already committed to disk; rollback via" >&2
    echo "        git checkout -- $LEDGER" >&2
    exit 2
  else
    target="${candidates[0]}"
    echo "  Patching $target (sha256:TBD-on-receipt → sha256:${sha})"
    # Portable first-match replacement via awk (BSD/GNU sed differ on
    # the `0,/pat/` and `1,/pat/` range forms; awk is uniform).
    tmp_out="$(mktemp)"
    awk -v new="sha256:${sha}" '
      BEGIN { done = 0 }
      {
        if (!done) {
          idx = index($0, "sha256:TBD-on-receipt")
          if (idx > 0) {
            $0 = substr($0, 1, idx - 1) new substr($0, idx + length("sha256:TBD-on-receipt"))
            done = 1
          }
        }
        print
      }
    ' "$target" > "$tmp_out"
    mv "$tmp_out" "$target"
    # Verify the substitution actually fired.
    if grep -q "sha256:${sha}" "$target"; then
      matrix_touched="$target"
      echo "  OK: replacement applied."
    else
      echo "ERROR: awk substitution did not produce expected token in $target." >&2
      exit 2
    fi
  fi
else
  echo "Matrix update skipped (--no-matrix-update)."
fi
echo ""

# --- Verify ---------------------------------------------------------------

echo "Running scripts/verify-aws-artifact-pdf.sh --verify-ledger…"
if ! bash "$VERIFIER" --verify-ledger; then
  echo ""
  echo "ERROR: verify-ledger failed after append. The appended line is:" >&2
  tail -2 "$LEDGER" >&2
  echo "Rollback options:" >&2
  echo "  git checkout -- $LEDGER" >&2
  if [[ -n "$matrix_touched" ]]; then
    echo "  git checkout -- $matrix_touched" >&2
  fi
  exit 1
fi
echo ""

# --- Print git commands for Owner -----------------------------------------

short_date="${today}"
files_to_add=("$LEDGER")
if [[ -n "$matrix_touched" ]]; then
  files_to_add+=("$matrix_touched")
fi

echo "==============================================================="
echo "READY TO COMMIT. Copy-paste:"
echo ""
echo "  cd $REPO_ROOT"
echo "  git add \\"
for f in "${files_to_add[@]}"; do
  rel="${f#"$REPO_ROOT/"}"
  echo "    $rel \\"
done
echo "  "
echo "  git diff --cached    # review before committing"
echo ""
echo "  git commit -s -m \"\$(cat <<'EOF'"
echo "chore(debt-003): record AWS Artifact ${kind^^} PDF SHA-256 (${short_date})"
echo ""
echo "Owner-side closure of DEBT-003 §6 closure protocol — appends ledger"
echo "row for ${report_name} and flips the BYOK matrix"
echo "sha256:TBD-on-receipt placeholder to the recorded digest."
echo ""
echo "SHA-256: ${sha}"
echo "Filename: ${filename}"
echo "Recorded: ${today} by ${downloader}"
echo ""
echo "Per specs/_compliance/aws-artifact-placeholder.md §6 step 8 and"
echo "docs/internal/aws-artifact-fetch-quickstart.md."
echo ""
echo "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
echo "EOF"
echo "  )\""
echo "==============================================================="
echo ""
echo "OK — ledger appended, matrix flipped, verifier green."
echo "Next: review the diff, then run the commit command above."
exit 0
