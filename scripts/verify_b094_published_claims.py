#!/usr/bin/env python3
"""Deterministic inventory gate for the published B-094 surface.

The old B-094 check was a closed list of English regular expressions.  It could
miss a new wording, could not account for every occurrence, and used population
floors that were easy to turn into folklore.  This gate has two independent
parts:

* the checked-in manifest gives every Buck2/Pants/gRPC occurrence a stable
  identity (path + normalized line + duplicate ordinal) and an explicit reason;
  an added, removed, or changed occurrence is a HALT until the inventory is
  deliberately re-derived;
* a small semantic guard rejects positive Buck2/Pants support claims even when
  the wording is new.  Negative status, protocol descriptions, competitor
  copy, and vocabulary lists remain valid inventory entries.

Only the customer-facing Markdown/MDX/HTML roots and the explicit Docusaurus
metadata sources listed in the manifest are in scope.  No network access is
performed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import unicodedata
from pathlib import Path
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parent.parent
MANIFEST = REPO_ROOT / "scripts" / "b094_published_inventory.json"
SCHEMA = 1
EXTENSIONS = {".md", ".mdx", ".html", ".htm"}
ROOTS = ("apps/docs", "marketing", "legal")
ROOT_FILES = ("README.md",)
# ``docusaurus.config.ts`` is executable source, but its ``metadata`` array is
# emitted directly into the public site head.  It therefore needs the same
# inventory/hash protection as authored pages rather than a broad ``*.ts``
# sweep that would mix implementation-only source into the published corpus.
PUBLISHED_METADATA_FILES = ("apps/docs/docusaurus.config.ts",)
WALK_EXCLUDED = {"node_modules", "build", "dist", ".docusaurus", ".wrangler"}
TERM_RE = re.compile(r"\b(?:buck2|pants|gRPC)\b", re.IGNORECASE)

CLASSIFICATIONS = {
    "negative-status": "an explicit unsupported/unavailable/removed status",
    "protocol-context": "a protocol or implementation description, not a product promise",
    "competitor-context": "a comparison describing another product or protocol",
    "vocabulary": "a linter/style vocabulary or other non-customer text",
    "historical": "a dated or historical record retained for provenance",
    "instructional-counterexample": "an exact, reviewed example of a claim support staff must not send",
}

# This is intentionally a path-and-line allowlist, not an ``example`` keyword
# escape hatch.  The template introduces the quoted sentence as a reply agents
# must never send; changing either the location or wording makes it ordinary
# customer-facing copy again and the semantic guard evaluates it.
INSTRUCTIONAL_COUNTEREXAMPLES = {
    (
        "marketing/launch/SUPPORT-RESPONSE-TEMPLATES.md",
        'e.g., "Yes, we support Pants 2.20 — see docs link below."}}',
    ): CLASSIFICATIONS["instructional-counterexample"],
}

# These patterns intentionally describe the *relationship* rather than a fixed
# sentence.  A new sentence such as "CoreLink is fully compatible with Buck2"
# therefore fails even if it was not present when the manifest was generated.
POSITIVE_PATTERNS = (
    re.compile(r"\b(?:buck2|pants)\b(?:\W+\w+){0,3}\W+\b(?:is|are)\s+(?:(?:fully|directly|natively)\s+)?(?:supported|compatible|available|enabled|integrated)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b.{0,45}\b(?:fully\s+)?supported\s+by\s+\bcorelink\b", re.I),
    re.compile(r"\b(?:fully\s+supported|compatible|supported)\s+by\s+\bcorelink\b.{0,40}\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:supports?|accepts?|serves?|integrates?\s+with|works?\s+with)\s+(?:the\s+)?\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b.{0,45}\b(?:works?|connects?|runs?)\s+(?:with|on|against)\s+\bcorelink\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b.{0,60}\bcan\s+use\b.{0,40}\bcorelink\b", re.I),
    re.compile(r"\bcorelink\b.{0,60}\b(?:accepts?|supports?|serves?|integrates?\s+with|works?\s+with)\b.{0,60}\b(?:buck2|pants)\b", re.I),
    re.compile(r"\bcorelink\b.{0,60}\b(?:offers?|provides?)\b.{0,40}\b(?:buck2|pants)\b.{0,40}\b(?:integration|compatib(?:ility|le)|support)\b", re.I),
    re.compile(r"\bcorelink\b.{0,60}\b(?:offers?|provides?)\b.{0,40}\b(?:integration|compatib(?:ility|le)|support)\b.{0,40}\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b.{0,50}\b(?:is|are)\b.{0,25}\b(?:native|first[ -]class|built[ -]in)\b.{0,40}\bcorelink\b.{0,40}\b(?:integration|compatib(?:ility|le)|support)\b", re.I),
    re.compile(r"\buse\b.{0,20}\b(?:buck2|pants)\b.{0,30}\bwith\b.{0,30}\bcorelink\b.{0,60}\bremote\s+cache\b", re.I),
    re.compile(r"\buse\b.{0,20}\bcorelink\b.{0,60}\bremote\s+cache\b.{0,40}\b(?:for|with)\b.{0,20}\b(?:buck2|pants)\b", re.I),
    re.compile(r"\bcorelink\b.{0,60}\b(?:has|includes?|ships?\s+with|comes\s+with)\b.{0,40}\b(?:native|first[ -]class|built[ -]in)\b.{0,40}\b(?:buck2|pants)\b.{0,40}\b(?:integration|compatib(?:ility|le)|support)\b", re.I),
    re.compile(r"\bcorelink\b.{0,60}\b(?:has|includes?|ships?\s+with|comes\s+with)\b.{0,40}\b(?:buck2|pants)\b.{0,40}\b(?:native|first[ -]class|built[ -]in)\b.{0,40}\b(?:integration|compatib(?:ility|le)|support)\b", re.I),
)
NEGATION_RE = re.compile(
    r"\b(?:not|no|cannot|can't|never|without|unsupported|unavailable|removed|doesn['’]?t|does not|won['’]?t|won’t|only)\b",
    re.I,
)
CLAUSE_CONJUNCTION_RE = re.compile(
    r"\b(?:and|or|nor|but|however|although|while|yet)\b",
    re.I,
)
CORELINK_RE = re.compile(r"\bcorelink\b", re.I)
ACTOR_RE = re.compile(r"\b(?:corelink|buildbuddy|engflow|buildbarn|nativelink|bazel-remote|competitors?)\b", re.I)
COMPETITOR_RE = re.compile(r"\b(?:buildbuddy|engflow|buildbarn|nativelink|bazel-remote)\b", re.I)
RELATION_VERB_RE = re.compile(
    r"\b(?:support(?:s|ed)?|accept(?:s|ed)?|serv(?:e|es|ed)?|integrat(?:e|es|ed)?|"
    r"work(?:s|ed)?|offer(?:s|ed)?|provid(?:e|es|ed)?|has|have|includes?|ships?|comes?)\b",
    re.I,
)


def _is_clause_separator(text: str, index: int) -> bool:
    character = text[index]
    category = unicodedata.category(character)
    if category[:1] not in {"P", "S"}:
        return False
    if character in {"'", "’"}:
        previous = text[index - 1] if index else ""
        following = text[index + 1] if index + 1 < len(text) else ""
        if previous.isalnum() and following.isalnum():
            return False
    if character == "-":
        previous = text[index - 1] if index else ""
        following = text[index + 1] if index + 1 < len(text) else ""
        if previous.isalnum() and following.isalnum():
            return False
    return True


def _has_clause_boundary(text: str) -> bool:
    """Return whether text separates two independent claims.

    Inventory entries are line-oriented, so a full Markdown parser would not
    make the semantic check more reliable.  Instead, inspect the text directly
    between the nearest negation and positive match: any Unicode punctuation or
    symbol (including slash, en/em dash, and parentheses), or a coordinating
    conjunction, makes the negation belong to a neighbouring clause.  Apostrophe
    characters inside words remain part of contractions such as ``doesn't``.
    """
    if CLAUSE_CONJUNCTION_RE.search(text):
        return True
    return any(_is_clause_separator(text, index) for index in range(len(text)))


def _clause_start(text: str, position: int) -> int:
    """Find the start of the clause containing ``position``."""
    start = 0
    for index in range(position):
        if _is_clause_separator(text, index):
            start = index + 1
    for conjunction in CLAUSE_CONJUNCTION_RE.finditer(text, 0, position):
        start = max(start, conjunction.end())
    return start


def _clause_end(text: str, position: int) -> int:
    """Find the end of the clause containing ``position``."""
    for index in range(position, len(text)):
        if _is_clause_separator(text, index):
            return index
    conjunction = CLAUSE_CONJUNCTION_RE.search(text, position)
    return conjunction.start() if conjunction else len(text)


def _match_crosses_clause(text: str, match: re.Match[str]) -> bool:
    """Reject a broad pattern match that crossed a structural separator."""
    return any(_is_clause_separator(text, index) for index in range(match.start(), match.end()))


def _match_is_corelink_claim(text: str, match: re.Match[str]) -> bool:
    """Require the matched claim's local subject or object to name CoreLink."""
    clause_start = _clause_start(text, match.start())
    relation = RELATION_VERB_RE.search(match.group(0))
    if relation:
        verb_position = match.start() + relation.start()
        prefix = text[clause_start:verb_position]
        actors = list(ACTOR_RE.finditer(prefix))
        if actors:
            actor = actors[-1]
            if any(
                _is_clause_separator(text, index) and text[index] != ","
                for index in range(clause_start + actor.end(), verb_position)
            ):
                return False
            return actor.group(0).lower() == "corelink"
        if any(_is_clause_separator(text, index) for index in range(clause_start, verb_position)):
            return False
    suffix = text[match.end() : _clause_end(text, match.end())]
    if re.search(r"\b(?:by|with|against|for|on|via|through)\s+corelink\b", suffix, re.I):
        return True
    return bool(CORELINK_RE.search(match.group(0)))


