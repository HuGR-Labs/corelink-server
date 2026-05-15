#!/usr/bin/env python3
"""Regenerate ``openapi/corelink-v1.json`` from the canonical
``openapi/corelink-v1.yaml`` and validate the result.

Single source of truth: the YAML. The JSON sibling exists for tooling
that cannot consume YAML (Redocly Bundle pipelines, some SDK generators,
JS-side fetch consumers) and is regenerated from the YAML by this
script.

Usage:
    python3 scripts/openapi_sync.py            # regenerate + validate
    python3 scripts/openapi_sync.py --check    # fail if regen would change anything

Exit codes:
    0  ok / already in sync
    1  validation failure
    2  drift detected under --check
"""

from __future__ import annotations

import argparse
import difflib
import json
import sys
from pathlib import Path

try:
    import yaml
except ImportError:
    print("error: PyYAML required (pip install pyyaml)", file=sys.stderr)
    sys.exit(1)

ROOT = Path(__file__).resolve().parent.parent
YAML_PATH = ROOT / "openapi" / "corelink-v1.yaml"
JSON_PATH = ROOT / "openapi" / "corelink-v1.json"


def load_spec() -> dict:
    with YAML_PATH.open() as f:
        return yaml.safe_load(f)


def validate_structure(spec: dict) -> list[str]:
    """Run the structural checks every CI run must pass. Returns a list
    of error strings (empty == ok). Catches the regressions that
    landed mid-authorship of the spec; the OpenAPI 3.1 meta-schema
    validation is layered on top in `validate_meta`."""

    errors: list[str] = []
    if not str(spec.get("openapi", "")).startswith("3.1"):
        errors.append(f"openapi version must start with 3.1; got {spec.get('openapi')!r}")
    paths = spec.get("paths") or {}
    if not paths:
        errors.append("no paths defined")
    operations = 0
    for path, item in paths.items():
        for method, op in (item or {}).items():
            if method.startswith("$"):
                continue
            if method not in {"get", "post", "put", "patch", "delete", "head", "options", "trace"}:
                continue
            operations += 1
            if "responses" not in op:
                errors.append(f"{method.upper()} {path} missing `responses`")
            if "summary" not in op:
                errors.append(f"{method.upper()} {path} missing `summary`")
    # Resolve refs (shallow)
    def resolve(ref: str) -> bool:
        if not ref.startswith("#/"):
            return True  # external refs not used
        cur = spec
        for part in ref.lstrip("#/").split("/"):
            if isinstance(cur, dict) and part in cur:
                cur = cur[part]
            else:
                return False
        return True

    def walk(node):
        if isinstance(node, dict):
            ref = node.get("$ref")
            if isinstance(ref, str) and not resolve(ref):
                errors.append(f"broken $ref: {ref}")
            for v in node.values():
                walk(v)
        elif isinstance(node, list):
            for v in node:
                walk(v)

    walk(spec)
    print(f"openapi structure: {len(paths)} paths, {operations} operations")
    return errors


def validate_meta(spec: dict) -> list[str]:
    """Run OpenAPI 3.1 meta-schema validation if `openapi-spec-validator`
    is installed; otherwise skip with a notice."""

    try:
        from openapi_spec_validator import validate
    except Exception:
        print("notice: openapi-spec-validator not installed; skipping meta-schema check")
        return []
    try:
        validate(spec)
        print("openapi 3.1 meta-schema: ok")
    except Exception as exc:
        return [f"meta-schema validation failed: {exc}"]
    return []


def emit_json(spec: dict) -> str:
    return json.dumps(spec, indent=2, sort_keys=False) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    spec = load_spec()
    errors = validate_structure(spec) + validate_meta(spec)
    if errors:
        for e in errors:
            print(f"error: {e}", file=sys.stderr)
        return 1

    new_json = emit_json(spec)
    if JSON_PATH.exists():
        existing = JSON_PATH.read_text()
    else:
        existing = ""

    if args.check:
        if existing == new_json:
            print(f"{JSON_PATH.name}: in sync")
            return 0
        print(f"error: {JSON_PATH.name} drift detected — run scripts/openapi_sync.py", file=sys.stderr)
        diff = difflib.unified_diff(
            existing.splitlines(keepends=True),
            new_json.splitlines(keepends=True),
            fromfile=str(JSON_PATH.relative_to(ROOT)) + " (on disk)",
            tofile=str(JSON_PATH.relative_to(ROOT)) + " (regenerated)",
            n=2,
        )
        sys.stderr.writelines(diff)
        return 2

    JSON_PATH.write_text(new_json)
    print(f"wrote {JSON_PATH.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
