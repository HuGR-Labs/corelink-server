#!/usr/bin/env python3
"""CI hook: validate legal/sub-processors.md YAML frontmatter.

WI-S11-005 §6.1 scope:
  (a) YAML schema validation
  (b) semver bump on any change
  (c) per-sub-processor required fields validation
  (d) Legal Review evidence path check per sub-processor

Usage:
  python scripts/validate_sub_processors.py [--previous-version VERSION]

Exit 0 = valid; non-zero = invalid (CI fails).
"""

import sys
import re
import os
import argparse
from typing import Any

try:
    import yaml
except ImportError:
    print("ERROR: PyYAML not installed. Run: pip install pyyaml", file=sys.stderr)
    sys.exit(1)


REQUIRED_SP_FIELDS = [
    "id",
    "name",
    "role",
    "data_categories_processed",
    "region",
    "certifications",
    "dpa_url",
    "primary_jurisdiction",
    "contract_signed_at",
    "legal_review_evidence",
]

REQUIRED_TOP_FIELDS = [
    "version",
    "last_updated",
    "notification_required",
    "sub_processors",
]

SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+$")

EVIDENCE_PATH_RE = re.compile(r"^docs/compliance/vendor-reviews/.*\.md$")


def parse_frontmatter(path: str) -> dict[str, Any]:
    """Extract and parse YAML frontmatter from a markdown file."""
    with open(path, encoding="utf-8") as f:
        content = f.read()

    # Extract YAML between first --- pair
    if not content.startswith("---"):
        raise ValueError(f"{path}: file does not start with YAML frontmatter (---)")

    end_idx = content.index("---", 3)
    yaml_str = content[3:end_idx].strip()
    return yaml.safe_load(yaml_str)


def validate_semver(version: str) -> bool:
    """Validate semver format."""
    return bool(SEMVER_RE.match(version))


def semver_gt(a: str, b: str) -> bool:
    """Return True if semver a > semver b."""
    def parts(v: str) -> tuple[int, int, int]:
        p = v.split(".")
        return (int(p[0]), int(p[1]), int(p[2]))
    return parts(a) > parts(b)


def validate_sub_processors(data: dict[str, Any], previous_version: str | None) -> list[str]:
    """Validate sub-processors frontmatter. Returns list of error messages."""
    errors: list[str] = []

    # (a) Required top-level fields
    for field in REQUIRED_TOP_FIELDS:
        if field not in data:
            errors.append(f"Missing required top-level field: {field!r}")

    if errors:
        return errors  # Stop early if basic structure missing

    # (a) semver format
    version = str(data.get("version", ""))
    if not validate_semver(version):
        errors.append(f"version {version!r} is not valid semver (expected X.Y.Z)")

    # (b) semver bump check
    if previous_version is not None:
        if not validate_semver(previous_version):
            errors.append(f"previous_version {previous_version!r} is not valid semver")
        elif not semver_gt(version, previous_version):
            errors.append(
                f"version {version!r} must be greater than previous {previous_version!r} "
                "on any change to sub-processors.md"
            )

    # (c) sub_processors is a list
    sub_processors = data.get("sub_processors")
    if not isinstance(sub_processors, list):
        errors.append("sub_processors must be a YAML list")
        return errors

    if len(sub_processors) == 0:
        errors.append("sub_processors list is empty — at least 1 sub-processor required")

    seen_ids: set[str] = set()

    for i, sp in enumerate(sub_processors):
        prefix = f"sub_processors[{i}]"

        if not isinstance(sp, dict):
            errors.append(f"{prefix}: must be a mapping, got {type(sp).__name__}")
            continue

        sp_id = sp.get("id", f"<unknown at index {i}>")

        # Duplicate id check
        if sp_id in seen_ids:
            errors.append(f"{prefix}: duplicate id {sp_id!r}")
        seen_ids.add(str(sp_id))

        # (c) Required fields per sub-processor
        for field in REQUIRED_SP_FIELDS:
            if field not in sp:
                errors.append(f"{prefix} (id={sp_id!r}): missing required field {field!r}")

        # (d) Legal Review evidence path format check
        evidence = sp.get("legal_review_evidence", "")
        if evidence and not EVIDENCE_PATH_RE.match(str(evidence)):
            errors.append(
                f"{prefix} (id={sp_id!r}): legal_review_evidence {evidence!r} must match "
                "pattern docs/compliance/vendor-reviews/*.md"
            )

        # data_categories_processed must be a list
        if "data_categories_processed" in sp:
            dcp = sp["data_categories_processed"]
            if not isinstance(dcp, list) or len(dcp) == 0:
                errors.append(
                    f"{prefix} (id={sp_id!r}): data_categories_processed must be "
                    "a non-empty list"
                )

        # certifications must be a list
        if "certifications" in sp:
            certs = sp["certifications"]
            if not isinstance(certs, list):
                errors.append(
                    f"{prefix} (id={sp_id!r}): certifications must be a list"
                )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate legal/sub-processors.md YAML frontmatter (WI-S11-005 CI hook)"
    )
    parser.add_argument(
        "--file",
        default="legal/sub-processors.md",
        help="Path to sub-processors.md (default: legal/sub-processors.md)",
    )
    parser.add_argument(
        "--previous-version",
        default=None,
        help="Previous semver to compare against (for semver bump enforcement)",
    )
    args = parser.parse_args()

    file_path = args.file
    if not os.path.exists(file_path):
        print(f"ERROR: {file_path} not found", file=sys.stderr)
        return 1

    try:
        data = parse_frontmatter(file_path)
    except Exception as e:
        print(f"ERROR parsing frontmatter: {e}", file=sys.stderr)
        return 1

    errors = validate_sub_processors(data, args.previous_version)

    if errors:
        print(f"VALIDATION FAILED: {file_path}", file=sys.stderr)
        for err in errors:
            print(f"  - {err}", file=sys.stderr)
        return 1

    sp_count = len(data.get("sub_processors", []))
    version = data.get("version", "?")
    print(f"OK: {file_path} v{version} — {sp_count} sub-processors validated")
    return 0


if __name__ == "__main__":
    sys.exit(main())