def _match_is_recognized_competitor_claim(text: str, match: re.Match[str]) -> bool:
    """Allow only a known third-party subject in contextual inventory lines."""
    clause_start = _clause_start(text, match.start())
    relation = RELATION_VERB_RE.search(match.group(0))
    if relation:
        verb_position = match.start() + relation.start()
        prefix = text[clause_start:verb_position]
        actors = list(ACTOR_RE.finditer(prefix))
        if actors:
            actor = actors[-1].group(0)
            if any(
                _is_clause_separator(text, index) and text[index] != ","
                for index in range(clause_start + actors[-1].end(), verb_position)
            ):
                return False
            return bool(COMPETITOR_RE.fullmatch(actor))
    suffix = text[match.end() : _clause_end(text, match.end())]
    if re.search(r"\b(?:by|with|against|for|on|via|through)\s+" + COMPETITOR_RE.pattern, suffix, re.I):
        return True
    return bool(COMPETITOR_RE.search(match.group(0)) and not CORELINK_RE.search(match.group(0)))


def _negation_is_local(text: str, match: re.Match[str]) -> bool:
    """Check only negation attached to this positive match."""
    if NEGATION_RE.search(text[match.start() : match.end()]):
        return True
    previous_negations = list(NEGATION_RE.finditer(text, 0, match.start()))
    if not previous_negations:
        return False
    nearest = previous_negations[-1]
    return not _has_clause_boundary(text[nearest.end() : match.start()])


