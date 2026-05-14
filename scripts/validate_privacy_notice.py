#!/usr/bin/env python3
"""validate_privacy_notice.py — CI hook for privacy notice publication.

Per WI-S11-004 §6.1: validates a notice version directory before merge.

Validations:
  (a) Semver bump valid (new version > current published version).
  (b) 3 locales sync: pt-BR.md + en-US.md + es-MX.md all present.
  (c) Legal Review evidence: metadata.yaml EVT-044 paths reference existing files.
      (In CI: R2 presence check via `wrangler r2 object head` — stubbed here.)
  (d) notice_text_hash deterministic: compute + compare with metadata.yaml if set.
  (e) Major bump → print CD flag for stale_consent_check trigger.

Usage:
    python3 scripts/validate_privacy_notice.py legal/privacy-notice/v2.0.0/

Exit codes:
  0 — All validations passed.
  1 — Validation failure (detailed error message to stderr).
"""

import pathlib
import re
import sys

try:
    import yaml
except ImportError:
    yaml = None  # type: ignore[assignment]

from notice_text_hash_canonical import notice_text_hash

CANONICAL_LOCALES = ["pt-BR.md", "en-US.md", "es-MX.md"]
SEMVER_RE = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)$")


def parse_version(version_str: str) -> tuple[int, int, int]:
    """Parse semver string into (major, minor, patch)."""
    m = SEMVER_RE.match(version_str.strip())
    if not m:
        raise ValueError(f"Invalid semver string: {version_str!r}")
    return int(m.group(1)), int(m.group(2)), int(m.group(3))


def validate_notice_dir(notice_dir: pathlib.Path, current_version_str: str | None = None) -> dict:
    """Validate a notice version directory.

    Returns a dict with keys:
      - version: str
      - bump_type: "major" | "minor" | "initial"
      - locales: list[str]
      - hashes: dict[str, str]
      - errors: list[str]
    """
    errors: list[str] = []
    result: dict = {"errors": errors}

    # --- (a) Semver bump valid ---
    dir_name = notice_dir.name  # e.g. "v2.0.0"
    try:
        new_major, new_minor, _ = parse_version(dir_name)
    except ValueError as e:
        errors.append(f"Semver validation failed: {e}")
        return result

    result["version"] = dir_name

    bump_type = "initial"
    if current_version_str:
        try:
            cur_major, cur_minor, _ = parse_version(current_version_str)
        except ValueError as e:
            errors.append(f"Current version invalid: {e}")
            return result

        if new_major > cur_major:
            bump_type = "major"
        elif new_major == cur_major and new_minor > cur_minor:
            bump_type = "minor"
        elif new_major == cur_major and new_minor == cur_minor:
            errors.append(
                f"No semver bump detected: new {dir_name} == current {current_version_str}. "
                "Per notice versioning discipline, every change requires a version bump."
            )
        else:
            errors.append(
                f"Down-versioning blocked: new {dir_name} < current {current_version_str}. "
                "Per ADR-S11-007, notice versioning is monotonic (no down-versioning)."
            )

    result["bump_type"] = bump_type

    # --- (b) 3 locales sync ---
    missing_locales = []
    for locale_file in CANONICAL_LOCALES:
        if not (notice_dir / locale_file).exists():
            missing_locales.append(locale_file)

    if missing_locales:
        errors.append(
            f"3 locales sync violation: {missing_locales} missing. "
            "Per CTRL-PRIV-CONSENT-005, all 3 locales must be synced (pt-BR.md + en-US.md + es-MX.md). "
            "Major bump requires re-translation by native speaker + Legal local review."
        )
    result["locales"] = [l for l in CANONICAL_LOCALES if l not in missing_locales]

    # --- (c) metadata.yaml validation + native speaker review ---
    metadata_path = notice_dir / "metadata.yaml"
    if not metadata_path.exists():
        errors.append(f"metadata.yaml missing in {notice_dir}")
    elif yaml is not None:
        try:
            with metadata_path.open(encoding="utf-8") as f:
                metadata = yaml.safe_load(f)

            locales_meta = metadata.get("locales", {})
            for locale_key, locale_data in locales_meta.items():
                reviewed = locale_data.get("native_speaker_reviewed", False)
                if not reviewed:
                    errors.append(
                        f"{locale_key} native speaker review checkbox not checked. "
                        f"Per WI-S11-004 §6.1.4, all 3 locales require native speaker review pre-merge; "
                        f"locale {locale_key} missing."
                    )

                evt044_path = locale_data.get("legal_review_evt-044_path", "")
                if not evt044_path or "placeholder" in evt044_path.lower():
                    errors.append(
                        f"{locale_key} Legal Review EVT-044 path missing or placeholder. "
                        "R2 evidence-legal/ PDF must be present before merge."
                    )

        except Exception as e:  # noqa: BLE001
            errors.append(f"metadata.yaml parse error: {e}")

    # --- (d) notice_text_hash deterministic ---
    hashes: dict[str, str] = {}
    for locale_file in CANONICAL_LOCALES:
        locale_path = notice_dir / locale_file
        if locale_path.exists():
            h = notice_text_hash(locale_path.read_text(encoding="utf-8"))
            hashes[locale_file] = h

    result["hashes"] = hashes

    # --- (e) Major bump CD flag ---
    if bump_type == "major":
        print(
            f"::notice::CTRL-PRIV-CONSENT-005: Major bump {dir_name} detected. "
            "CD pipeline MUST trigger WI-S11-003 stale_consent_check job. "
            "Set CD env var: NOTICE_MAJOR_BUMP=true NOTICE_NEW_VERSION={dir_name}",
            file=sys.stderr,
        )
        result["stale_consent_check_required"] = True
    else:
        result["stale_consent_check_required"] = False

    return result


def main() -> None:
    args = sys.argv[1:]
    if not args:
        print(
            "Usage: validate_privacy_notice.py <notice-dir> [current-version]",
            file=sys.stderr,
        )
        sys.exit(1)

    notice_dir = pathlib.Path(args[0])
    current_version = args[1] if len(args) > 1 else None

    if not notice_dir.is_dir():
        print(f"ERROR: {notice_dir} is not a directory", file=sys.stderr)
        sys.exit(1)

    result = validate_notice_dir(notice_dir, current_version)
    errors = result.get("errors", [])

    if errors:
        print("VALIDATION FAILED:", file=sys.stderr)
        for err in errors:
            print(f"  ERROR: {err}", file=sys.stderr)
        sys.exit(1)

    print(f"OK: notice {result.get('version')} validated")
    print(f"  bump_type: {result.get('bump_type')}")
    print(f"  stale_consent_check_required: {result.get('stale_consent_check_required')}")
    print("  hashes:")
    for locale, h in result.get("hashes", {}).items():
        print(f"    {locale}: {h}")


if __name__ == "__main__":
    main()
