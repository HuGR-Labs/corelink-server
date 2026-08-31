#!/usr/bin/env python3
"""Regenerate the derived copies of ``openapi/corelink-v1.yaml`` and
validate the result.

Single source of truth: the YAML. Two artefacts are generated from it:

* ``openapi/corelink-v1.json`` — for tooling that cannot consume YAML
  (Redocly Bundle pipelines, some SDK generators, JS-side fetch consumers).
* ``worker/src/lib/openapi_v1.ts`` — the module the Worker serves at
  ``GET /openapi.json``. It lives under ``worker/src`` because that is what
  ``worker/tsconfig.json`` includes and what wrangler bundles, and it is
  generated rather than hand-kept so the contract the customer downloads
  cannot drift from the contract we review. ``--check`` fails on either.

Usage:
    python3 scripts/openapi_sync.py            # regenerate + validate
    python3 scripts/openapi_sync.py --check    # fail if regen would change anything

Exit codes:
    0  ok / already in sync
    1  validation failure
    2  drift detected under --check (in EITHER artefact)
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
WORKER_TS_PATH = ROOT / "worker" / "src" / "lib" / "openapi_v1.ts"


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


def emit_worker_ts(spec: dict) -> str:
    """The Worker-side module served at `GET /openapi.json`.

    The spec is embedded as a JSON *string* and parsed on first use, not as a
    TypeScript object literal. Two reasons, both measured rather than assumed:
    a ~120 KB object literal makes `tsc` infer a correspondingly huge literal
    type on every typecheck, and the route hands the bytes straight to
    `new Response(...)` anyway, so an object would be parsed on load only to be
    re-serialised on use. `JSON.parse` of a frozen string is the cheaper path in
    both directions.

    `JSON.stringify` of the JSON text gives a correctly escaped JS string
    literal — no hand-rolled escaping, so there is no quote or newline in the
    spec that can break out of it.
    """
    payload = json.dumps(spec, separators=(",", ":"), sort_keys=False)
    return (
        "// GENERATED by scripts/openapi_sync.py from openapi/corelink-v1.yaml.\n"
        "// DO NOT EDIT. Drift is gated by `python3 scripts/openapi_sync.py --check`\n"
        "// in .github/workflows/openapi-validate.yml.\n"
        "//\n"
        "// Served at `GET /openapi.json` (worker/src/index.ts).\n"
        "\n"
        "const RAW = " + json.dumps(payload) + ";\n"
        "\n"
        "/** The published CoreLink API contract, parsed once per isolate. */\n"
        "export const corelinkV1Spec: Record<string, unknown> = JSON.parse(RAW) as Record<\n"
        "  string,\n"
        "  unknown\n"
        ">;\n"
    )


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

    artefacts = [
        (JSON_PATH, emit_json(spec)),
        (WORKER_TS_PATH, emit_worker_ts(spec)),
    ]

    if args.check:
        drifted = False
        for path, generated in artefacts:
            existing = path.read_text() if path.exists() else ""
            if existing == generated:
                print(f"{path.name}: in sync")
                continue
            drifted = True
            what = "drift detected" if existing else "MISSING"
            print(
                f"error: {path.relative_to(ROOT)} {what} — run scripts/openapi_sync.py",
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
