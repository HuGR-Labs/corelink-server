#!/usr/bin/env python3
"""Verify that the three B-101 audit inputs have one recorded decision per finding.

This is intentionally an ingestion gate, not a document-name grep.  It parses
the finding syntax used by each pinned audit, checks the live 87 + 20 + 3
population (the first report's 21 confirmed headings plus its 66-item section),
and joins every parsed source identifier to the versioned
finding-to-decision manifest.  A decision is either an extant canonical B-ID
or an explicit decline that names both its reason and the cost of declining.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable


REPO_ROOT = Path(__file__).resolve().parent.parent
MANIFEST_RELATIVE = Path("reports/audit-finding-decisions/v1.json")


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


SPECS = (
    DocumentSpec("due_diligence_2026_06_15", "docs/security/2026-06-15-launch-due-diligence-audit.md", 87, parse_due_diligence),
    DocumentSpec("go_live_2026_08_26", "reports/audits/2026-08-26-go-live-readiness.md", 20, parse_go_live),
    DocumentSpec("pilot_identity_2026_07_02", "docs/security/2026-07-02-pilot-identity-brutal-audit.md", 3, parse_pilot_identity),
)


def parse_sources(repo_root: Path) -> tuple[dict[str, list[Finding]], dict[str, str]]:
    all_findings: dict[str, list[Finding]] = {}
    digests: dict[str, str] = {}
    for spec in SPECS:
        path = repo_root / spec.path
        try:
            raw = path.read_bytes()
            text = raw.decode("utf-8")
        except (OSError, UnicodeDecodeError) as exc:
            raise CoverageError(f"cannot read {spec.path}: {exc}") from exc
        parsed = spec.parser(text, spec.path)
        if len(parsed) != spec.expected_count:
            raise CoverageError(
                f"{spec.path}: parsed {len(parsed)} findings; documented B-101 population is {spec.expected_count}"
            )
        all_findings[spec.key] = parsed
        digests[spec.key] = hashlib.sha256(raw).hexdigest()
    return all_findings, digests


def _backlog_ids(repo_root: Path) -> set[str]:
    try:
        text = (repo_root / "BACKLOG.md").read_text(encoding="utf-8")
    except OSError as exc:
        raise CoverageError(f"cannot read BACKLOG.md: {exc}") from exc
    return set(re.findall(r"^### (B-\d+)\b", text, flags=re.MULTILINE))


def verify(repo_root: Path, manifest_path: Path | None = None) -> dict:
    manifest_path = manifest_path or repo_root / MANIFEST_RELATIVE
    findings_by_document, source_digests = parse_sources(repo_root)
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise CoverageError(f"cannot parse decision manifest {manifest_path}: {exc}") from exc
    if manifest.get("version") != 1:
        raise CoverageError("decision manifest must declare version: 1")
    expected_documents = {spec.key: spec.path for spec in SPECS}
    if manifest.get("documents") != expected_documents:
        raise CoverageError("decision manifest documents must equal B-101's exact three source paths")
    if manifest.get("source_sha256") != source_digests:
        raise CoverageError("a source audit changed; re-ingest its findings and deliberately update the manifest digest")
    decisions = manifest.get("decisions")
    if not isinstance(decisions, list):
        raise CoverageError("decision manifest decisions must be a list")
    parsed = {finding.source_id: finding for findings in findings_by_document.values() for finding in findings}
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
    tracked = declined = 0
    for decision in decisions:
        kind = decision.get("kind")
        if kind == "tracked":
            backlog_id = decision.get("backlog_id")
            if not isinstance(backlog_id, str) or backlog_id not in backlog_ids:
                raise CoverageError(f"{decision['source_id']}: tracked decision needs an existing canonical B-ID")
            if set(decision) != {"source_id", "kind", "backlog_id"}:
                raise CoverageError(f"{decision['source_id']}: tracked decision has ambiguous extra fields")
            tracked += 1
        elif kind == "declined":
            reason, price = decision.get("reason"), decision.get("price")
            if not isinstance(reason, str) or not reason.strip() or not isinstance(price, str) or not price.strip():
                raise CoverageError(f"{decision['source_id']}: decline needs explicit non-empty reason and price")
            if set(decision) != {"source_id", "kind", "reason", "price"}:
                raise CoverageError(f"{decision['source_id']}: declined decision has ambiguous extra fields")
            declined += 1
        else:
            raise CoverageError(f"{decision['source_id']}: kind must be tracked or declined")
    return {
        "documents": {key: len(items) for key, items in findings_by_document.items()},
        "total": len(parsed),
        "tracked": tracked,
        "declined": declined,
        "status": "complete",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--format", choices=("text", "json"), default="text")
    args = parser.parse_args()
    try:
        report = verify(args.repo_root.resolve(), args.manifest)
    except CoverageError as exc:
        print(f"AUDIT-FINDING-COVERAGE: RED: {exc}", file=sys.stderr)
        return 1
    if args.format == "json":
        print(json.dumps(report, sort_keys=True))
    else:
        print(
            "AUDIT-FINDING-COVERAGE: GREEN: "
            f"{report['total']} findings ({report['documents']}); "
            f"{report['tracked']} tracked, {report['declined']} explicitly declined"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
