#!/usr/bin/env python3
"""Regenerate OpenAPI sibling artifacts from the canonical YAML.

Single source of truth: the YAML. The JSON sibling exists for tooling that
cannot consume YAML, and the docs static YAML is the download served to
clients. Both are regenerated from the canonical source by this script.

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

from verify_b151_openapi import compare_closed_world, load_document

try:
    import yaml
except ImportError:
    print("error: PyYAML required (pip install pyyaml)", file=sys.stderr)
    sys.exit(1)

ROOT = Path(__file__).resolve().parent.parent
YAML_PATH = ROOT / "openapi" / "corelink-v1.yaml"
JSON_PATH = ROOT / "openapi" / "corelink-v1.json"
STATIC_PATH = ROOT / "apps" / "docs" / "static" / "openapi-corelink-v1.yaml"


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


def validate_static_contract(spec: dict) -> list[str]:
    """Validate the docs asset as the complete generated canonical contract."""
    try:
        published = load_document(STATIC_PATH, "published OpenAPI")
    except ValueError as exc:
        return [str(exc)]
    errors = compare_closed_world(spec, published)
    print(
        f"static OpenAPI contract: {len(published.get('paths') or {})} paths, "
        f"canonical has {len(spec.get('paths') or {})}"
    )
    return errors


def emit_json(spec: dict) -> str:
    return json.dumps(spec, indent=2, sort_keys=False) + "\n"


def emit_static_yaml() -> str:
    """Copy canonical source bytes so the published YAML is generated."""
    return YAML_PATH.read_text()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    spec = load_spec()
    errors = validate_structure(spec) + validate_meta(spec) + validate_static_contract(spec)
    if errors:
        for e in errors:
            print(f"error: {e}", file=sys.stderr)
        return 1

    artefacts = [(JSON_PATH, emit_json(spec)), (STATIC_PATH, emit_static_yaml())]
    if args.check:
        drifted = False
        for path, generated in artefacts:
            existing = path.read_text() if path.exists() else ""
            if existing == generated:
                print(f"{path.name}: in sync")
                continue
            drifted = True
            label = "drift detected" if existing else "MISSING"
            print(
                f"error: {path.relative_to(ROOT)} {label} — run scripts/openapi_sync.py",
                file=sys.stderr,
            )
            sys.stderr.writelines(
                difflib.unified_diff(
                    existing.splitlines(keepends=True),
                    generated.splitlines(keepends=True),
                    fromfile=str(path.relative_to(ROOT)) + " (on disk)",
                    tofile=str(path.relative_to(ROOT)) + " (regenerated)",
                    n=2,
                )
            )
        return 2 if drifted else 0
    for path, generated in artefacts:
        path.write_text(generated)
        print(f"wrote {path.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
