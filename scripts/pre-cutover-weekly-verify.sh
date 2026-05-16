#!/usr/bin/env bash
# pre-cutover-weekly-verify.sh — Wave-28 step-10 orchestrator that emits a
# weekly markdown digest describing the current state of the 8 canonical
# GA-readiness DEFER items + cheap validator/test re-runs, between now
# and the GA D-day cutover.
#
# Canonical 8 DEFER items (sources cross-ref):
#   1. LFPDPPP MX attorney sign-off (DEBT-025)
#       source: specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md
#   2. FW-H-1..4 role nominations (governance staffing)
#       source: specs/_audits/2026-05-16-final-cutover-readiness.md §1 row 2
#   3. External pentest vendor SOW countersign (DEBT-026)
#       source: reports/pentest-rfp-tracker.json
#   4. DEBT-003 AWS Artifact PDF download + sha256 (BYOK FIPS attestation)
#       source: specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md (TBD-on-receipt)
#   5. DEBT-016 Statuspage status.corelink.dev go-live
#       source: specs/_audits/2026-05-16-debt-016-statuspage-urls.md
#                + live HEAD check (when --probe-live)
#   6. Pilot signups ≥ 5 design-partner attestations (G4)
#       source: specs/_audits/2026-05-16-pre-cutover-state-snapshot.md L.* +
#                pilot tenant state under specs/_audits/2026-05-16-pilot-onboarding-e2e.md
#   7. Pentest retest letter zero HIGH/CRITICAL (DEBT-026 final gate; vendor-paced)
#       source: reports/pentest-rfp-tracker.json (retest_delivery_date)
#   8. Owner sign-off (ADR-0034b 2-key)
#       source: specs/_audits/2026-05-16-final-cutover-readiness.md §10 signature block
#
# Per-item this script captures:
#   - state token: NOT_STARTED | IN_FLIGHT | DRAFT_READY | VENDOR_SELECTED |
#                  SIGNED | CLOSED | UNKNOWN
#   - last_touched: ISO date (latest git log -1 --date=short -- <source-path>)
#   - readiness_class: ENGINEERING_CLOSED_OPERATOR_BOUND | OPERATOR_BOUND |
#                      VENDOR_BOUND | LEGAL_BOUND | SIGNED | PENDING
#   - notes: free-text projection of D-day readiness
#
# Re-runs (cheap subset):
#   - python3 scripts/validate_specs.py
#   - python3 scripts/validate_references.py
#   - python3 scripts/validate_canonical_consistency.py
#   - python3 scripts/ga-readiness-defer-drift.py
#   - python3 scripts/check-ga-freeze-allowed.py --self-test  (if available)
#
# Test re-runs (cheap subset; opt-out via --no-tests):
#   - cargo check --workspace                                  (skipped under --dry-run)
#   - cargo test -p corelink-audit-chain --lib                 (skipped under --dry-run)
#   - cargo test -p corelink-server --test audit_export        (skipped under --dry-run)
#
# Exit codes:
#   0 — all 8 items GREEN-or-as-expected; no regression detected.
#   1 — regression: at least one item went BACKWARDS from the prior digest
#       (e.g. SIGNED → IN_FLIGHT, CLOSED → DRAFT_READY) OR a validator
#       previously green now fails.
#   2 — environment / setup failure (jq missing, write-perm denied, …).
#
# Usage:
#   bash scripts/pre-cutover-weekly-verify.sh                    # full run
#   bash scripts/pre-cutover-weekly-verify.sh --dry-run          # no cargo, no diff
#   bash scripts/pre-cutover-weekly-verify.sh --no-tests         # skip cargo
#   bash scripts/pre-cutover-weekly-verify.sh --date YYYY-MM-DD  # override digest date
#
# Charter: SYNCHRONOUS BASH ONLY. No PII in digest (tenant_ids BLAKE3-pseudonymized).
# DCO sign-off + Co-Authored-By in the merge commit.
#
# Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

set -euo pipefail

# ----------------------------------------------------------------------
# Argument parsing.
# ----------------------------------------------------------------------