def published_files(root: Path) -> list[Path]:
    paths: list[Path] = []
    for rel in ROOT_FILES:
        path = root / rel
        if path.is_file() and path.suffix.lower() in EXTENSIONS:
            paths.append(path)
    for rel in PUBLISHED_METADATA_FILES:
        path = root / rel
        if path.is_file():
            paths.append(path)
    for rel in ROOTS:
        directory = root / rel
        if directory.is_dir():
            for current, directories, filenames in os.walk(directory):
                directories[:] = sorted(name for name in directories if name not in WALK_EXCLUDED)
                paths.extend(
                    Path(current) / name
                    for name in sorted(filenames)
                    if Path(name).suffix.lower() in EXTENSIONS
                )
    return sorted(set(paths))


def occurrence_id(path: str, text: str, ordinal: int) -> str:
    """Stable across line moves, but not across an unreviewed text change."""
    payload = f"{path}\0{text.strip()}\0{ordinal}".encode()
    return hashlib.sha256(payload).hexdigest()[:24]


def classify(path: str, text: str) -> tuple[str, str]:
    lower = text.lower()
    if (path, text.strip()) in INSTRUCTIONAL_COUNTEREXAMPLES:
        return "instructional-counterexample", INSTRUCTIONAL_COUNTEREXAMPLES[(path, text.strip())]
    if any(token in path.lower() for token in ("styles", "accept.txt", "translation-quality")):
        return "vocabulary", CLASSIFICATIONS["vocabulary"]
    if any(token in lower for token in ("competitor", "competitors", "protocol", "rfc-", "bazel-remote", "buildbuddy", "engflow", "buildbarn", "nativelink")) or "/vs-" in path.lower():
        return "competitor-context", CLASSIFICATIONS["competitor-context"]
    if any(token in lower for token in ("historical", "archive", "superseded", "was removed", "dated")):
        return "historical", CLASSIFICATIONS["historical"]
    if NEGATION_RE.search(lower):
        return "negative-status", CLASSIFICATIONS["negative-status"]
    return "protocol-context", CLASSIFICATIONS["protocol-context"]


