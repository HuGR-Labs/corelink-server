#!/usr/bin/env python3
"""Verify that every admitted B-101 audit finding has one recorded decision.

This is intentionally an ingestion gate, not a document-name grep. The
governed source registry defines the complete admitted population; an extra
file in that directory is red until it is explicitly admitted and decided. The
gate parses each admitted audit, checks its population, pins each source and
registry digest, and joins every parsed identifier to the versioned
finding-to-decision manifest. A decision is either an extant canonical B-ID with
an exact equivalence proof or a distinct canonical B-item proposal with source
evidence and a proof that it is not a duplicate.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

from backlog_verify import BLOCK_RE as BACKLOG_BLOCK_RE


REPO_ROOT = Path(__file__).resolve().parent.parent
MANIFEST_RELATIVE = Path("reports/audit-finding-decisions/v1.json")
SOURCE_DIRECTORY_RELATIVE = Path("reports/audit-finding-decisions/sources")
SOURCE_REGISTRY_RELATIVE = SOURCE_DIRECTORY_RELATIVE / "v1.json"
B101_STAGE = "historical_coverage_complete_semantic_review_complete"
CANONICAL_BACKLOG_ID = re.compile(r"B-\d{3}")
# This content certificate is stable across squash; it does not consult Git history.
B101_REGISTRY_SHA256 = "f3f05ed44d65843b3931ba7323ac015474fc1a1a6f2817d0c199b6ec232b29b9"
B101_MANIFEST_SHA256 = "334bdb575d7a66d499e2c77fda6f6cda44a94a97250d5aeda7282bd4fc6c6cbe"
B101_CENSUS_TREE_SHA256 = "f76c98cf3768044b3da97f96350e771e970bda986852b33667a630730228c5ca"
B101_CENSUS_ROOTS = ("docs/security", "reports/audits")
B101_PROPOSAL_IDS = {f"B-{number}" for number in range(171, 244)}


class CoverageError(ValueError):
    """A source was changed or its decisions no longer prove complete coverage."""


@dataclass(frozen=True)
class Finding:
    source_id: str
    document: str
    locator: str
    title: str


@dataclass(frozen=True)
class DocumentSpec:
    key: str
    path: str
    expected_count: int
    parser: Callable[[str, str], list[Finding]]


@dataclass(frozen=True)
class AuditRoot:
    path: str


@dataclass(frozen=True)
class CensusCheckpoint:
    registry: dict
    manifest: dict
    expected_report: dict


def _relative_parts(path: str, label: str) -> tuple[str, ...]:
    """Return a safe, non-empty path below the supplied repository root."""
    if not isinstance(path, str) or not path or "\0" in path:
        raise CoverageError(f"{label} must be a non-empty relative path beneath the repository root")
    if "\\" in path:
        raise CoverageError(f"{label} must be a canonical relative path beneath the repository root")
    candidate = Path(path)
    if candidate.is_absolute() or ".." in candidate.parts:
        raise CoverageError(f"{label} must be a non-empty relative path beneath the repository root")
    parts = candidate.parts
    if not parts or any(part in {"", "."} for part in parts):
        raise CoverageError(f"{label} must be a non-empty relative path beneath the repository root")
    if path != "/".join(parts):
        raise CoverageError(f"{label} must be a canonical relative path beneath the repository root")
    return parts


def _is_exact_version(value: object, expected: int) -> bool:
    """JSON booleans and floats must never alias an integer schema version."""
    return isinstance(value, int) and not isinstance(value, bool) and value == expected


def _checkpoint_contract(repo_root: Path) -> CensusCheckpoint:
    """Validate census content against a certificate independent of Git history."""
    registry_path = SOURCE_REGISTRY_RELATIVE.as_posix()
    manifest_path = MANIFEST_RELATIVE.as_posix()
    registry_bytes = _read_regular_file_beneath(repo_root, registry_path, "governed source registry")
    manifest_bytes = _read_regular_file_beneath(repo_root, manifest_path, "decision manifest")
    if hashlib.sha256(registry_bytes).hexdigest() != B101_REGISTRY_SHA256:
        raise CoverageError("governed source registry differs from the B-101 content certificate")
    if hashlib.sha256(manifest_bytes).hexdigest() != B101_MANIFEST_SHA256:
        raise CoverageError("decision manifest differs from the B-101 content certificate")
    try:
        registry, manifest = json.loads(registry_bytes), json.loads(manifest_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise CoverageError("B-101 content certificate files have malformed JSON") from exc
    if not isinstance(registry, dict) or not isinstance(manifest, dict):
        raise CoverageError("B-101 content certificate requires JSON objects")
    paths: set[str] = set()
    for root in B101_CENSUS_ROOTS:
        paths |= _regular_files_in_root(repo_root, AuditRoot(root))
    census = sorted((path, hashlib.sha256(_read_regular_file_beneath(repo_root, path, "census entry")).hexdigest()) for path in paths)
    digest = hashlib.sha256(json.dumps(census, separators=(",", ":")).encode()).hexdigest()
    if digest != B101_CENSUS_TREE_SHA256:
        raise CoverageError("governed audit tree differs from the B-101 content certificate")
    return CensusCheckpoint(registry=registry, manifest=manifest, expected_report={"documents": {"due_diligence_2026_06_15": 87, "go_live_2026_08_26": 20, "pilot_identity_2026_07_02": 3, "b373_dependabot_2026_09_09": 19}, "total": 129, "tracked": 52, "duplicate": 4, "proposed": 73, "status": "historical_coverage_complete_semantic_review_complete"})

def _open_directory_beneath(repo_root: Path, relative: str, label: str) -> Path:
    """Resolve a directory while rejecting symlinked path components."""
    parts = _relative_parts(relative, label)
    current = repo_root
    try:
        for part in parts:
            current /= part
            if current.is_symlink() or not current.is_dir():
                raise CoverageError(f"{label} must be a non-symlink directory beneath the repository root: {relative}")
    except OSError as exc:
        raise CoverageError(f"{label} cannot be read: {relative}") from exc
    return current


def _read_regular_file_beneath(repo_root: Path, relative: str, label: str) -> bytes:
    """Read a regular file without following symlinks or opening a FIFO."""
    parts = _relative_parts(relative, label)
    parent = _open_directory_beneath(repo_root, "/".join(parts[:-1]), label) if len(parts) > 1 else repo_root
    path = parent / parts[-1]
    try:
        mode = path.stat(follow_symlinks=False).st_mode
        if path.is_symlink() or not stat.S_ISREG(mode):
            raise CoverageError(f"{label} must be a regular file beneath the repository root: {relative}")
        return path.read_bytes()
    except CoverageError:
        raise
    except OSError as exc:
        raise CoverageError(f"{label} cannot be read: {relative}") from exc


def _regular_files_in_root(repo_root: Path, root: AuditRoot) -> set[str]:
    """Return every regular file in a governed root; all other entries are red."""
    root_path = _open_directory_beneath(repo_root, root.path, "governed audit root")
    files: set[str] = set()
    for directory, subdirectories, filenames in os.walk(root_path, followlinks=False):
        current = Path(directory)
        for name in subdirectories:
            path = current / name
            if path.is_symlink():
                raise CoverageError(f"symlinked entry in governed audit root: {path.relative_to(repo_root).as_posix()}")
        for name in filenames:
            path = current / name
            relative = path.relative_to(repo_root).as_posix()
            if path.is_symlink():
                raise CoverageError(f"symlinked entry in governed audit root: {relative}")
            try:
                mode = path.stat(follow_symlinks=False).st_mode
            except OSError as exc:
                raise CoverageError(f"cannot stat governed audit root entry: {relative}") from exc
            if not stat.S_ISREG(mode):
                raise CoverageError(f"governed audit root entry must be a regular file: {relative}")
            files.add(relative)
    return files


SEVERITY = r"(?:CRITICAL|HIGH|MEDIUM|LOW|INFO)"


def _section(text: str, start: str, end: str | None, document: str) -> str:
    begin = text.find(start)
    if begin < 0:
        raise CoverageError(f"{document}: required section start is missing: {start!r}")
    begin += len(start)
    finish = len(text) if end is None else text.find(end, begin)
    if finish < 0:
        raise CoverageError(f"{document}: required section end is missing: {end!r}")
    return text[begin:finish]


def _unique(findings: list[Finding], document: str) -> list[Finding]:
    ids = [finding.source_id for finding in findings]
    repeated = sorted({source_id for source_id in ids if ids.count(source_id) > 1})
    if repeated:
        raise CoverageError(f"{document}: duplicate parsed finding IDs: {', '.join(repeated)}")
    titles = [re.sub(r"\s+", " ", finding.title).strip().casefold() for finding in findings]
    duplicate_titles = sorted({title for title in titles if titles.count(title) > 1})
    if duplicate_titles:
        raise CoverageError(f"{document}: ambiguous duplicate finding titles: {duplicate_titles!r}")
    return findings


def parse_due_diligence(text: str, document: str) -> list[Finding]:
    high = _section(
        text,
        "## Confirmed CRITICAL/HIGH (survived 2-vote adversarial refutation)\n",
        "## Root-cause clusters",
        document,
    )
    medium_low = _section(text, "## MEDIUM / LOW (66)\n", None, document)
    findings: list[Finding] = []
    for number, severity, title in re.findall(
        rf"^### (\d+)\. \[({SEVERITY})\] (.+)$", high, flags=re.MULTILINE
    ):
        findings.append(Finding(f"DD-{int(number):03d}", document, f"heading {number}", title.strip()))
    if [finding.source_id for finding in findings] != [f"DD-{number:03d}" for number in range(1, 22)]:
        raise CoverageError(f"{document}: confirmed finding headings must be a contiguous DD-001..DD-021")
    for index, match in enumerate(
        re.finditer(rf"^- \[({SEVERITY})\] \(([^)]*)\) (.+)$", medium_low, flags=re.MULTILINE),
        start=22,
    ):
        findings.append(Finding(f"DD-{index:03d}", document, f"MEDIUM / LOW item {index - 21}", match.group(3).strip()))
    return _unique(findings, document)


def parse_go_live(text: str, document: str) -> list[Finding]:
    ledger = _section(text, "### 4. Findings ledger revisado", "### 5.", document)
    findings: list[Finding] = []
    for line in ledger.splitlines():
        match = re.match(r"^\| \*\*(F-\d{3})\*\* \| (.+?) \| \*?\*?P\d\*?\*? \|", line)
        if match:
            source_id, title = match.groups()
            findings.append(Finding(source_id, document, source_id, title.strip()))
    expected = [f"F-{number:03d}" for number in range(1, 21)]
    if [finding.source_id for finding in findings] != expected:
        raise CoverageError(f"{document}: ledger IDs must be exactly {expected[0]}..{expected[-1]}")
    return _unique(findings, document)


def parse_pilot_identity(text: str, document: str) -> list[Finding]:
    followups = _section(text, "## TRACKED FOLLOW-UPS (not pilot-blockers)\n", None, document)
    findings: list[Finding] = []
    for index, match in enumerate(
        re.finditer(rf"^- \*\*\[({SEVERITY})\] (.+)$", followups, flags=re.MULTILINE), start=1
    ):
        findings.append(Finding(f"PI-{index:03d}", document, f"follow-up {index}", match.group(2).strip()))
    return _unique(findings, document)


def parse_dependabot_snapshot(text: str, document: str) -> list[Finding]:
    """Parse a reviewed Dependabot snapshot as one finding per alert.

    The snapshot is an evidence source, not prose: every alert gets a stable
    locator and title so the B-101 manifest must make an explicit canonical
    disposition for each advisory rather than hiding nine alerts in one blob.
    """
    try:
        snapshot = json.loads(text)
    except json.JSONDecodeError as exc:
        raise CoverageError(f"{document}: malformed Dependabot JSON") from exc
    if not isinstance(snapshot, dict) or snapshot.get("schema_version") != 1:
        raise CoverageError(f"{document}: unsupported Dependabot snapshot schema")
    alerts = snapshot.get("alerts")
    if not isinstance(alerts, list) or not alerts:
        raise CoverageError(f"{document}: Dependabot snapshot has no alerts")
    findings: list[Finding] = []
    seen: set[int] = set()
    for alert in alerts:
        if not isinstance(alert, dict) or not isinstance(alert.get("number"), int):
            raise CoverageError(f"{document}: malformed Dependabot alert")
        number = alert["number"]
        if number in seen:
            raise CoverageError(f"{document}: duplicate Dependabot alert #{number}")
        seen.add(number)
        package, ghsa = alert.get("package"), alert.get("ghsa")
        if not isinstance(package, str) or not package or not isinstance(ghsa, str) or not ghsa:
            raise CoverageError(f"{document}: alert #{number} lacks package/GHSA identity")
        findings.append(
            Finding(
                f"DA-{number:03d}",
                document,
                f"alert {number}",
                f"Dependabot alert #{number}: {package} ({ghsa})",
            )
        )
    return _unique(findings, document)


PARSERS: dict[str, Callable[[str, str], list[Finding]]] = {
    "due_diligence": parse_due_diligence,
    "go_live": parse_go_live,
    "pilot_identity": parse_pilot_identity,
    "b028_dependabot": parse_dependabot_snapshot,
    "b373_dependabot": parse_dependabot_snapshot,
}


def source_specs(repo_root: Path) -> tuple[tuple[DocumentSpec, ...], str, CensusCheckpoint]:
    """Load a closed-world, extension-agnostic classification of every governed file."""
    source_directory = _open_directory_beneath(
        repo_root, SOURCE_DIRECTORY_RELATIVE.as_posix(), "governed audit source directory"
    )
    try:
        files: set[str] = set()
        for path in source_directory.iterdir():
            if path.is_symlink():
                raise CoverageError(f"governed audit source entry must not be a symlink: {path.name}")
            if not path.is_file() or not stat.S_ISREG(path.stat(follow_symlinks=False).st_mode):
                raise CoverageError(f"governed audit source entry must be a regular file: {path.name}")
            files.add(path.name)
    except OSError as exc:
        raise CoverageError(f"cannot read governed audit source directory {SOURCE_DIRECTORY_RELATIVE}: {exc}") from exc
    if files != {"v1.json"}:
        unexpected = sorted(files - {"v1.json"})
        missing = sorted({"v1.json"} - files)
        details = []
        if unexpected:
            details.append(f"unadmitted audit source files: {', '.join(unexpected)}")
        if missing:
            details.append(f"missing governed source files: {', '.join(missing)}")
        raise CoverageError("; ".join(details))
    try:
        raw = _read_regular_file_beneath(
            repo_root, SOURCE_REGISTRY_RELATIVE.as_posix(), "governed source registry"
        )
        registry = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise CoverageError(f"cannot parse governed source registry {SOURCE_REGISTRY_RELATIVE}: {exc}") from exc
    if (
        not isinstance(registry, dict)
        or set(registry) != {"version", "audit_roots", "excluded_audits", "sources"}
        or not _is_exact_version(registry["version"], 1)
    ):
        raise CoverageError("governed source registry must be version 1 with audit roots, exclusions, and sources")
    audit_roots = registry["audit_roots"]
    if not isinstance(audit_roots, list) or not audit_roots:
        raise CoverageError("governed source registry audit_roots must be a non-empty list")
    roots: list[AuditRoot] = []
    root_paths: set[str] = set()
    for root in audit_roots:
        if not isinstance(root, dict) or set(root) != {"path"}:
            raise CoverageError("every audit root needs only a path")
        root_path = root["path"]
        if not isinstance(root_path, str) or root_path in root_paths:
            raise CoverageError("audit root paths must be unique relative paths")
        _relative_parts(root_path, "audit root path")
        _open_directory_beneath(repo_root, root_path, "governed audit root")
        root_paths.add(root_path)
        roots.append(AuditRoot(root_path))
    sources = registry["sources"]
    if not isinstance(sources, list) or not sources:
        raise CoverageError("governed source registry sources must be a non-empty list")
    specs: list[DocumentSpec] = []
    source_ids: set[str] = set()
    paths: set[str] = set()
    for source in sources:
        if not isinstance(source, dict) or set(source) != {"source_id", "path", "expected_count", "parser"}:
            raise CoverageError("every governed source needs only source_id, path, expected_count, and parser")
        source_id, path, expected_count, parser_name = (
            source["source_id"],
            source["path"],
            source["expected_count"],
            source["parser"],
        )
        if not isinstance(source_id, str) or not source_id or source_id in source_ids:
            raise CoverageError("governed source IDs must be unique non-empty strings")
        if not isinstance(path, str) or path in paths:
            raise CoverageError("governed source paths must be unique relative paths")
        _relative_parts(path, "governed source path")
        if not isinstance(expected_count, int) or isinstance(expected_count, bool) or expected_count <= 0:
            raise CoverageError("governed source expected_count must be a positive integer")
        if not isinstance(parser_name, str) or parser_name not in PARSERS:
            raise CoverageError("governed source parser is not admitted")
        source_ids.add(source_id)
        paths.add(path)
        specs.append(DocumentSpec(source_id, path, expected_count, PARSERS[parser_name]))
    excluded_audits = registry["excluded_audits"]
    if not isinstance(excluded_audits, list):
        raise CoverageError("governed source registry excluded_audits must be a list")
    excluded_paths: set[str] = set()
    excluded_digests: dict[str, str] = {}
    for excluded in excluded_audits:
        if not isinstance(excluded, dict) or set(excluded) != {"path", "reason", "sha256"}:
            raise CoverageError("every excluded audit needs only path, reason, and sha256")
        path, reason, digest = excluded["path"], excluded["reason"], excluded["sha256"]
        if not isinstance(path, str) or path in excluded_paths or path in paths:
            raise CoverageError("excluded audit paths must be unique relative paths outside sources")
        _relative_parts(path, "excluded audit path")
        if not isinstance(reason, str) or not reason.strip():
            raise CoverageError("every excluded audit needs an explicit non-empty reason")
        if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise CoverageError("every excluded audit needs one lowercase SHA-256 digest")
        excluded_paths.add(path)
        excluded_digests[path] = digest
    candidate_paths: set[str] = set()
    for root in roots:
        candidate_paths |= _regular_files_in_root(repo_root, root)
    declared_paths = paths | excluded_paths
    if candidate_paths != declared_paths:
        unadmitted = sorted(candidate_paths - declared_paths)
        absent = sorted(declared_paths - candidate_paths)
        details = []
        if unadmitted:
            details.append(f"unadmitted audit source files: {', '.join(unadmitted)}")
        if absent:
            details.append(f"declared audit paths no longer match governed roots: {', '.join(absent)}")
        raise CoverageError("; ".join(details))
    for path in paths:
        _read_regular_file_beneath(repo_root, path, "admitted audit source")
    for path, expected_digest in excluded_digests.items():
        actual_digest = hashlib.sha256(
            _read_regular_file_beneath(repo_root, path, "excluded audit")
        ).hexdigest()
        if actual_digest != expected_digest:
            raise CoverageError(f"excluded audit content changed; reclassify it: {path}")
    checkpoint = _checkpoint_contract(repo_root)
    if registry != checkpoint.registry:
        raise CoverageError("governed source registry differs from the B-101 content certificate")
    return tuple(specs), hashlib.sha256(raw).hexdigest(), checkpoint


def parse_sources(
    repo_root: Path,
) -> tuple[tuple[DocumentSpec, ...], dict[str, list[Finding]], dict[str, str], str, CensusCheckpoint]:
    specs, registry_digest, checkpoint = source_specs(repo_root)
    all_findings: dict[str, list[Finding]] = {}
    digests: dict[str, str] = {}
    for spec in specs:
        try:
            raw = _read_regular_file_beneath(repo_root, spec.path, "admitted audit source")
            text = raw.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise CoverageError(f"cannot read {spec.path}: {exc}") from exc
        parsed = spec.parser(text, spec.path)
        if len(parsed) != spec.expected_count:
            raise CoverageError(
                f"{spec.path}: parsed {len(parsed)} findings; documented B-101 population is {spec.expected_count}"
            )
        all_findings[spec.key] = parsed
        digests[spec.key] = hashlib.sha256(raw).hexdigest()
    return specs, all_findings, digests, registry_digest, checkpoint


def _backlog_ids(repo_root: Path) -> set[str]:
    try:
        text = _read_regular_file_beneath(repo_root, "BACKLOG.md", "BACKLOG.md").decode("utf-8")
    except UnicodeDecodeError as exc:
        raise CoverageError(f"cannot read BACKLOG.md: {exc}") from exc
    canonical_ids: set[str] = set()
    # Reuse the backlog gate's fenced-block grammar. Headings in prose examples
    # or fenced text are not registered items; only `id:` keys in real
    # ```backlog blocks can satisfy a tracked decision.
    for block in BACKLOG_BLOCK_RE.finditer(text):
        matches = re.findall(r"^id:\s*(B-\d+)\s*$", block.group(1), flags=re.MULTILINE)
        if len(matches) != 1:
            continue
        if match := re.fullmatch(r"B-(\d+)", matches[0]):
            number = int(match.group(1))
            canonical = f"B-{number:03d}" if number > 0 else ""
            if matches[0] == canonical:
                canonical_ids.add(canonical)
    return canonical_ids


def _backlog_titles(repo_root: Path) -> dict[str, str]:
    """Return exact canonical item titles for semantic equivalence proofs."""
    try:
        text = _read_regular_file_beneath(repo_root, "BACKLOG.md", "BACKLOG.md").decode("utf-8")
    except UnicodeDecodeError as exc:
        raise CoverageError(f"cannot read BACKLOG.md: {exc}") from exc
    return {
        match.group(1): match.group(2).strip()
        for match in re.finditer(r"^### (B-\d{3}) — (.+)$", text, flags=re.MULTILINE)
    }


def _backlog_proposal_contracts(repo_root: Path) -> dict[str, str]:
    """Return the complete fenced contract for each reserved B-101 proposal."""
    try:
        text = _read_regular_file_beneath(repo_root, "BACKLOG.md", "BACKLOG.md").decode("utf-8")
    except UnicodeDecodeError as exc:
        raise CoverageError(f"cannot read BACKLOG.md: {exc}") from exc
    contracts: dict[str, str] = {}
    for block in BACKLOG_BLOCK_RE.finditer(text):
        match = re.search(r"^id:\s*(B-\d+)$", block.group(1), flags=re.MULTILINE)
        if match and match.group(1) in B101_PROPOSAL_IDS:
            contracts[match.group(1)] = block.group(1)
    return contracts


def _require_proposal_contract(proposal_id: str, contract: str, evidence: dict) -> None:
    required = {
        "owner": r"^owner: tl$",
        # B-101 proposals may have since graduated through their own
        # load-bearing closure gates; requiring the historical `open` value
        # makes this census stale as soon as an admitted finding is repaired.
        # A parked proposal is a truthful external/runtime hold, not a
        # completion. It must carry either the explicit owner packet below or
        # a bounded local retirement/dependency verifier; unknown statuses
        # remain invalid and cannot shrink the certified population.
        "status": r"^status: (?:open|done|parked)$",
        "dependencies": r"^dependencies: \[.*\]$",
        "source-document": rf"^source-document: .*{re.escape(evidence['source_document'])}.*$",
        "source-locator": rf"^source-locator: .*{re.escape(evidence['source_locator'])}.*$",
        "finding-title": rf"^finding-title: .*{re.escape(evidence['finding_title'])}.*$",
        "problem": rf"^problem: .*{re.escape(evidence['finding_title'])}.*$",
        "evidence": rf"^evidence: .*{re.escape(evidence['source_document'])}.*{re.escape(evidence['finding_title'])}.*$",
        "next-action": r'^next-action: ".+"$',
        "acceptance": r'^acceptance: ".+"$',
        # Preserve a real executable gate while admitting split-family
        # verifiers and closure-specific commands introduced after census.
        "verify": r"^  (?:python3 scripts/verify_[^\n]+|bash -c .+)$",
    }
    for field, pattern in required.items():
        if not re.search(pattern, contract, flags=re.MULTILINE):
            raise CoverageError(f"{proposal_id}: canonical proposal contract is missing or incomplete: {field}")
    status_match = re.search(r"^status: (open|done|parked)$", contract, flags=re.MULTILINE)
    if status_match is None:
        raise CoverageError(f"{proposal_id}: canonical proposal contract is missing or incomplete: status")
    if status_match.group(1) == "parked":
        external_packet = re.search(
            r"^verify-means:\s*\|\n  parked — .*owner packet `docs/internal/[^`]+` remains:",
            contract,
            flags=re.MULTILINE,
        )
        local_verifier = re.search(
            r"^  python3 scripts/verify_[^\n]+ --self-test\s*$",
            contract,
            flags=re.MULTILINE,
        )
        local_reason = re.search(
            r"^verify-means:\s*\|\n  parked — (?=[^\n]*(?:retir|deprecat|dependenc))[^\n]*\bB-\d{3}\b",
            contract,
            flags=re.MULTILINE | re.IGNORECASE,
        )
        if not external_packet and not (local_verifier and local_reason):
            raise CoverageError(
                f"{proposal_id}: parked proposal needs an owner packet or bounded local retirement verifier"
            )
    if any(marker in contract.casefold() for marker in ("<", "todo", "tbd", "placeholder", "generic")):
        raise CoverageError(f"{proposal_id}: canonical proposal contract contains placeholder or generic evidence")


def verify_b101_contract(repo_root: Path) -> None:
    """Require the governing item to retain its completed canonicalization stage."""
    try:
        text = _read_regular_file_beneath(repo_root, "BACKLOG.md", "BACKLOG.md").decode("utf-8")
    except UnicodeDecodeError as exc:
        raise CoverageError(f"cannot read BACKLOG.md: {exc}") from exc
    block = re.search(r"(?ms)^### B-101\b.*?(?=^### B-\d+\b|\Z)", text)
    if not block:
        raise CoverageError("B-101 governing backlog item is missing")
    required_lines = {
        "status: done": r"^status: done$",
        f"stage: {B101_STAGE}": rf"^stage: {re.escape(B101_STAGE)}$",
        "automatic verifier": r"^verify: python3 scripts/verify_audit_finding_coverage\.py --format json$",
    }
    for description, pattern in required_lines.items():
        if not re.search(pattern, block.group(0), flags=re.MULTILINE):
            raise CoverageError(f"B-101 must retain {description} until semantic review closes it")
def verify(repo_root: Path, manifest_path: Path | None = None) -> dict:
    verify_b101_contract(repo_root)
    specs, findings_by_document, source_digests, source_registry_digest, checkpoint = parse_sources(repo_root)
    manifest_path = manifest_path or repo_root / MANIFEST_RELATIVE
    try:
        manifest = json.loads(
            manifest_path.read_text(encoding="utf-8")
        )
    except json.JSONDecodeError as exc:
        raise CoverageError(f"cannot parse decision manifest {MANIFEST_RELATIVE}: {exc}") from exc
    if not isinstance(manifest, dict):
        raise CoverageError("decision manifest must be a JSON object")
    if set(manifest) != {"version", "documents", "source_registry_sha256", "source_sha256", "decisions"}:
        raise CoverageError("decision manifest has ambiguous extra fields")
    if not _is_exact_version(manifest.get("version"), 1):
        raise CoverageError("decision manifest must declare version: 1")
    expected_documents = {spec.key: spec.path for spec in specs}
    if manifest.get("documents") != expected_documents:
        raise CoverageError("decision manifest documents must equal the complete admitted source registry")
    if manifest.get("source_registry_sha256") != source_registry_digest:
        raise CoverageError("governed source registry changed; explicitly re-admit and decide its population")
    if manifest.get("source_sha256") != source_digests:
        raise CoverageError("a source audit changed; re-ingest its findings and deliberately update the manifest digest")
    decisions = manifest.get("decisions")
    if not isinstance(decisions, list):
        raise CoverageError("decision manifest decisions must be a list")
    parsed_findings = [finding for findings in findings_by_document.values() for finding in findings]
    source_ids = [finding.source_id for finding in parsed_findings]
    repeated_source_ids = sorted({source_id for source_id in source_ids if source_ids.count(source_id) > 1})
    if repeated_source_ids:
        raise CoverageError(f"admitted audit sources reuse finding IDs: {', '.join(repeated_source_ids)}")
    parsed = {finding.source_id: finding for finding in parsed_findings}
    decision_ids = [decision.get("source_id") for decision in decisions if isinstance(decision, dict)]
    if len(decision_ids) != len(decisions) or any(not isinstance(source_id, str) for source_id in decision_ids):
        raise CoverageError("every manifest decision needs one string source_id")
    duplicate_ids = sorted({source_id for source_id in decision_ids if decision_ids.count(source_id) > 1})
    if duplicate_ids:
        raise CoverageError(f"duplicate decisions are ambiguous: {', '.join(duplicate_ids)}")
    unknown = sorted(set(decision_ids) - set(parsed))
    missing = sorted(set(parsed) - set(decision_ids))
    if unknown or missing:
        details = []
        if unknown:
            details.append(f"unparsed/unknown source IDs: {', '.join(unknown)}")
        if missing:
            details.append(f"new or unassigned findings: {', '.join(missing)}")
        raise CoverageError("; ".join(details))
    backlog_ids = _backlog_ids(repo_root)
    backlog_titles = _backlog_titles(repo_root)
    backlog_proposal_contracts = _backlog_proposal_contracts(repo_root)
    tracked = duplicate = proposed = 0
    proposal_ids: set[str] = set()
    for decision in decisions:
        kind = decision.get("kind")
        source = parsed[decision["source_id"]]
        semantic = decision.get("semantic_disposition")
        if not isinstance(semantic, dict):
            raise CoverageError(f"{decision['source_id']}: semantic disposition is required")
        evidence = semantic.get("evidence")
        if not isinstance(evidence, dict) or set(evidence) != {"source_document", "source_locator", "finding_title"}:
            raise CoverageError(f"{decision['source_id']}: semantic evidence must identify the source finding exactly")
        if evidence != {"source_document": source.document, "source_locator": source.locator, "finding_title": source.title}:
            raise CoverageError(f"{decision['source_id']}: semantic evidence does not match the parsed finding")
        if kind == "tracked":
            backlog_id = decision.get("backlog_id")
            if not isinstance(backlog_id, str) or not CANONICAL_BACKLOG_ID.fullmatch(backlog_id):
                raise CoverageError(f"{decision['source_id']}: tracked decision needs canonical B-ID format")
            if backlog_id not in backlog_ids:
                raise CoverageError(f"{decision['source_id']}: tracked decision needs an existing canonical B-ID")
            if set(decision) != {"source_id", "kind", "backlog_id", "semantic_disposition"}:
                raise CoverageError(f"{decision['source_id']}: tracked decision has ambiguous fields")
            if (
                set(semantic) != {"relation", "canonical_id", "canonical_title", "evidence", "proof"}
                or semantic["relation"] != "equivalent_existing_backlog_item"
                or semantic["canonical_id"] != backlog_id
                or semantic["canonical_title"] != backlog_titles.get(backlog_id)
                or not isinstance(semantic["proof"], str)
                or not semantic["proof"].strip()
            ):
                raise CoverageError(f"{decision['source_id']}: tracked decision needs an exact canonical equivalence proof")
            tracked += 1
        elif kind == "proposed":
            proposal_id = decision.get("proposal_id")
            if not isinstance(proposal_id, str) or not CANONICAL_BACKLOG_ID.fullmatch(proposal_id):
                raise CoverageError(f"{decision['source_id']}: proposal needs canonical B-ID format")
            if proposal_id not in B101_PROPOSAL_IDS:
                raise CoverageError(f"{decision['source_id']}: proposal B-ID is outside the certified B-101 proposal set")
            if proposal_id in proposal_ids:
                raise CoverageError(f"{decision['source_id']}: proposal B-ID is duplicated in the manifest")
            if proposal_id not in backlog_ids:
                raise CoverageError(f"{decision['source_id']}: proposal B-ID needs an existing canonical backlog item")
            if set(decision) != {"source_id", "kind", "proposal_id", "proposed_title", "semantic_disposition"}:
                raise CoverageError(f"{decision['source_id']}: proposal has ambiguous fields")
            if (
                set(semantic) != {"relation", "proposal_id", "evidence", "proof"}
                or semantic["relation"] != "new_canonical_finding_proposal"
                or semantic["proposal_id"] != proposal_id
                or decision.get("proposed_title") != source.title
                or not isinstance(semantic["proof"], str)
                or not semantic["proof"].strip()
            ):
                raise CoverageError(f"{decision['source_id']}: proposal needs an exact distinct-finding proof")
            if backlog_titles.get(proposal_id) != decision["proposed_title"]:
                raise CoverageError(f"{decision['source_id']}: canonical proposal title does not match the manifest")
            contract = backlog_proposal_contracts.get(proposal_id)
            if contract is None:
                raise CoverageError(f"{decision['source_id']}: canonical proposal backlog contract is missing")
            _require_proposal_contract(proposal_id, contract, evidence)
            proposal_ids.add(proposal_id)
            proposed += 1
        elif kind == "duplicate":
            duplicate_of = decision.get("duplicate_of")
            if not isinstance(duplicate_of, str) or duplicate_of == decision["source_id"]:
                raise CoverageError(f"{decision['source_id']}: duplicate needs a different source finding target")
            target = next((item for item in decisions if item.get("source_id") == duplicate_of), None)
            if not isinstance(target, dict) or target.get("kind") not in {"tracked", "proposed"}:
                raise CoverageError(f"{decision['source_id']}: duplicate target must be another tracked or proposed finding")
            if set(decision) != {"source_id", "kind", "duplicate_of", "semantic_disposition"}:
                raise CoverageError(f"{decision['source_id']}: duplicate decision has ambiguous fields")
            if (
                set(semantic) != {"relation", "duplicate_of", "evidence", "proof"}
                or semantic["relation"] != "exact_duplicate_finding"
                or semantic["duplicate_of"] != duplicate_of
                or not isinstance(semantic["proof"], str)
                or not semantic["proof"].strip()
            ):
                raise CoverageError(f"{decision['source_id']}: duplicate decision needs an exact equivalence proof")
            duplicate += 1
        else:
            raise CoverageError(f"{decision['source_id']}: kind must be tracked, duplicate, or proposed")
    if proposal_ids != B101_PROPOSAL_IDS:
        missing_proposals = sorted(B101_PROPOSAL_IDS - proposal_ids)
        raise CoverageError(f"B-101 proposal census is incomplete: {', '.join(missing_proposals)}")
    report = {
        "documents": {key: len(items) for key, items in findings_by_document.items()},
        "total": len(parsed),
        "tracked": tracked,
        "duplicate": duplicate,
        "proposed": proposed,
        "status": "historical_coverage_complete_semantic_review_complete",
    }
    if manifest != checkpoint.manifest:
        raise CoverageError("decision manifest differs from the B-101 content certificate")
    if report != checkpoint.expected_report:
        raise CoverageError("parsed B-101 result differs from the B-101 content certificate")
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    parser.add_argument("--format", choices=("text", "json"), default="text")
    args = parser.parse_args()
    try:
        report = verify(args.repo_root.resolve())
    except CoverageError as exc:
        print(f"AUDIT-FINDING-COVERAGE: RED: {exc}", file=sys.stderr)
        return 1
    if args.format == "json":
        print(json.dumps(report, sort_keys=True))
    else:
        print(
            "AUDIT-FINDING-COVERAGE: GREEN: "
            f"{report['total']} findings ({report['documents']}); "
            f"{report['tracked']} equivalent, {report['duplicate']} exact duplicates, "
            f"{report['proposed']} new canonical proposals; "
            "historical coverage complete"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