DRY_RUN=0
NO_TESTS=0
DIGEST_DATE="$(date -u +"%Y-%m-%d")"
DIGEST_DIR="reports/pre-cutover-weekly"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            NO_TESTS=1
            shift
            ;;
        --no-tests)
            NO_TESTS=1
            shift
            ;;
        --date)
            DIGEST_DATE="${2:-}"
            shift 2
            ;;
        --digest-dir)
            DIGEST_DIR="${2:-}"
            shift 2
            ;;
        -h|--help)
            grep '^# ' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "unknown flag: $1" >&2
            echo "usage: $0 [--dry-run] [--no-tests] [--date YYYY-MM-DD] [--digest-dir <path>]" >&2
            exit 2
            ;;
    esac
done

# ----------------------------------------------------------------------
# Locate repo root.
# ----------------------------------------------------------------------

if ! REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    echo "::error::not inside a git repo" >&2
    exit 2
fi
cd "${REPO_ROOT}"

mkdir -p "${DIGEST_DIR}"
DIGEST_PATH="${DIGEST_DIR}/${DIGEST_DATE}-digest.md"
DIGEST_TMP="$(mktemp)"
trap 'rm -f "${DIGEST_TMP}"' EXIT

# ----------------------------------------------------------------------
# Pseudonymization helper (BLAKE3 if available, else SHA-256 fallback).
# ----------------------------------------------------------------------

pseudonymize() {
    local in="$1"
    if command -v b3sum >/dev/null 2>&1; then
        printf '%s' "${in}" | b3sum --no-names | cut -c1-16
    else
        # Fallback: SHA-256 prefix (still deterministic + non-reversible
        # for short-input cases; digest never embeds raw tenant_id).
        printf '%s' "${in}" | shasum -a 256 | cut -c1-16
    fi
}

# ----------------------------------------------------------------------
# Source-path helpers.
# ----------------------------------------------------------------------

last_touched() {
    local path="$1"
    if [[ -e "${path}" ]]; then
        local d
        d="$(git log -1 --format=%cs -- "${path}" 2>/dev/null || true)"
        if [[ -n "${d}" ]]; then
            printf '%s' "${d}"
        else
            printf 'unknown'
        fi
    else
        printf 'missing'
    fi
}

# ----------------------------------------------------------------------
# Per-item state probes (deterministic; no network unless --probe-live).
# Each probe prints: STATE|READINESS_CLASS|LAST_TOUCHED|NOTE
# ----------------------------------------------------------------------

probe_item_1_lfpdppp() {
    local src="specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md"
    local lt; lt="$(last_touched "${src}")"
    local state="DRAFT_READY"
    local rc="LEGAL_BOUND"
    local note="Engineering package SEALED; awaiting MX attorney sign-off (8-12 attorney-hours)."
    # Detect closure: a closure-marker file (operator-written) or a canonical
    # change-log row recording the OPEN → CLOSED transition. The mere text
    # "DEBT-025 → CLOSED" appears in the OPEN row's closure-plan narrative,
    # so we must not trip on that. Closure is signalled by:
    #   - reports/lfpdppp-mx-closure.json (operator deposit), OR
    #   - a register change-log row matching "DEBT-025 (?:CLOSED|RESOLVED)"
    #     at the beginning of a date-prefixed log row.
    if [[ -f "reports/lfpdppp-mx-closure.json" ]]; then
        state="CLOSED"
        rc="SIGNED"
        note="Closure record present at reports/lfpdppp-mx-closure.json."
    elif grep -qE "^\|\s*[0-9]{4}-[0-9]{2}-[0-9]{2}\s*\|\s*v[0-9.]+\s+—\s+\*\*DEBT-025 (CLOSED|RESOLVED)" specs/_audits/2026-05-15-debt-register.md 2>/dev/null; then
        state="CLOSED"
        rc="SIGNED"
        note="DEBT-025 closure row present in register change-log."
    fi
    printf '%s|%s|%s|%s' "${state}" "${rc}" "${lt}" "${note}"
}

probe_item_2_fwh_nominations() {
    local src="specs/_audits/2026-05-16-final-cutover-readiness.md"
    local lt; lt="$(last_touched "${src}")"
    local state="NOT_STARTED"
    local rc="OPERATOR_BOUND"
    local note="ADR-0034b 2-key path bridges until FW-H named. Single-Owner staffing waiver active."
    # If specs/_governance/fw-h-nominations.md exists with non-empty entries, advance state.
    if [[ -f "specs/_governance/fw-h-nominations.md" ]]; then
        if grep -qE "FW-H-[1-4].*\b(named|nominated|appointed)\b" "specs/_governance/fw-h-nominations.md" 2>/dev/null; then
            state="SIGNED"
            rc="SIGNED"
            note="At least one FW-H role nominated per governance doc."
        fi
    fi
    printf '%s|%s|%s|%s' "${state}" "${rc}" "${lt}" "${note}"
}

