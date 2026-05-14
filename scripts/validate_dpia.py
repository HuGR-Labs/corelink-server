#!/usr/bin/env python3
"""
validate_dpia.py — CoreLink DPIA CI hook (WI-S11-008)

Detects PRs that change PII handling without a corresponding DPIA document.
Called by .github/workflows/tla_dsr_erasure_check.yml and dpia_check.yml.

Usage:
  python3 scripts/validate_dpia.py [--check-only] [--changed-files <file>]

Options:
  --check-only          Run presence-only check (no PR context needed). Verifies
                        that all registered DPIAs exist on disk and are non-empty.
  --changed-files <f>   Path to a file containing newline-separated list of changed
                        files (from git diff --name-only). If not provided, uses
                        git diff HEAD~1..HEAD.
  --pr-body <text>      PR body text for override detection.
  --override-approver   Flag: Privacy Officer has approved override via PR comment.

Exit codes:
  0  — OK (no PII signals, or DPIA present, or valid override)
  1  — DPIA required but missing (CI fail)
  2  — Usage error
  3  — Internal error

PII signal detection (conservative — DD-006):
  Only flags CLEAR PII signals to minimize false-positives:
  1. New enum value matching PII patterns in data_model.md or consent schema.
  2. New sub-processor entry in sub-processors.md.
  3. New purpose value in ConsentPurpose / PurposeBasis enums.
  4. New telemetry field matching PII patterns in telemetry code.
  5. New column added to Neon billing / D1 tables containing PII field names.

DPIA presence check:
  For each triggered PII signal, checks if a DPIA exists in legal/dpia/ covering
  the affected feature (slug matching). If missing, CI fails with guidance.

Override mechanism:
  PR comment `[dpia: skip; rationale: <X>]` requires Privacy Officer GitHub approval.
  Without approval, CI re-fails regardless of comment.

Metrics emitted (stdout, for Prometheus pushgateway via CI):
  corelink_dpia_pr_coverage_total{outcome='enforced'} 1
  corelink_dpia_pr_coverage_total{outcome='ok_dpia_present'} 1
  corelink_dpia_pr_coverage_total{outcome='skipped_with_rationale'} 1
  corelink_dpia_pr_coverage_total{outcome='check_only'} 1
"""

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

# ── Constants ────────────────────────────────────────────────────────────────

REPO_ROOT = Path(__file__).resolve().parent.parent
DPIA_DIR = REPO_ROOT / "legal" / "dpia"
LIA_DIR = REPO_ROOT / "legal" / "lia"

# PII signal patterns (conservative — DD-006)
PII_FIELD_PATTERNS = re.compile(
    r"""
    (?:email|name|address|phone|ip_addr|subject_id|user_id|
       tax_id|cpf|cnpj|vat_number|payment|card|stripe|
       geolocation|biometric|health|birth_date|gender|
       race|religion|political_opinion|sexual_orientation)
    """,
    re.IGNORECASE | re.VERBOSE,
)

# Files whose changes trigger PII signal detection
PII_TRIGGER_PATHS = [
    re.compile(r"specs/03_architecture/data_model\.md"),
    re.compile(r"specs/03_architecture/sub-processors\.md"),
    re.compile(r"crates/corelink-privacy-.*/src/.*\.rs"),
    re.compile(r"crates/corelink-billing-.*/src/.*\.rs"),
    re.compile(r"migrations/d1/.*\.sql"),
    re.compile(r"migrations/neon/.*\.sql"),
]

# Known DPIA slugs for each trigger path pattern (conservative — only known features)
PATH_TO_DPIA_SLUG = {
    r"data_model\.md": None,       # generic — must have ANY dpia in registry
    r"sub-processors\.md": None,   # generic — Privacy Officer reviews manually
    r"s07|dedup|cas_integrity|ac_meta": "s07-dedup-leakage",
    r"s09|telemetry|analytics|loki": "s09-telemetry-aggregation",
    r"s10|billing|stripe|invoice": "s10-billing-cross-border",
    r"s11|erasure|dsr|consent": None,  # WI-S11 DPIAs handled by direct DPIA presence check
}

# Override pattern in PR body
OVERRIDE_PATTERN = re.compile(
    r"\[dpia:\s*skip;\s*rationale:\s*(.+?)\]",
    re.IGNORECASE,
)

# Minimum DPIA file size (bytes) to be considered non-empty / non-stub
MIN_DPIA_SIZE = 2000


# ── Helpers ──────────────────────────────────────────────────────────────────