def collect_occurrences(root: Path) -> list[dict[str, object]]:
    occurrences: list[dict[str, object]] = []
    duplicate_ordinals: dict[tuple[str, str], int] = {}
    for path in published_files(root):
        rel = path.relative_to(root).as_posix()
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except UnicodeDecodeError:
            continue
        for line_number, line in enumerate(lines, 1):
            if not TERM_RE.search(line):
                continue
            key = (rel, line.strip())
            ordinal = duplicate_ordinals.get(key, 0)
            duplicate_ordinals[key] = ordinal + 1
            category, reason = classify(rel, line)
            occurrences.append(
                {
                    "id": occurrence_id(rel, line, ordinal),
                    "path": rel,
                    "line": line_number,
                    "text": line.strip(),
                    "class": category,
                    "reason": reason,
                    "terms": sorted({match.group(0).lower() for match in TERM_RE.finditer(line)}),
                }
            )
    return occurrences


def build_manifest(root: Path) -> dict[str, object]:
    files = published_files(root)
    occurrences = collect_occurrences(root)
    # A sorted-line digest is intentionally invariant to harmless line moves,
    # while still making deletion/truncation of context (including lines that do
    # not mention Buck2/Pants/gRPC) a hard stop until the population is reviewed.
    file_hashes = {
        path.relative_to(root).as_posix(): hashlib.sha256(
            "\n".join(sorted(path.read_text(encoding="utf-8").splitlines())).encode("utf-8")
        ).hexdigest()
        for path in files
    }
    return {
        "schema": SCHEMA,
        "scope": {
            "roots": list(ROOTS),
            "root_files": list(ROOT_FILES),
            "metadata_files": list(PUBLISHED_METADATA_FILES),
            "extensions": sorted(EXTENSIONS),
            "excluded": {
                "internal_wiki": "docs/knowledge and generated OKF material are sources of truth, not published claims",
                "history": "CHANGELOG.md and changelog.d are release history, not the deployed docs surface",
                "source": "Rust/source files are product evidence; B-094 checks the published copy only",
                "metadata": "only explicit Docusaurus metadata emitters are included; implementation-only TypeScript remains source",
                "build": "node_modules, build, dist, and generated output are not authored surfaces",
            },
        },
        "population": {
            "files": [path.relative_to(root).as_posix() for path in files],
            "file_hashes": file_hashes,
            "occurrence_ids": [entry["id"] for entry in occurrences],
        },
        "classifications": CLASSIFICATIONS,
        "occurrences": occurrences,
    }