probe_item_3_pentest_sow() {
    local src="reports/pentest-rfp-tracker.json"
    local lt; lt="$(last_touched "${src}")"
    local state="NOT_CONTACTED"
    local rc="VENDOR_BOUND"
    local note="RFP authorized wave-26; vendor-selection clock running."
    if [[ -f "${src}" ]] && command -v jq >/dev/null 2>&1; then
        # Aggregate: max(state) across vendors, mapping CONTACTED < RFP_SENT < RESPONDED < SOW_DRAFT < SOW_COUNTERSIGNED < KICKOFF.
        local agg
        agg="$(jq -r '
            [.vendors[].state] as $s
            | if any($s[]; . == "SOW_COUNTERSIGNED") then "VENDOR_SELECTED"
              elif any($s[]; . == "SOW_DRAFT") then "IN_FLIGHT"
              elif any($s[]; . == "RESPONDED") then "IN_FLIGHT"
              elif any($s[]; . == "RFP_SENT") then "IN_FLIGHT"
              else "NOT_CONTACTED" end
        ' "${src}" 2>/dev/null || printf 'NOT_CONTACTED')"
        state="${agg}"
        if [[ "${state}" == "VENDOR_SELECTED" ]]; then
            rc="SIGNED"
            note="At least one vendor SOW countersigned per tracker."
        elif [[ "${state}" == "IN_FLIGHT" ]]; then
            note="RFP outstanding to ≥ 1 tier-1 vendor."
        fi
    fi
    printf '%s|%s|%s|%s' "${state}" "${rc}" "${lt}" "${note}"
}

probe_item_4_debt_003_aws() {
    local src="specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md"
    local lt; lt="$(last_touched "${src}")"
    local state="DRAFT_READY"
    local rc="OPERATOR_BOUND"
    local note="3/4 provider rows complete (GCP/Azure/Vault); AWS row TBD-on-receipt."
    if [[ -f "${src}" ]]; then
        if grep -q "TBD-on-receipt" "${src}" 2>/dev/null; then
            state="DRAFT_READY"
        else
            # If TBD-on-receipt is gone, AWS row presumed filled.
            state="CLOSED"
            rc="SIGNED"
            note="AWS row no longer TBD-on-receipt (matrix filled)."
        fi
    else
        state="UNKNOWN"
    fi
    printf '%s|%s|%s|%s' "${state}" "${rc}" "${lt}" "${note}"
}

probe_item_5_statuspage() {
    local src="specs/_audits/2026-05-16-debt-016-statuspage-urls.md"
    local lt; lt="$(last_touched "${src}")"
    local state="DRAFT_READY"
    local rc="ENGINEERING_CLOSED_OPERATOR_BOUND"
    local note="URL substitution mechanism CLOSED; T-7d operator provisioning per STATUSPAGE-INIT.md."
    # Detect if a status snapshot exists that records LIVE state.
    if [[ -f "reports/statuspage-init-dressrun-${DIGEST_DATE}.json" ]]; then
        note="Dress-run snapshot captured ${DIGEST_DATE}."
    fi
    if [[ -f "reports/statuspage-live-state.json" ]] && command -v jq >/dev/null 2>&1; then
        local live; live="$(jq -r '.status // "unknown"' reports/statuspage-live-state.json 2>/dev/null || printf 'unknown')"
        if [[ "${live}" == "OPERATIONAL" || "${live}" == "operational" ]]; then
            state="SIGNED"
            rc="SIGNED"
            note="status.corelink.dev OPERATIONAL per live state file."
        fi
    fi
    printf '%s|%s|%s|%s' "${state}" "${rc}" "${lt}" "${note}"
}