def emit_metric(outcome: str) -> None:
    """Emit Prometheus metric to stdout for CI log capture."""
    print(f"# METRIC corelink_dpia_pr_coverage_total{{outcome='{outcome}'}} 1")


def get_changed_files(changed_files_path: str | None) -> list[str]:
    """Return list of changed file paths."""
    if changed_files_path:
        with open(changed_files_path) as f:
            return [line.strip() for line in f if line.strip()]
    # Fall back to git diff
    try:
        result = subprocess.run(
            ["git", "diff", "--name-only", "HEAD~1..HEAD"],
            capture_output=True, text=True, cwd=REPO_ROOT,
        )
        if result.returncode != 0:
            # Try staged files (for pre-commit use)
            result = subprocess.run(
                ["git", "diff", "--cached", "--name-only"],
                capture_output=True, text=True, cwd=REPO_ROOT,
            )
        return [line.strip() for line in result.stdout.splitlines() if line.strip()]
    except FileNotFoundError:
        return []


def is_pii_trigger(changed_files: list[str]) -> list[str]:
    """Return list of changed files that match PII trigger paths."""
    triggered = []
    for f in changed_files:
        for pattern in PII_TRIGGER_PATHS:
            if pattern.search(f):
                triggered.append(f)
                break
    return triggered


def check_pii_content(filepath: str) -> bool:
    """Return True if the diff content contains PII field patterns (best-effort)."""
    try:
        result = subprocess.run(
            ["git", "diff", "HEAD~1..HEAD", "--", filepath],
            capture_output=True, text=True, cwd=REPO_ROOT,
        )
        added_lines = [
            line[1:] for line in result.stdout.splitlines()
            if line.startswith("+") and not line.startswith("+++")
        ]
        added_text = "\n".join(added_lines)
        return bool(PII_FIELD_PATTERNS.search(added_text))
    except Exception:
        # Conservative: if we can't check, assume PII signal
        return True


def find_dpia_for_trigger(trigger_file: str) -> str | None:
    """Find an appropriate DPIA slug for the triggered file, or None for generic check."""
    for pattern_str, slug in PATH_TO_DPIA_SLUG.items():
        if re.search(pattern_str, trigger_file, re.IGNORECASE):
            return slug
    return None


def dpia_exists(slug: str | None) -> bool:
    """Check if a DPIA exists for the given slug (or any DPIA if slug is None)."""
    if slug is None:
        # Generic: check if ANY dpia exists with size > MIN
        if not DPIA_DIR.exists():
            return False
        dpias = list(DPIA_DIR.glob("*.md"))
        substantive = [f for f in dpias if f.stat().st_size >= MIN_DPIA_SIZE and f.name != "REVIEW_PROCESS.md"]
        return len(substantive) > 0
    dpia_path = DPIA_DIR / f"{slug}.md"
    return dpia_path.exists() and dpia_path.stat().st_size >= MIN_DPIA_SIZE


def detect_override(pr_body: str) -> tuple[bool, str]:
    """Return (has_override, rationale) from PR body."""
    match = OVERRIDE_PATTERN.search(pr_body or "")
    if match:
        return True, match.group(1).strip()
    return False, ""


def check_only_mode() -> int:
    """Presence-only check: verify all registered DPIAs exist and are non-stub."""
    print("Running DPIA presence-only check (--check-only mode)...")
    all_ok = True

    if not DPIA_DIR.exists():
        print(f"ERROR: DPIA directory not found: {DPIA_DIR}", file=sys.stderr)
        return 1

    required_dpias = [
        "s07-dedup-leakage",
        "s09-telemetry-aggregation",
        "s10-billing-cross-border",
    ]
    required_lias = [
        "s09-telemetry-aggregation",
    ]
    required_process_docs = [
        DPIA_DIR / "REVIEW_PROCESS.md",
    ]
    required_templates = [
        REPO_ROOT / "specs" / "_templates" / "dpia.md",
        REPO_ROOT / "specs" / "_templates" / "lia.md",
    ]

    for slug in required_dpias:
        path = DPIA_DIR / f"{slug}.md"
        if not path.exists():
            print(f"MISSING DPIA: {path}", file=sys.stderr)
            all_ok = False
        elif path.stat().st_size < MIN_DPIA_SIZE:
            print(f"STUB DPIA (too small): {path} ({path.stat().st_size} bytes < {MIN_DPIA_SIZE})", file=sys.stderr)
            all_ok = False
        else:
            print(f"OK DPIA: {path} ({path.stat().st_size} bytes)")

    for slug in required_lias:
        path = LIA_DIR / f"{slug}.md"
        if not path.exists():
            print(f"MISSING LIA: {path}", file=sys.stderr)
            all_ok = False
        elif path.stat().st_size < MIN_DPIA_SIZE:
            print(f"STUB LIA (too small): {path}", file=sys.stderr)
            all_ok = False
        else:
            print(f"OK LIA: {path} ({path.stat().st_size} bytes)")

    for doc in required_process_docs:
        if not doc.exists():
            print(f"MISSING process doc: {doc}", file=sys.stderr)
            all_ok = False
        else:
            print(f"OK process doc: {doc}")

    for tmpl in required_templates:
        if not tmpl.exists():
            print(f"MISSING template: {tmpl}", file=sys.stderr)
            all_ok = False
        else:
            print(f"OK template: {tmpl}")

    if all_ok:
        emit_metric("check_only")
        print("\nDPIA presence check PASSED — all required artifacts present.")
        return 0
    else:
        print("\nDPIA presence check FAILED — see errors above.", file=sys.stderr)
        return 1


