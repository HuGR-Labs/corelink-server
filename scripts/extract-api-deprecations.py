#!/usr/bin/env python3
"""
extract-api-deprecations.py — parse OpenAPI YAML for deprecated operations
and emit a structured report.

Reads `openapi/corelink-v1.yaml` (or the file passed via --spec) and walks every
operation under `paths.*.*`. For each operation that has `deprecated: true` it
extracts:

    operationId
    method (get/post/put/patch/delete)
    path
    tags
    x-stability                (ga | preview | internal)
    x-deprecated-since         (ISO-8601 date)
    x-sunset-date              (ISO-8601 date)
    x-replaced-by              (path or empty string)
    x-deprecation-issue        (URL)

It also reports operations that are missing the `x-stability` extension entirely
(necessary follow-up flagged by the 2026-05-15 baseline).

The script ALSO validates lead-time invariants used by the CI gate
(`.github/workflows/api-deprecation-check.yml`):

    - GA-tier deprecation: sunset_date - today >= 730 days
    - Preview-tier deprecation: sunset_date - today >= 90 days
    - Required keys present: x-sunset-date, x-replaced-by, x-deprecation-issue
    - x-sunset-date >= x-deprecated-since

Output formats:

    --format text  (default; human-readable summary)
    --format json  (structured JSON for CI consumption / dashboards)

Exit codes:

    0  no deprecated operations OR all deprecated operations pass invariants
    1  one or more deprecated operations violate an invariant
    2  spec file unreadable / invalid YAML / no paths section

Usage:

    python3 scripts/extract-api-deprecations.py
    python3 scripts/extract-api-deprecations.py --spec openapi/corelink-v1.yaml --format json
    python3 scripts/extract-api-deprecations.py --check-lead-time

Dependencies:
    pip install pyyaml
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import date, datetime
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError:
    sys.exit("ERROR: pyyaml not installed. `pip install pyyaml`")


REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SPEC = REPO_ROOT / "openapi" / "corelink-v1.yaml"

HTTP_METHODS = {"get", "post", "put", "patch", "delete", "options", "head", "trace"}

# Lead-time invariants from apps/docs/docs/explanation/api-stability.mdx §3.1
LEAD_TIME_DAYS = {
    "ga": 730,
    "preview": 90,
    # Internal endpoints are not required to follow a deprecation cycle; lead
    # time of 0 means "no enforcement". They are reported but never fail CI.
    "internal": 0,
}


def parse_iso_date(value: str) -> date:
    """Parse YYYY-MM-DD; raises ValueError on bad input."""
    return datetime.strptime(value, "%Y-%m-%d").date()


def extract_deprecations(spec: dict[str, Any]) -> list[dict[str, Any]]:
    """Walk the OpenAPI doc and return one record per deprecated operation."""
    out: list[dict[str, Any]] = []
    paths = spec.get("paths") or {}
    for path, methods in paths.items():
        if not isinstance(methods, dict):
            continue
        for method, op in methods.items():
            if method.lower() not in HTTP_METHODS:
                continue
            if not isinstance(op, dict):
                continue
            if not op.get("deprecated", False):
                continue
            out.append({
                "operationId": op.get("operationId", "<missing>"),
                "method": method.upper(),
                "path": path,
                "tags": op.get("tags", []),
                "x-stability": op.get("x-stability", "<missing>"),
                "x-deprecated-since": op.get("x-deprecated-since", "<missing>"),
                "x-sunset-date": op.get("x-sunset-date", "<missing>"),
                "x-replaced-by": op.get("x-replaced-by", "<missing>"),
                "x-deprecation-issue": op.get("x-deprecation-issue", "<missing>"),
            })
    return out


def list_missing_stability(spec: dict[str, Any]) -> list[dict[str, str]]:
    """Operations with no `x-stability` extension. Returned for visibility."""
    out: list[dict[str, str]] = []
    paths = spec.get("paths") or {}
    for path, methods in paths.items():
        if not isinstance(methods, dict):
            continue
        for method, op in methods.items():
            if method.lower() not in HTTP_METHODS or not isinstance(op, dict):
                continue
            if "x-stability" not in op:
                out.append({
                    "operationId": op.get("operationId", "<missing>"),
                    "method": method.upper(),
                    "path": path,
                })
    return out


def validate_invariants(
    deprecations: list[dict[str, Any]],
    today: date,
) -> list[str]:
    """Return a list of human-readable violations; empty if all pass."""
    violations: list[str] = []
    required_keys = ("x-sunset-date", "x-replaced-by", "x-deprecation-issue")
    for d in deprecations:
        op = d["operationId"]
        stability = d["x-stability"]
        # Required keys
        for key in required_keys:
            if d[key] == "<missing>":
                violations.append(
                    f"{op}: missing required key `{key}` for deprecated operation"
                )
        # Sunset-date present + format + lead time
        if d["x-sunset-date"] == "<missing>":
            continue
        try:
            sunset = parse_iso_date(d["x-sunset-date"])
        except ValueError:
            violations.append(
                f"{op}: x-sunset-date `{d['x-sunset-date']}` is not ISO-8601 YYYY-MM-DD"
            )
            continue
        if d["x-deprecated-since"] != "<missing>":
            try:
                since = parse_iso_date(d["x-deprecated-since"])
                if sunset < since:
                    violations.append(
                        f"{op}: x-sunset-date {sunset} is earlier than "
                        f"x-deprecated-since {since}"
                    )
            except ValueError:
                violations.append(
                    f"{op}: x-deprecated-since `{d['x-deprecated-since']}` is not ISO-8601"
                )
        required_days = LEAD_TIME_DAYS.get(stability, 0)
        if required_days == 0 and stability not in ("internal",):
            violations.append(
                f"{op}: missing or unknown x-stability `{stability}`; "
                f"cannot validate lead time"
            )
            continue
        gap_days = (sunset - today).days
        if gap_days < required_days:
            violations.append(
                f"{op}: stability=`{stability}` requires sunset >= {required_days}d in future; "
                f"got {gap_days}d (sunset={sunset}, today={today})"
            )
    return violations


def render_text(
    deprecations: list[dict[str, Any]],
    missing_stability: list[dict[str, str]],
    violations: list[str],
) -> str:
    lines: list[str] = []
    lines.append("=" * 72)
    lines.append("CoreLink API deprecation report")
    lines.append("=" * 72)
    lines.append(f"Deprecated operations: {len(deprecations)}")
    lines.append(f"Operations missing x-stability: {len(missing_stability)}")
    lines.append(f"Invariant violations: {len(violations)}")
    lines.append("")
    if deprecations:
        lines.append("-- Deprecated operations --")
        for d in deprecations:
            lines.append(
                f"  [{d['x-stability']:>8}] {d['method']:6} {d['path']}  "
                f"(op={d['operationId']})"
            )
            lines.append(
                f"           since={d['x-deprecated-since']} "
                f"sunset={d['x-sunset-date']} "
                f"replaced-by={d['x-replaced-by']} "
                f"issue={d['x-deprecation-issue']}"
            )
        lines.append("")
    if missing_stability:
        lines.append("-- Operations missing x-stability (follow-up) --")
        for m in missing_stability:
            lines.append(f"  {m['method']:6} {m['path']}  (op={m['operationId']})")
        lines.append("")
    if violations:
        lines.append("-- Invariant violations --")
        for v in violations:
            lines.append(f"  FAIL: {v}")
        lines.append("")
    else:
        lines.append("All invariants pass.")
    return "\n".join(lines)


def render_json(
    deprecations: list[dict[str, Any]],
    missing_stability: list[dict[str, str]],
    violations: list[str],
) -> str:
    return json.dumps(
        {
            "deprecated_count": len(deprecations),
            "deprecated": deprecations,
            "missing_stability": missing_stability,
            "violations": violations,
        },
        indent=2,
        sort_keys=True,
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Parse OpenAPI YAML for deprecated endpoints and validate lead-time invariants.",
    )
    parser.add_argument(
        "--spec",
        type=Path,
        default=DEFAULT_SPEC,
        help=f"Path to OpenAPI YAML (default: {DEFAULT_SPEC.relative_to(REPO_ROOT)})",
    )
    parser.add_argument(
        "--format",
        choices=("text", "json"),
        default="text",
        help="Output format (default: text)",
    )
    parser.add_argument(
        "--check-lead-time",
        action="store_true",
        help="Exit non-zero if any deprecated operation violates lead-time invariants. "
             "Enabled by default; pass --no-check-lead-time to disable.",
        default=True,
    )
    parser.add_argument(
        "--no-check-lead-time",
        dest="check_lead_time",
        action="store_false",
        help="Report violations but do not exit non-zero.",
    )
    parser.add_argument(
        "--today",
        type=str,
        default=None,
        help="Override today's date (YYYY-MM-DD) for deterministic CI tests.",
    )
    args = parser.parse_args()

    if not args.spec.exists():
        print(f"ERROR: spec not found at {args.spec}", file=sys.stderr)
        return 2
    try:
        spec = yaml.safe_load(args.spec.read_text())
    except yaml.YAMLError as exc:
        print(f"ERROR: invalid YAML in {args.spec}: {exc}", file=sys.stderr)
        return 2
    if not isinstance(spec, dict) or "paths" not in spec:
        print(f"ERROR: {args.spec} has no `paths` section", file=sys.stderr)
        return 2

    today = parse_iso_date(args.today) if args.today else date.today()

    deprecations = extract_deprecations(spec)
    missing_stability = list_missing_stability(spec)
    violations = validate_invariants(deprecations, today)

    if args.format == "json":
        print(render_json(deprecations, missing_stability, violations))
    else:
        print(render_text(deprecations, missing_stability, violations))

    if violations and args.check_lead_time:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