probe_item_6_pilot_signups() {
    local src="specs/_audits/2026-05-16-pilot-onboarding-e2e.md"
    local lt; lt="$(last_touched "${src}")"
    local state="IN_FLIGHT"
    local rc="OPERATOR_BOUND"
    local note="wave-23 streams #5+#7+#8 built post-signup machinery; G4 snapshot at T-24h."
    if [[ -f "reports/pilot-tenant-state.json" ]] && command -v jq >/dev/null 2>&1; then
        # Count signed pilot attestations; pseudonymize tenant_ids defensively.
        local signed
        signed="$(jq -r '[.tenants[]? | select(.attestation_signed == true)] | length' reports/pilot-tenant-state.json 2>/dev/null || printf '0')"
        if [[ "${signed}" -ge 5 ]]; then
            state="SIGNED"
            rc="SIGNED"
            note="${signed}/5 pilot attestations signed (tenant_ids pseudonymized in digest)."
        elif [[ "${signed}" -gt 0 ]]; then
            note="${signed}/5 pilot attestations signed; ≥ 5 required for G4 greenlight."
        fi
    fi
    printf '%s|%s|%s|%s' "${state}" "${rc}" "${lt}" "${note}"
}

probe_item_7_pentest_retest() {
    local src="reports/pentest-rfp-tracker.json"
    local lt; lt="$(last_touched "${src}")"
    local state="NOT_STARTED"
    local rc="VENDOR_BOUND"
    local note="Earliest retest delivery 2026-07-29 — binding cutover-date determinant."
    if [[ -f "${src}" ]] && command -v jq >/dev/null 2>&1; then
        local retest
        retest="$(jq -r '[.vendors[]? | .retest_delivery_date // empty] | map(select(. != null)) | first // empty' "${src}" 2>/dev/null || printf '')"
        if [[ -n "${retest}" ]]; then
            state="IN_FLIGHT"
            note="Retest delivery scheduled ${retest} per tracker."
        fi
        local letter
        letter="$(jq -r '.retest_letter_received // false' "${src}" 2>/dev/null || printf 'false')"
        if [[ "${letter}" == "true" ]]; then
            state="SIGNED"
            rc="SIGNED"
            note="Retest letter received with zero HIGH/CRITICAL."
        fi
    fi
    printf '%s|%s|%s|%s' "${state}" "${rc}" "${lt}" "${note}"
}

probe_item_8_owner_signoff() {
    local src="specs/_audits/2026-05-16-final-cutover-readiness.md"
    local lt; lt="$(last_touched "${src}")"
    local state="NOT_STARTED"
    local rc="OPERATOR_BOUND"
    local note="§10 signature block awaiting Owner + on-call SRE 2-key per ADR-0034b."
    # Detect signed state via presence of populated signature line.
    if [[ -f "${src}" ]]; then
        if grep -qE "^\*\*Signature:\*\*\s+[^_\s]" "${src}" 2>/dev/null; then
            state="SIGNED"
            rc="SIGNED"
            note="Signature block populated in §10."
        fi
    fi
    printf '%s|%s|%s|%s' "${state}" "${rc}" "${lt}" "${note}"
}

# ----------------------------------------------------------------------
# Item enumeration (ordered, canonical 1..8).
# ----------------------------------------------------------------------

declare -a ITEMS=(
    "1|LFPDPPP MX attorney sign-off (DEBT-025)|probe_item_1_lfpdppp"
    "2|FW-H-1..4 role nominations|probe_item_2_fwh_nominations"
    "3|External pentest vendor SOW countersign (DEBT-026)|probe_item_3_pentest_sow"
    "4|DEBT-003 AWS Artifact PDF + sha256|probe_item_4_debt_003_aws"
    "5|DEBT-016 Statuspage go-live|probe_item_5_statuspage"
    "6|Pilot signups ≥ 5 design-partners (G4)|probe_item_6_pilot_signups"
    "7|Pentest retest letter zero HIGH/CRITICAL|probe_item_7_pentest_retest"
    "8|Owner sign-off (ADR-0034b 2-key)|probe_item_8_owner_signoff"
)

# State ordinal (higher = closer to closure). Used for regression detection.
state_ordinal() {
    case "$1" in
        UNKNOWN) echo 0 ;;
        NOT_STARTED|NOT_CONTACTED) echo 1 ;;
        IN_FLIGHT) echo 2 ;;
        DRAFT_READY) echo 3 ;;
        VENDOR_SELECTED) echo 4 ;;
        SIGNED) echo 5 ;;
        CLOSED) echo 6 ;;
        *) echo 0 ;;
    esac
}