def positive_claims(occurrences: Iterable[dict[str, object]]) -> list[str]:
    failures: list[str] = []
    for entry in occurrences:
        text = str(entry["text"])
        # The exact reviewed template is an instructional counterexample, not
        # customer-facing copy.  Other contextual classes are checked per
        # positive match so ambient words such as "protocol" or "historical"
        # cannot waive a separate CoreLink claim.
        if entry.get("class") == "instructional-counterexample":
            continue
        for pattern in POSITIVE_PATTERNS:
            for match in pattern.finditer(text):
                if _match_crosses_clause(text, match):
                    continue
                if entry.get("class") in {"competitor-context", "historical", "vocabulary"}:
                    if _match_is_corelink_claim(text, match):
                        pass
                    elif _match_is_recognized_competitor_claim(text, match):
                        continue
                if _negation_is_local(text, match):
                    continue
                failures.append(f"{entry['path']}:{entry['line']}: positive support claim: {text}")
                break
            else:
                continue
            break
    return failures


def validate(root: Path, manifest_path: Path = MANIFEST) -> list[str]:
    failures: list[str] = []
    if not manifest_path.is_file():
        return [f"HALT: inventory manifest is missing: {manifest_path}"]
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        return [f"HALT: inventory manifest cannot be parsed: {exc}"]
    if manifest.get("schema") != SCHEMA:
        failures.append(f"HALT: unsupported inventory schema: {manifest.get('schema')!r}")
    occurrences = collect_occurrences(root)
    expected = manifest.get("occurrences")
    if not isinstance(expected, list) or not expected:
        failures.append("HALT: inventory has no occurrence population")
        expected = []
    if not occurrences:
        failures.append("HALT: published corpus is empty or truncated; no Buck2/Pants/gRPC occurrences were parsed")
    population = manifest.get("population")
    expected_files = set(population.get("files", [])) if isinstance(population, dict) else set()
    actual_files = {path.relative_to(root).as_posix() for path in published_files(root)}
    if not expected_files:
        failures.append("HALT: inventory has no published-file population")
    if expected_files - actual_files:
        failures.append("HALT: published file corpus is truncated; inventoried file(s) disappeared")
    if actual_files - expected_files:
        failures.append("HALT: new published file(s) are outside the inventory; re-derive and review the manifest")
    expected_hashes = population.get("file_hashes") if isinstance(population, dict) else None
    if not isinstance(expected_hashes, dict) or not expected_hashes:
        failures.append("HALT: inventory has no file-content population")
    else:
        actual_hashes = {
            path.relative_to(root).as_posix(): hashlib.sha256(
                "\n".join(sorted(path.read_text(encoding="utf-8").splitlines())).encode("utf-8")
            ).hexdigest()
            for path in published_files(root)
        }
        if expected_hashes != actual_hashes:
            failures.append("HALT: published file content/population changed; review for truncation and re-derive the manifest")
    expected_ids = {str(entry.get("id")) for entry in expected if isinstance(entry, dict)}
    actual_ids = {str(entry["id"]) for entry in occurrences}
    missing = sorted(expected_ids - actual_ids)
    unknown = sorted(actual_ids - expected_ids)
    if missing:
        failures.append(f"HALT: {len(missing)} inventoried occurrence(s) disappeared; re-derive and review the manifest")
    if unknown:
        failures.append(f"HALT: {len(unknown)} new or changed occurrence(s) are not classified in the manifest")
    expected_classes = set(CLASSIFICATIONS)
    for entry in expected:
        if not isinstance(entry, dict) or entry.get("class") not in expected_classes:
            failures.append(f"HALT: occurrence has no valid classification: {entry!r}")
    failures.extend(positive_claims(occurrences))

    anchor = root / "crates/corelink-container/src/routes/bazel_v2.rs"
    if not anchor.is_file() or "Buck2 cannot use these routes" not in anchor.read_text(encoding="utf-8"):
        failures.append("HALT: product anchor no longer records that Buck2 cannot use the REST routes; re-evaluate B-094")
    return failures


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=REPO_ROOT)
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    parser.add_argument("--write-manifest", action="store_true")
    args = parser.parse_args(argv)
    if args.write_manifest:
        args.manifest.write_text(json.dumps(build_manifest(args.root), indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"wrote B-094 inventory: {args.manifest}")
        return 0
    failures = validate(args.root, args.manifest)
    if failures:
        for failure in failures:
            print(f"B-094 INVALID: {failure}", file=sys.stderr)
        return 1
    print("B-094 OK: published occurrence inventory is complete and contains no positive Buck2/Pants support claim")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