# ── Main ─────────────────────────────────────────────────────────────────────

def main() -> int:
    parser = argparse.ArgumentParser(description="CoreLink DPIA CI hook (WI-S11-008)")
    parser.add_argument("--check-only", action="store_true",
                        help="Presence-only check; no PR context needed")
    parser.add_argument("--changed-files", default=None,
                        help="Path to file with newline-separated changed file list")
    parser.add_argument("--pr-body", default="",
                        help="PR body text for override detection")
    parser.add_argument("--override-approver", action="store_true",
                        help="Privacy Officer has approved override (set by CI from GitHub approval status)")
    args = parser.parse_args()

    if args.check_only:
        return check_only_mode()

    # Full PR mode
    changed_files = get_changed_files(args.changed_files)
    if not changed_files:
        print("No changed files detected — skipping DPIA check.")
        emit_metric("ok_dpia_present")
        return 0

    triggered_files = is_pii_trigger(changed_files)
    if not triggered_files:
        print("No PII trigger paths changed — DPIA check not required.")
        emit_metric("ok_dpia_present")
        return 0

    # Check PII content in triggered files
    pii_files = []
    for f in triggered_files:
        if check_pii_content(f):
            pii_files.append(f)

    if not pii_files:
        print("PII trigger paths changed but no PII-pattern additions detected — no DPIA required.")
        emit_metric("ok_dpia_present")
        return 0

    print(f"PII signal detected in {len(pii_files)} file(s):")
    for f in pii_files:
        print(f"  - {f}")

    # Check override
    has_override, rationale = detect_override(args.pr_body)
    if has_override:
        if args.override_approver:
            print(f"DPIA override accepted — Privacy Officer approved. Rationale: {rationale}")
            emit_metric("skipped_with_rationale")
            return 0
        else:
            print("ERROR: [dpia: skip] comment found but Privacy Officer approval NOT confirmed.", file=sys.stderr)
            print("       Set --override-approver flag only when GitHub review approval is confirmed.", file=sys.stderr)
            print("       CI re-fails per DD-006 override policy.", file=sys.stderr)
            emit_metric("enforced")
            return 1

    # Check DPIA presence for each trigger
    missing_dpias = []
    for f in pii_files:
        slug = find_dpia_for_trigger(f)
        if not dpia_exists(slug):
            slug_display = slug if slug else "(any DPIA)"
            missing_dpias.append((f, slug_display))

    if not missing_dpias:
        print("DPIA present for all PII-impacting changes. OK.")
        emit_metric("ok_dpia_present")
        return 0

    # Fail CI
    print("\nERROR: DPIA required for PII handling change(s):", file=sys.stderr)
    for f, slug in missing_dpias:
        print(f"  - {f} → expected DPIA: legal/dpia/{slug}.md", file=sys.stderr)
    print("", file=sys.stderr)
    print("To resolve:", file=sys.stderr)
    print("  1. Create a DPIA using specs/_templates/dpia.md at legal/dpia/<feature-slug>.md", file=sys.stderr)
    print("  2. Get Privacy Officer review and approval on the PR.", file=sys.stderr)
    print("  3. Or add [dpia: skip; rationale: <X>] to PR body + Privacy Officer approval.", file=sys.stderr)
    print("  Reference: legal/dpia/REVIEW_PROCESS.md", file=sys.stderr)
    emit_metric("enforced")
    return 1


if __name__ == "__main__":
    sys.exit(main())