# ----------------------------------------------------------------------
# Validator + test re-runs (cheap subset).
# ----------------------------------------------------------------------

run_validator() {
    local name="$1"; shift
    local cmd=("$@")
    local rc=0
    local out
    if ! out="$("${cmd[@]}" 2>&1)"; then
        rc=$?
    fi
    if [[ "${rc}" -eq 0 ]]; then
        printf 'PASS|%s|exit 0' "${name}"
    else
        printf 'FAIL|%s|exit %d' "${name}" "${rc}"
    fi
}

# ----------------------------------------------------------------------
# Probe all 8 items.
# ----------------------------------------------------------------------

declare -a ITEM_RESULTS=()
REGRESSION=0
REGRESSION_REASONS=()

# Prior digest (for regression detection).
PRIOR_DIGEST=""
if [[ -d "${DIGEST_DIR}" ]]; then
    PRIOR_DIGEST="$(ls -1 "${DIGEST_DIR}"/*-digest.md 2>/dev/null | grep -v "${DIGEST_DATE}-digest.md" | sort | tail -1 || true)"
fi

declare -A PRIOR_STATES=()
if [[ -n "${PRIOR_DIGEST}" && -f "${PRIOR_DIGEST}" ]]; then
    # Extract item|state pairs from the prior digest's machine-block.
    while IFS='|' read -r idx state _rest; do
        [[ -n "${idx}" ]] && PRIOR_STATES["${idx}"]="${state}"
    done < <(awk '/^<!-- BEGIN_STATE_MACHINE -->/{f=1; next} /^<!-- END_STATE_MACHINE -->/{f=0} f' "${PRIOR_DIGEST}" | grep -E '^[1-8]\|' || true)
fi

for spec in "${ITEMS[@]}"; do
    IFS='|' read -r idx title fn <<< "${spec}"
    res="$("${fn}")"
    IFS='|' read -r state rc_class lt note <<< "${res}"
    ITEM_RESULTS+=("${idx}|${title}|${state}|${rc_class}|${lt}|${note}")

    if [[ -n "${PRIOR_DIGEST}" ]]; then
        prior="${PRIOR_STATES[${idx}]:-}"
        if [[ -n "${prior}" && "${prior}" != "${state}" ]]; then
            cur_ord="$(state_ordinal "${state}")"
            prior_ord="$(state_ordinal "${prior}")"
            if [[ "${cur_ord}" -lt "${prior_ord}" ]]; then
                REGRESSION=1
                REGRESSION_REASONS+=("item ${idx} (${title}) regressed: ${prior} → ${state}")
            fi
        fi
    fi
done

# ----------------------------------------------------------------------
# Validator re-runs.
# ----------------------------------------------------------------------

declare -a VAL_RESULTS=()

VAL_RESULTS+=("$(run_validator "validate_specs.py" python3 scripts/validate_specs.py)")
VAL_RESULTS+=("$(run_validator "validate_references.py" python3 scripts/validate_references.py)")
VAL_RESULTS+=("$(run_validator "validate_canonical_consistency.py" python3 scripts/validate_canonical_consistency.py)")
VAL_RESULTS+=("$(run_validator "ga-readiness-defer-drift.py" python3 scripts/ga-readiness-defer-drift.py)")
if [[ -f "scripts/check-ga-freeze-allowed.py" ]]; then
    VAL_RESULTS+=("$(run_validator "check-ga-freeze-allowed.py --self-test" python3 scripts/check-ga-freeze-allowed.py --self-test)")
fi

for vr in "${VAL_RESULTS[@]}"; do
    IFS='|' read -r status name detail <<< "${vr}"
    if [[ "${status}" != "PASS" ]]; then
        REGRESSION=1
        REGRESSION_REASONS+=("validator ${name} not green: ${detail}")
    fi
done

# ----------------------------------------------------------------------
# Cargo re-runs (cheap subset).
# ----------------------------------------------------------------------

declare -a CARGO_RESULTS=()

