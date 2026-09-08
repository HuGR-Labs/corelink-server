#!/usr/bin/env python3
"""Bounded, deterministic census of terms on the published claim surface.

This is primarily an inventory gate, with one deliberately narrow semantic
guard for the currently verified enterprise-adoption boundary.  ``BYOK``,
``Buck2`` and ``pentest`` occur in both legitimate context and claims that
still need triage.  Every occurrence is therefore recorded with its exact source line,
stable identity and file hash; a changed population is a fail-closed review
stop rather than a silently changed count.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
INVENTORY = REPO_ROOT / "scripts" / "published_claims_inventory.json"
SCHEMA = 1
ROOTS = ("apps/docs", "marketing", "legal")
ROOT_FILES = ("README.md",)
EXTENSIONS = {".md", ".mdx", ".html", ".htm"}
EXCLUDED_DIRS = {".git", ".docusaurus", ".wrangler", "build", "dist", "node_modules"}
TERMS = ("BYOK", "Buck2", "pentest")
TERM_RE = re.compile(
    # Normalize the spelling family, including the hyphenated form used by
    # the vendor RFPs, into the canonical ``pentest`` term below.
    r"\b(?:BYOK|Buck2|pentest|penetration[-\s]+test(?:ing)?)\b",
    re.IGNORECASE,
)
MAX_FILES = 2_000
MAX_FILE_BYTES = 8 * 1024 * 1024
MAX_TOTAL_BYTES = 128 * 1024 * 1024

# The owner evidence packet establishes that no external enterprise lighthouse
# or BYOK customer has adopted CoreLink yet. Keep this guard scoped to the
# customer-facing launch artifacts: runbooks and internal planning documents
# may legitimately describe the future gate or onboarding workflow. The
# negative/historical markers below permit explicit non-claims and provenance
# notes while rejecting a stale positive assertion or an equivalent rewrite.
ENTERPRISE_CLAIM_SURFACE_PREFIXES = (
    "marketing/launch/PRESS-RELEASE.md",
    "marketing/launch/SOCIAL/",
    "marketing/launch/PRODUCT-HUNT/",
    "marketing/launch/BLOG-POSTS/",
    "marketing/launch/CASE-STUDIES/enterprise-byok.md",
)
STALE_ENTERPRISE_ADOPTION_PATTERNS = (
    re.compile(r"\b(?:three|3)\s+lighthouse\s+customers?\b", re.IGNORECASE),
    re.compile(
        r"\b(?:two|2)\s+team[- ]tier\s+deployments?.{0,100}\b(?:one|1)\s+enterprise\s+BYOK\s+deployment\b",
        re.IGNORECASE,
    ),
    re.compile(r"\b(?:one|an|1)\s+enterprise\s+BYOK\s+deployment\b", re.IGNORECASE),
    re.compile(r"\b(?:one|an|1)\s+enterprise\s+BYOK\s+customer\b", re.IGNORECASE),
    re.compile(
        r"\b(?:enterprise\s+)?lighthouse\s+customers?\s+(?:have|has|adopted|signed|attested|deployed|completed|selected)\b",
        re.IGNORECASE,
    ),
    re.compile(
        r"\b(?:enterprise\s+BYOK\s+)?customer(?:s)?\s+(?:adopted|selected|deployed|signed|attested)\s+CoreLink\b",
        re.IGNORECASE,
    ),
    re.compile(r"\b(?:we|CoreLink)\s+(?:named|have|had)\s+three\s+lighthouse\s+customers?\b", re.IGNORECASE),
    re.compile(r"\b(?:our|one\s+of\s+our)\s+lighthouse\s+customers?\b", re.IGNORECASE),
    re.compile(r"\bat\s+our\s+three\s+lighthouse\s+customers?\b", re.IGNORECASE),
    re.compile(r"\bsanitized\s+variant\b.{0,40}\b(?:available|maintained)\b", re.IGNORECASE),
)
NONCLAIM_CONTEXT = re.compile(
    r"\b(?:no|not|never|none|unmet|pending|placeholder|draft|future|former|removed|"
    r"without|does\s+not|cannot|can't|not\s+yet|before\s+any|remain(?:s)?\s+unpopulated|"
    r"must\s+not|may\s+ship\s+only|not\s+authorized|previously)\b",
    re.IGNORECASE,
)


def _is_claim_surface(relative_path: str) -> bool:
    return any(
        relative_path == prefix or relative_path.startswith(prefix)
        for prefix in ENTERPRISE_CLAIM_SURFACE_PREFIXES
    )


def enterprise_adoption_violations(root: Path, paths: list[Path] | None = None) -> list[str]:
    """Return positive enterprise-adoption claims forbidden by the owner packet."""

    paths = published_files(root) if paths is None else paths
    violations: list[str] = []
    for path in paths:
        relative_path = path.relative_to(root).as_posix()
        if not _is_claim_surface(relative_path):
            continue
        for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if NONCLAIM_CONTEXT.search(line):
                continue
            for pattern in STALE_ENTERPRISE_ADOPTION_PATTERNS:
                if pattern.search(line):
                    violations.append(
                        f"{relative_path}:{line_number}: stale positive enterprise-adoption claim: {line.strip()}"
                    )
                    break
    return violations


def published_files(root: Path) -> list[Path]:
    paths: list[Path] = []
    for rel in ROOT_FILES:
        path = root / rel
        if path.is_file() and path.suffix.lower() in EXTENSIONS:
            paths.append(path)
    for rel in ROOTS:
        directory = root / rel
        if not directory.is_dir():
            continue
        for path in directory.rglob("*"):
            if not path.is_file() or path.suffix.lower() not in EXTENSIONS:
                continue
            try:
                relative_parts = path.relative_to(directory).parts
            except ValueError:
                continue
            if any(part in EXCLUDED_DIRS for part in relative_parts):
                continue
            paths.append(path)
    return sorted(set(paths))


def _check_bounds(paths: list[Path], root: Path) -> None:
    if len(paths) > MAX_FILES:
        raise ValueError(f"{len(paths)} files exceeds {MAX_FILES}")
    total = 0
    for path in paths:
        size = path.stat().st_size
        if size > MAX_FILE_BYTES:
            raise ValueError(f"{path.relative_to(root)} exceeds {MAX_FILE_BYTES} bytes")
        total += size
    if total > MAX_TOTAL_BYTES:
        raise ValueError(f"{total} bytes exceeds {MAX_TOTAL_BYTES}")


def _occurrence_id(path: str, line: int, term: str, text: str, ordinal: int) -> str:
    payload = f"{path}\0{line}\0{term.lower()}\0{text}\0{ordinal}".encode()
    return hashlib.sha256(payload).hexdigest()[:24]


def collect_occurrences(root: Path, paths: list[Path] | None = None) -> list[dict[str, object]]:
    paths = published_files(root) if paths is None else paths
    occurrences: list[dict[str, object]] = []
    ordinals: dict[tuple[str, int, str, str], int] = {}
    for path in paths:
        rel = path.relative_to(root).as_posix()
        text = path.read_text(encoding="utf-8")
        for line_number, line in enumerate(text.splitlines(), 1):
            for match in TERM_RE.finditer(line):
                matched = match.group(0).lower()
                term = "pentest" if matched.startswith("penetration") else next(
                    (canonical for canonical in TERMS if canonical.lower() == matched),
                    match.group(0),
                )
                key = (rel, line_number, term.lower(), line)
                ordinal = ordinals.get(key, 0)
                ordinals[key] = ordinal + 1
                occurrences.append({
                    "id": _occurrence_id(rel, line_number, term, line, ordinal),
                    "path": rel,
                    "line": line_number,
                    "term": term,
                    "text": line.strip(),
                    "triage": "untriaged",
                })
    return occurrences


def build_inventory(root: Path) -> dict[str, object]:
    paths = published_files(root)
    _check_bounds(paths, root)
    occurrences = collect_occurrences(root, paths)
    return {
        "schema": SCHEMA,
        "generated_by": "scripts/published_claims_census.py",
        "scope": {
            "roots": list(ROOTS),
            "root_files": list(ROOT_FILES),
            "extensions": sorted(EXTENSIONS),
            "excluded_dirs": sorted(EXCLUDED_DIRS),
        },
        "population": {
            "files": [path.relative_to(root).as_posix() for path in paths],
            "file_hashes": {
                path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
                for path in paths
            },
            "occurrence_ids": [entry["id"] for entry in occurrences],
        },
        "occurrences": occurrences,
    }


def _summary(inventory: dict[str, object]) -> str:
    occurrences = inventory.get("occurrences") or []
    by_term = {term: 0 for term in TERMS}
    files_by_term = {term: set() for term in TERMS}
    for entry in occurrences:
        if not isinstance(entry, dict):
            continue
        term = str(entry.get("term", "")).lower()
        canonical = next((name for name in TERMS if name.lower() == term), None)
        if canonical:
            by_term[canonical] += 1
            files_by_term[canonical].add(str(entry.get("path", "")))
    return " ".join(
        f"{term}={by_term[term]} occurrences/{len(files_by_term[term])} files"
        for term in TERMS
    )


def validate(root: Path, inventory_path: Path = INVENTORY) -> list[str]:
    if not inventory_path.is_file():
        return [f"HALT: census inventory is missing: {inventory_path}"]
    try:
        expected = json.loads(inventory_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        return [f"HALT: census inventory cannot be parsed: {exc}"]
    if not isinstance(expected, dict):
        return ["HALT: census inventory must be a JSON object"]
    if expected.get("schema") != SCHEMA:
        return [f"HALT: unsupported census schema: {expected.get('schema')!r}"]
    try:
        actual = build_inventory(root)
    except (OSError, UnicodeDecodeError, ValueError) as exc:
        return [f"HALT: published census cannot be derived: {exc}"]
    failures: list[str] = []
    failures.extend(
        f"HALT: {violation}"
        for violation in enterprise_adoption_violations(root)
    )
    expected_population = expected.get("population")
    actual_population = actual["population"]
    if not isinstance(expected_population, dict):
        return ["HALT: census population is missing or not an object"]
    if expected_population.get("files") != actual_population["files"]:
        failures.append("HALT: published file population changed; review new/deleted files")
    if expected_population.get("file_hashes") != actual_population["file_hashes"]:
        failures.append("HALT: published file bytes changed; re-census and triage the diff")
    expected_occurrences = expected.get("occurrences")
    actual_occurrences = actual["occurrences"]
    if not isinstance(expected_occurrences, list) or not expected_occurrences:
        failures.append("HALT: census has no occurrence population")
        expected_occurrences = []
    if not actual_occurrences:
        failures.append("HALT: published claim surface has no terms; scope likely disappeared")
    if expected_occurrences != actual_occurrences:
        expected_ids = [entry.get("id") for entry in expected_occurrences if isinstance(entry, dict)]
        actual_ids = [entry.get("id") for entry in actual_occurrences if isinstance(entry, dict)]
        if len(expected_ids) != len(set(expected_ids)):
            failures.append("HALT: census contains duplicate occurrence IDs")
        if len(actual_ids) != len(set(actual_ids)):
            failures.append("HALT: derived census contains duplicate occurrence IDs")
        if len(expected_occurrences) < len(actual_occurrences):
            failures.append(f"HALT: {len(actual_occurrences) - len(expected_occurrences)} new/changed occurrence(s) require triage")
        elif len(expected_occurrences) > len(actual_occurrences):
            failures.append(f"HALT: {len(expected_occurrences) - len(actual_occurrences)} inventoried occurrence(s) disappeared")
        failures.append("HALT: occurrence records changed (path/line/term/text); review before re-deriving")
    if expected_population.get("occurrence_ids") != [entry.get("id") for entry in expected_occurrences if isinstance(entry, dict)]:
        failures.append("HALT: population.occurrence_ids does not match inventory records")
    if expected_population.get("occurrence_ids") != [entry.get("id") for entry in actual_occurrences if isinstance(entry, dict)]:
        failures.append("HALT: population.occurrence_ids does not match derived records")
    for entry in expected_occurrences:
        if not isinstance(entry, dict) or entry.get("triage") != "untriaged":
            failures.append("HALT: occurrence triage field is not the explicit untriaged state")
            break
    return failures


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(REPO_ROOT))
    parser.add_argument("--inventory", default=str(INVENTORY))
    parser.add_argument("--write", action="store_true", help="write a newly derived inventory")
    parser.add_argument("--json", action="store_true", help="print the derived census")
    args = parser.parse_args(argv)
    root = Path(args.root).resolve()
    inventory_path = Path(args.inventory).resolve()
    try:
        derived = build_inventory(root)
    except (OSError, UnicodeDecodeError, ValueError) as exc:
        print(f"HALT: published census cannot be derived: {exc}", file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(derived, indent=2, sort_keys=False))
    violations = enterprise_adoption_violations(root)
    if violations:
        print("\n".join(f"HALT: {violation}" for violation in violations), file=sys.stderr)
        return 1
    if args.write:
        inventory_path.write_text(json.dumps(derived, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {inventory_path}: {_summary(derived)}")
        return 0
    failures = validate(root, inventory_path)
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"published claim census valid: {_summary(derived)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
