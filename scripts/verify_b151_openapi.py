#!/usr/bin/env python3
"""Closed-world verifier for backlog item B-151.

The docs download and the canonical OpenAPI contract are one public contract.
This gate intentionally rejects both omissions and additions: a subset check
would allow a second, silently smaller API to be published again.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError:
    print("error: PyYAML required (pip install pyyaml)", file=sys.stderr)
    sys.exit(2)

HTTP_METHODS = frozenset({"get", "post", "put", "patch", "delete", "head", "options", "trace"})
EXPECTED_LOCALES = ("de", "es-419", "pt-BR")
STALE_RBAC_PHRASE = "five customer-facing categories"


class InstrumentError(ValueError):
    """A missing or malformed input makes a verdict untrustworthy."""


def _relative(path: Path, root: Path) -> str:
    try:
        return str(path.relative_to(root))
    except ValueError:
        return str(path)


def load_document(path: Path, label: str) -> dict[str, Any]:
    if not path.exists():
        raise InstrumentError(f"{label} missing: {path}")
    try:
        with path.open(encoding="utf-8") as handle:
            document = yaml.safe_load(handle)
    except Exception as exc:  # parser errors are instrument failures, not drift
        raise InstrumentError(f"{label} could not be parsed: {exc}") from exc
    if not isinstance(document, dict):
        raise InstrumentError(f"{label} must be a YAML mapping")
    paths = document.get("paths")
    if not isinstance(paths, dict) or not paths:
        raise InstrumentError(f"{label} has no paths")
    return document


def operation_surface(document: dict[str, Any], label: str) -> dict[str, dict[str, Any]]:
    """Return every path and HTTP method, retaining operation identity."""
    paths = document.get("paths")
    if not isinstance(paths, dict) or not paths:
        raise InstrumentError(f"{label} has no paths")
    surface: dict[str, dict[str, Any]] = {}
    for path, item in paths.items():
        if not isinstance(path, str) or not isinstance(item, dict):
            raise InstrumentError(f"{label} has malformed path item: {path!r}")
        operations: dict[str, Any] = {}
        for method, operation in item.items():
            if not isinstance(method, str):
                raise InstrumentError(f"{label} has non-string path-item key at {path}: {method!r}")
            if method.startswith("x-") or method.startswith("$"):
                continue
            if method not in HTTP_METHODS:
                raise InstrumentError(f"{label} has unknown path-item key {method!r} at {path}")
            if not isinstance(operation, dict):
                raise InstrumentError(f"{label} has malformed operation: {method.upper()} {path}")
            operations[method] = operation.get("operationId")
        if not operations:
            raise InstrumentError(f"{label} path has no HTTP operations: {path}")
        surface[path] = operations
    return surface


def compare_closed_world(canonical: dict[str, Any], published: dict[str, Any]) -> list[str]:
    """Report all closed-world and full-document differences."""
    canonical_surface = operation_surface(canonical, "canonical OpenAPI")
    published_surface = operation_surface(published, "published OpenAPI")
    errors: list[str] = []

    missing_paths = sorted(set(canonical_surface) - set(published_surface))
    extra_paths = sorted(set(published_surface) - set(canonical_surface))
    if missing_paths:
        errors.append("published OpenAPI is missing paths: " + ", ".join(missing_paths))
    if extra_paths:
        errors.append("published OpenAPI has extra paths: " + ", ".join(extra_paths))

    for path in sorted(set(canonical_surface) & set(published_surface)):
        canonical_methods = canonical_surface[path]
        published_methods = published_surface[path]
        missing_methods = sorted(set(canonical_methods) - set(published_methods))
        extra_methods = sorted(set(published_methods) - set(canonical_methods))
        if missing_methods:
            errors.append(
                f"published OpenAPI is missing methods at {path}: "
                + ", ".join(method.upper() for method in missing_methods)
            )
        if extra_methods:
            errors.append(
                f"published OpenAPI has extra methods at {path}: "
                + ", ".join(method.upper() for method in extra_methods)
            )
        for method in sorted(set(canonical_methods) & set(published_methods)):
            expected = canonical_methods[method]
            actual = published_methods[method]
            if expected != actual:
                errors.append(
                    f"published OpenAPI operationId drift at {method.upper()} {path}: "
                    f"{actual!r} != {expected!r}"
                )

    # The static file is generated from the canonical YAML.  Surface equality
    # alone would still permit silent schema/security/info drift, so compare
    # the complete parsed documents as the final closed-world boundary.
    if canonical != published:
        errors.append("published OpenAPI differs from canonical beyond path/method/operationId surface")
    return errors


def rbac_locale_paths(root: Path) -> list[Path]:
    base = root / "apps" / "docs" / "i18n"
    return [
        base / locale / "docusaurus-plugin-content-docs" / "current" / "explanation" / "rbac" / "index.mdx"
        for locale in EXPECTED_LOCALES
    ]


def check_rbac_locales(root: Path) -> list[str]:
    errors: list[str] = []
    paths = rbac_locale_paths(root)
    for path in paths:
        if not path.exists():
            errors.append(f"RBAC locale missing: {_relative(path, root)}")
            continue
        try:
            contents = path.read_text(encoding="utf-8")
        except OSError as exc:
            errors.append(f"RBAC locale could not be read: {_relative(path, root)}: {exc}")
            continue
        if STALE_RBAC_PHRASE in contents:
            errors.append(f"RBAC locale retains stale phrase at {_relative(path, root)}")
    return errors


def assess(root: Path) -> tuple[list[str], int, int]:
    canonical_path = root / "openapi" / "corelink-v1.yaml"
    published_path = root / "apps" / "docs" / "static" / "openapi-corelink-v1.yaml"
    canonical = load_document(canonical_path, "canonical OpenAPI")
    published = load_document(published_path, "published OpenAPI")
    canonical_surface = operation_surface(canonical, "canonical OpenAPI")
    published_surface = operation_surface(published, "published OpenAPI")
    if len(canonical_surface) < 10:
        raise InstrumentError(
            f"canonical OpenAPI declares only {len(canonical_surface)} paths; instrument or spec is broken"
        )
    errors = compare_closed_world(canonical, published) + check_rbac_locales(root)
    canonical_operations = sum(len(methods) for methods in canonical_surface.values())
    published_operations = sum(len(methods) for methods in published_surface.values())
    return errors, len(canonical_surface), canonical_operations if not errors else published_operations


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--expect", choices=("open", "done"), default="done")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args()
    try:
        errors, paths, operations = assess(args.root.resolve())
    except InstrumentError as exc:
        print(f"B-151 instrument failure: {exc}", file=sys.stderr)
        return 2

    actual = "done" if not errors else "open"
    if errors:
        for error in errors:
            print(f"B-151: {error}", file=sys.stderr)
    else:
        print(
            f"B-151 done: canonical and published OpenAPI match ({paths} paths, "
            f"{operations} operations); RBAC locales checked: {len(EXPECTED_LOCALES)}"
        )
    if actual != args.expect:
        print(f"B-151 expected {args.expect}, observed {actual}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