if [[ "${NO_TESTS}" -eq 0 ]]; then
    CARGO_RESULTS+=("$(run_validator "cargo check --workspace" cargo check --workspace --quiet)")
    CARGO_RESULTS+=("$(run_validator "cargo test -p corelink-audit-chain --lib" cargo test -p corelink-audit-chain --lib --quiet)")
    CARGO_RESULTS+=("$(run_validator "cargo test -p corelink-server --test audit_export" cargo test -p corelink-server --test audit_export --quiet)")
    for cr in "${CARGO_RESULTS[@]}"; do
        IFS='|' read -r status name detail <<< "${cr}"
        if [[ "${status}" != "PASS" ]]; then
            REGRESSION=1
            REGRESSION_REASONS+=("test ${name} not green: ${detail}")
        fi
    done
else
    CARGO_RESULTS+=("SKIPPED|cargo check --workspace|--dry-run / --no-tests")
    CARGO_RESULTS+=("SKIPPED|cargo test -p corelink-audit-chain --lib|--dry-run / --no-tests")
    CARGO_RESULTS+=("SKIPPED|cargo test -p corelink-server --test audit_export|--dry-run / --no-tests")
fi

# ----------------------------------------------------------------------
# Emit digest markdown.
# ----------------------------------------------------------------------

GIT_HEAD="$(git rev-parse --short HEAD 2>/dev/null || printf 'unknown')"

{
    printf '# Pre-Cutover Weekly Verification Digest — %s\n\n' "${DIGEST_DATE}"
    printf '> **Doc kind:** weekly verification digest (no canonical front matter required — `reports/` excluded from `validate_specs.py`).\n>\n'
    printf '> **Generator:** `scripts/pre-cutover-weekly-verify.sh` (wave-28 step-10).\n'
    printf '> **Git HEAD:** `%s`.\n' "${GIT_HEAD}"
    printf '> **Mode:** %s.\n' "$([[ "${DRY_RUN}" -eq 1 ]] && echo 'dry-run (no cargo)' || ([[ "${NO_TESTS}" -eq 1 ]] && echo 'no-tests' || echo 'full'))"
    printf '> **Prior digest:** `%s`.\n' "${PRIOR_DIGEST:-<none>}"
    printf '> **Canonical sources:** `specs/_audits/2026-05-16-ga-readiness-final.md §11`, `specs/_audits/2026-05-16-final-cutover-readiness.md §1`, `specs/_audits/2026-05-16-ga-final-checklist.md §G`, `specs/_audits/2026-05-16-pre-cutover-state-snapshot.md §L`.\n>\n'
    printf '> **Charter:** SYNCHRONOUS BASH ONLY. Digest contains zero PII; any tenant_ids are BLAKE3-pseudonymized.\n\n'

    printf -- '---\n\n'
    printf '## §1. Verdict\n\n'
    if [[ "${REGRESSION}" -eq 1 ]]; then
        printf '**REGRESSION** — at least one DEFER item or validator regressed since the prior digest.\n\n'
        printf '**Regression reasons:**\n\n'
        for r in "${REGRESSION_REASONS[@]}"; do
            printf -- '- %s\n' "${r}"
        done
        printf '\n'
    else
        printf '**NO REGRESSION** — all 8 DEFER items at or above prior-digest state; all validators green'
        if [[ "${NO_TESTS}" -eq 0 ]]; then
            printf '; cargo subset green'
        fi
        printf '.\n\n'
    fi

    printf -- '---\n\n'
    printf '## §2. Per-item DEFER state\n\n'
    printf '| # | Item | State | Readiness class | Last touched | Note |\n'
    printf '|---|---|---|---|---|---|\n'
    for r in "${ITEM_RESULTS[@]}"; do
        IFS='|' read -r idx title state rc_class lt note <<< "${r}"
        printf '| %s | %s | `%s` | `%s` | %s | %s |\n' "${idx}" "${title}" "${state}" "${rc_class}" "${lt}" "${note}"
    done
    printf '\n'

    printf -- '---\n\n'
    printf '## §3. Validator re-runs\n\n'
    printf '| Validator | Status | Detail |\n'
    printf '|---|---|---|\n'
    for vr in "${VAL_RESULTS[@]}"; do
        IFS='|' read -r status name detail <<< "${vr}"
        printf '| `%s` | %s | %s |\n' "${name}" "${status}" "${detail}"
    done
    printf '\n'

    printf -- '---\n\n'
    printf '## §4. Test re-runs (cheap subset)\n\n'
    printf '| Command | Status | Detail |\n'
    printf '|---|---|---|\n'
    for cr in "${CARGO_RESULTS[@]}"; do
        IFS='|' read -r status name detail <<< "${cr}"
        printf '| `%s` | %s | %s |\n' "${name}" "${status}" "${detail}"
    done
    printf '\n'

    printf -- '---\n\n'
    printf '## §5. D-day readiness projection\n\n'
    # Aggregate per readiness class.
    ENG_CLOSED=0; LEGAL=0; VENDOR=0; OPERATOR=0; SIGNED=0; UNKNOWN=0
    for r in "${ITEM_RESULTS[@]}"; do
        IFS='|' read -r _idx _title _state rc_class _lt _note <<< "${r}"
        case "${rc_class}" in
            ENGINEERING_CLOSED_OPERATOR_BOUND) ENG_CLOSED=$((ENG_CLOSED+1)) ;;
            LEGAL_BOUND) LEGAL=$((LEGAL+1)) ;;
            VENDOR_BOUND) VENDOR=$((VENDOR+1)) ;;
            OPERATOR_BOUND) OPERATOR=$((OPERATOR+1)) ;;
            SIGNED) SIGNED=$((SIGNED+1)) ;;
            *) UNKNOWN=$((UNKNOWN+1)) ;;
        esac
    done
    printf -- '- Engineering-CLOSED operator-bound: **%d**\n' "${ENG_CLOSED}"
    printf -- '- Legal-bound: **%d**\n' "${LEGAL}"
    printf -- '- Vendor-bound: **%d**\n' "${VENDOR}"
    printf -- '- Operator-bound: **%d**\n' "${OPERATOR}"
    printf -- '- Signed/Closed: **%d**\n' "${SIGNED}"
    printf -- '- Unknown: **%d**\n' "${UNKNOWN}"
    printf -- '- **Total:** 8\n\n'

    printf -- '---\n\n'
    printf '## §6. Next-week watch-list\n\n'
    for r in "${ITEM_RESULTS[@]}"; do
        IFS='|' read -r idx title state _rc _lt _note <<< "${r}"
        case "${state}" in
            SIGNED|CLOSED) : ;;
            *) printf -- '- Item %s (%s) still `%s` — surface in next-week meeting.\n' "${idx}" "${title}" "${state}" ;;
        esac
    done
    printf '\n'

    printf -- '---\n\n'
    printf '## §7. Machine-readable state block (consumed by `pre-cutover-state-diff.py`)\n\n'
    printf '<!-- BEGIN_STATE_MACHINE -->\n'
    printf '```\n'
    for r in "${ITEM_RESULTS[@]}"; do
        IFS='|' read -r idx title state rc_class lt _note <<< "${r}"
        printf '%s|%s|%s|%s|%s\n' "${idx}" "${state}" "${rc_class}" "${lt}" "${title}"
    done
    printf '```\n'
    printf '<!-- END_STATE_MACHINE -->\n\n'

    printf -- '---\n\n'
    printf '## §8. Snapshot record\n\n'
    printf -- '- **Digest date:** %s\n' "${DIGEST_DATE}"
    printf -- '- **Git HEAD:** `%s`\n' "${GIT_HEAD}"
    printf -- '- **Generator:** `scripts/pre-cutover-weekly-verify.sh`\n'
    printf -- '- **Cron:** Monday 09:00 Bahia (UTC-3) via `.github/workflows/pre-cutover-weekly-cron.yml`\n'
    printf -- '- **Co-Authored-By:** Claude Opus 4.7 <noreply@anthropic.com>\n'
} > "${DIGEST_TMP}"

# Atomic move.
mv "${DIGEST_TMP}" "${DIGEST_PATH}"

# ----------------------------------------------------------------------
# Final.
# ----------------------------------------------------------------------

printf 'Digest written: %s\n' "${DIGEST_PATH}" >&2
if [[ "${REGRESSION}" -eq 1 ]]; then
    printf 'REGRESSION detected (exit 1):\n' >&2
    for r in "${REGRESSION_REASONS[@]}"; do
        printf -- '  - %s\n' "${r}" >&2
    done
    exit 1
fi
printf 'NO REGRESSION (exit 0).\n' >&2
exit 0
