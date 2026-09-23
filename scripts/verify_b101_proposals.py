#!/usr/bin/env python3
"""Fail-closed guard for the 73 canonical B-101 proposal records.

The proposal registry is the immutable finding census; its records remain
``open`` while the implementation backlog can independently close an item.
Open implementation items use this guard's unfinished-state verifier. Done
items must name their executable inverted closure verifier and witness. Missing
or generic metadata is an instrument failure, never a green result.
"""

from __future__ import annotations

import argparse
import json
import re
import stat
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
REGISTRY = Path("reports/audit-finding-decisions/proposals-v1.json")
MANIFEST = Path("reports/audit-finding-decisions/v1.json")
BACKLOG = Path("BACKLOG.md")
PROPOSAL_IDS = {f"B-{number}" for number in range(171, 244)}
KNOWN_CANONICAL_IDS = {f"B-{number:03d}" for number in range(1, 244)}
REQUIRED_RECORD_KEYS = {
    "id", "source_id", "title", "source_document", "source_locator",
    "finding_title", "problem", "evidence", "owner", "status",
    "dependencies", "next_action", "acceptance",
}
PLACEHOLDER_MARKERS = ("<", ">", "TODO", "TBD", "placeholder", "generic")
ACTION_WORDS = ("add", "change", "remove", "replace", "implement", "update", "remediate", "refactor", "gate")
ACCEPTANCE_WORDS = ("test", "fixture", "gate", "artifact", "behavior", "evidence", "result")

# Done B-101 items may use a focal verifier shared by a contiguous closure
# wave.  Keep this allowlist explicit: a generic command or an unrecognised
# script must never turn a done backlog item green.  B-231..B-243 use one
# canonical static contract verifier (the backlog command intentionally has
# no per-ID flags), while each earlier item has its executable witness.
DONE_VERIFIERS: dict[str, tuple[str, bool]] = {
    **{
        f"B-{number}": ("verify_b171_180_closures.py", True)
        for number in range(171, 181)
    },
    **{
        f"B-{number}": ("verify_b181_b192_closures.py", True)
        for number in range(181, 193)
    },
    **{
        f"B-{number}": ("verify_b231_b243_contracts.py", False)
        for number in range(231, 244)
    },
    **{
        f"B-{number}": ("verify_b101_closures.py", True)
        for number in range(215, 231)
    },
    **{
        f"B-{number}": ("verify_b193_b214_closures.py", True)
        for number in range(193, 215)
    },
}
# B-226 graduated with its source-specific semantic gate.  It intentionally
# has no separate B-101 witness: the gate owns the four-channel and receipt
# contract, including its adversarial mutations.
DONE_VERIFIERS["B-226"] = ("verify_b226_alert_wiring.py", False)

# B-210 was truthfully retired when B-119 removed the unbound admin surface.
# Its immutable proposal record keeps the original finding contract; the done
# backlog record has a separate retirement contract verified below.
DONE_VERIFIERS["B-210"] = ("verify_b210_retirement.py", False)
B210_DONE_FIELDS = {
    "next_action": "Keep B-210 done while B-119 remains done and /admin/ops* remains absent; if a durable, securely bound approval surface is restored, re-open B-119 and B-210 together and re-audit SSR guard ordering before publishing any page.",
    "acceptance": "B-210 is retired/superseded by done B-119: /admin/ops* is absent, the B-119 census remains done, and the retirement gate fails closed on status regression or surface reintroduction; restore of a durable bound surface reopens both items.",
}


class ProposalVerificationError(ValueError):
    """A proposal registry or its corresponding backlog contract is invalid."""


def _load_json(root: Path, relative: Path, label: str) -> dict:
    path = root / relative
    if not path.is_file():
        raise ProposalVerificationError(f"missing {label}: {relative}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ProposalVerificationError(f"cannot parse {label}: {relative}: {exc}") from exc
    if not isinstance(value, dict):
        raise ProposalVerificationError(f"{label} must be a JSON object: {relative}")
    return value


def _read_source_document(root: Path, relative: str, proposal_id: str) -> str:
    """Read only an admitted regular file that resolves below ``root``.

    ``Path.is_file()`` follows symlinks.  Without the explicit resolution and
    regular-file check, a proposal could point at an external file containing
    a copied finding title and make the ingestion gate green.
    """
    path = root / relative
    if not relative or Path(relative).is_absolute() or "\\" in relative or ".." in Path(relative).parts:
        raise ProposalVerificationError(f"{proposal_id}: source_document is not a bounded relative path: {relative}")
    if not path.exists() and not path.is_symlink():
        raise ProposalVerificationError(f"{proposal_id}: admitted source is missing: {relative}")
    try:
        root_real = root.resolve(strict=True)
        resolved = path.resolve(strict=True)
        resolved.relative_to(root_real)
        mode = path.stat(follow_symlinks=False).st_mode
    except (OSError, RuntimeError, ValueError) as exc:
        raise ProposalVerificationError(
            f"{proposal_id}: source_document must resolve inside the repository root: {relative}"
        ) from exc
    if path.is_symlink() or not stat.S_ISREG(mode):
        raise ProposalVerificationError(f"{proposal_id}: admitted source is not a regular non-symlink file: {relative}")
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise ProposalVerificationError(f"{proposal_id}: admitted source is unreadable: {relative}") from exc


def _records(root: Path) -> list[dict]:
    registry = _load_json(root, REGISTRY, "B-101 proposal registry")
    if set(registry) != {"version", "proposals"} or registry.get("version") != 1:
        raise ProposalVerificationError("proposal registry must be version 1 with only proposals")
    records = registry.get("proposals")
    if not isinstance(records, list) or len(records) != len(PROPOSAL_IDS):
        raise ProposalVerificationError("proposal registry must contain exactly 73 records")
    ids: list[str] = []
    source_ids: list[str] = []
    substantive_contracts: set[tuple[str, ...]] = set()
    for record in records:
        if not isinstance(record, dict) or set(record) != REQUIRED_RECORD_KEYS:
            raise ProposalVerificationError("every proposal record must have the complete canonical contract")
        proposal_id = record.get("id")
        ids.append(proposal_id)
        if not isinstance(proposal_id, str) or proposal_id not in PROPOSAL_IDS:
            raise ProposalVerificationError(f"proposal registry has stale or unreserved id: {proposal_id!r}")
        for key in ("source_id", "title", "source_document", "source_locator", "finding_title", "problem", "evidence", "next_action", "acceptance"):
            value = record.get(key)
            if not isinstance(value, str) or not value.strip():
                raise ProposalVerificationError(f"{proposal_id}: {key} must be non-empty text")
            if any(marker.casefold() in value.casefold() for marker in PLACEHOLDER_MARKERS):
                raise ProposalVerificationError(f"{proposal_id}: {key} contains placeholder or generic evidence")
        source_id = record["source_id"].strip()
        if not re.fullmatch(r"(?:DD|F|PI)-\d{3}", source_id):
            raise ProposalVerificationError(f"{proposal_id}: source_id must be a normalized admitted finding id")
        source_ids.append(source_id.casefold())
        # Collapse whitespace before comparing contracts.  This prevents two
        # records from evading duplicate detection through line-wrap/spacing
        # noise while retaining the complete source-specific fields.
        normalize = lambda value: re.sub(r"\s+", " ", value).strip().casefold()
        fingerprint = tuple(normalize(record[key]) for key in ("source_id", "title", "problem", "evidence", "next_action", "acceptance"))
        if fingerprint in substantive_contracts:
            raise ProposalVerificationError(f"{proposal_id}: unique normalized substantive contract is duplicated")
        substantive_contracts.add(fingerprint)
        if record.get("owner") != "tl":
            raise ProposalVerificationError(f"{proposal_id}: owner must be tl unless separately reviewed as external")
        if record.get("status") != "open":
            raise ProposalVerificationError(f"{proposal_id}: unfinished proposal status must be open")
        if not isinstance(record.get("dependencies"), list) or any(
            not isinstance(dep, str) or dep not in KNOWN_CANONICAL_IDS
            for dep in record["dependencies"]
        ):
            raise ProposalVerificationError(f"{proposal_id}: dependencies must be a list of canonical B-IDs")
        action = record["next_action"].casefold()
        if record["source_document"].casefold() not in action or record["source_locator"].casefold() not in action:
            raise ProposalVerificationError(
                f"{proposal_id}: backlog contract next_action must name the exact source document and locator"
            )
        if not any(word in action for word in ACTION_WORDS):
            raise ProposalVerificationError(f"{proposal_id}: backlog contract next_action must contain a concrete action")
        acceptance = record["acceptance"].casefold()
        if record["source_id"].casefold() not in acceptance or not any(
            word in acceptance for word in ACCEPTANCE_WORDS
        ):
            raise ProposalVerificationError(f"{proposal_id}: acceptance must name source-specific evidence")
    if len(set(ids)) != len(ids) or set(ids) != PROPOSAL_IDS:
        raise ProposalVerificationError("proposal registry IDs must be unique and exactly B-171..B-243")
    if len(set(source_ids)) != len(source_ids):
        raise ProposalVerificationError("proposal registry source_id values must be unique")
    return records


def _manifest_proposals(root: Path) -> dict[str, dict]:
    manifest = _load_json(root, MANIFEST, "B-101 decision manifest")
    decisions = manifest.get("decisions")
    if not isinstance(decisions, list):
        raise ProposalVerificationError("B-101 decision manifest decisions must be a list")
    proposals: dict[str, dict] = {}
    for decision in decisions:
        if isinstance(decision, dict) and decision.get("kind") == "proposed":
            proposal_id = decision.get("proposal_id")
            if not isinstance(proposal_id, str) or proposal_id in proposals:
                raise ProposalVerificationError("B-101 decision manifest has duplicate proposal IDs")
            proposals[proposal_id] = decision
    if set(proposals) != PROPOSAL_IDS:
        raise ProposalVerificationError("B-101 decision manifest proposal set is not B-171..B-243")
    return proposals


def _backlog_section(text: str, proposal_id: str) -> str:
    match = re.search(
        rf"(?ms)^### {re.escape(proposal_id)} — (?P<title>.+?)\n(?P<body>.*?)(?=^### B-\d+\b|\Z)",
        text,
    )
    if not match:
        raise ProposalVerificationError(f"{proposal_id}: canonical backlog heading is missing")
    block = re.search(r"(?ms)^```backlog\n(?P<yaml>.*?)^```", match.group("body"))
    if not block:
        raise ProposalVerificationError(f"{proposal_id}: canonical backlog block is missing")
    return match.group("title").strip() + "\n" + block.group("yaml")


def _verify_one(root: Path, record: dict, manifest: dict[str, dict], backlog: str) -> str:
    proposal_id = record["id"]
    decision = manifest.get(proposal_id)
    if not decision or decision.get("source_id") != record["source_id"] or decision.get("proposed_title") != record["title"]:
        raise ProposalVerificationError(f"{proposal_id}: registry does not match the B-101 manifest proposal")
    evidence = decision.get("semantic_disposition", {}).get("evidence", {})
    expected_evidence = {
        "source_document": record["source_document"],
        "source_locator": record["source_locator"],
        "finding_title": record["finding_title"],
    }
    if evidence != expected_evidence:
        raise ProposalVerificationError(f"{proposal_id}: source evidence does not match the B-101 manifest")
    source_text = _read_source_document(root, record["source_document"], proposal_id)
    if source_text.count(record["finding_title"]) != 1:
        raise ProposalVerificationError(f"{proposal_id}: source finding title is absent or duplicated")
    if record["problem"] != record["finding_title"] or record["finding_title"] not in record["evidence"]:
        raise ProposalVerificationError(f"{proposal_id}: problem/evidence is not specific to the finding")
    section = _backlog_section(backlog, proposal_id)
    title, yaml_text = section.split("\n", 1)
    if title != record["title"]:
        raise ProposalVerificationError(f"{proposal_id}: backlog title does not match the specific proposal")
    status_match = re.search(r"^status: (open|done|parked)$", yaml_text, flags=re.MULTILINE)
    if status_match is None:
        raise ProposalVerificationError(f"{proposal_id}: backlog contract has no valid status")
    backlog_status = status_match.group(1)
    if backlog_status == "done":
        verifier_entry = DONE_VERIFIERS.get(proposal_id)
        if verifier_entry is None:
            raise ProposalVerificationError(
                f"{proposal_id}: done proposal has no canonical closure verifier"
            )
        verifier, witness_required = verifier_entry
        if witness_required:
            expected_verify = (
                rf"^  python3 scripts/{re.escape(verifier)} --id "
                rf"{re.escape(proposal_id)} --expect done$"
            )
        else:
            expected_verify = rf"^  python3 scripts/{re.escape(verifier)}$"
    else:
        expected_verify = rf"^  python3 scripts/verify_b101_proposals\.py --id {re.escape(proposal_id)}$"
    done_fields = B210_DONE_FIELDS if proposal_id == "B-210" and backlog_status == "done" else {}
    required_lines = {
        "id": rf"^id: {re.escape(proposal_id)}$",
        "owner": r"^owner: tl$",
        "status": rf"^status: {re.escape(backlog_status)}$",
        "dependencies": rf"^dependencies: {re.escape(json.dumps(record['dependencies'], ensure_ascii=False))}$",
        "source-document": rf"^source-document: {re.escape(json.dumps(record['source_document'], ensure_ascii=False))}$",
        "source-locator": rf"^source-locator: {re.escape(json.dumps(record['source_locator'], ensure_ascii=False))}$",
        "finding-title": rf"^finding-title: {re.escape(json.dumps(record['finding_title'], ensure_ascii=False))}$",
        "problem": rf"^problem: {re.escape(json.dumps(record['problem'], ensure_ascii=False))}$",
        "evidence": rf"^evidence: {re.escape(json.dumps(record['evidence'], ensure_ascii=False))}$",
        "next_action": rf"^next-action: {re.escape(json.dumps(done_fields.get('next_action', record['next_action']), ensure_ascii=False))}$",
        "acceptance": rf"^acceptance: {re.escape(json.dumps(done_fields.get('acceptance', record['acceptance']), ensure_ascii=False))}$",
        "verify": expected_verify,
    }
    for field, pattern in required_lines.items():
        if not re.search(pattern, yaml_text, flags=re.MULTILINE):
            raise ProposalVerificationError(f"{proposal_id}: backlog contract missing or mismatched {field}")
    if record["source_document"] not in yaml_text or record["source_locator"] not in yaml_text or record["finding_title"] not in yaml_text:
        raise ProposalVerificationError(f"{proposal_id}: backlog evidence is not specific to the admitted finding")
    if backlog_status == "done":
        verifier, witness_required = DONE_VERIFIERS[proposal_id]
        verifier_path = root / "scripts" / verifier
        if verifier_path.is_symlink() or not verifier_path.is_file():
            raise ProposalVerificationError(f"{proposal_id}: canonical closure verifier is missing: {verifier}")
        if witness_required:
            witness = root / "tests/audit/b101/closures" / f"{proposal_id}.py"
            if witness.is_symlink() or not witness.is_file():
                raise ProposalVerificationError(f"{proposal_id}: done item is missing its executable closure witness")
    return backlog_status


def verify(root: Path = ROOT, proposal_id: str | None = None) -> dict[str, int]:
    records = _records(root)
    manifest = _manifest_proposals(root)
    backlog_path = root / BACKLOG
    if not backlog_path.is_file():
        raise ProposalVerificationError("missing canonical BACKLOG.md")
    backlog = backlog_path.read_text(encoding="utf-8")
    selected = [record for record in records if proposal_id is None or record["id"] == proposal_id]
    if proposal_id is not None and not selected:
        raise ProposalVerificationError(f"unknown proposal id: {proposal_id}")
    statuses = [_verify_one(root, record, manifest, backlog) for record in selected]
    # Metadata alone is not enough to keep an OPEN item green.  Run the
    # source/configuration-bound unfinished-state verifier once per selected
    # item.  It is intentionally a separate executable so a future WP can
    # replace the OPEN witness with its own behavioural closure test without
    # editing this metadata guard into a false green.
    try:
        import verify_b101_open_state
    except ImportError as exc:
        raise ProposalVerificationError(f"open-state verifier is unavailable: {exc}") from exc
    open_ids = [record["id"] for record, status in zip(selected, statuses) if status == "open"]
    if open_ids:
        try:
            # The no-id verifier performs one bounded census and excludes the
            # implementation items already closed by their inverted witnesses.
            # Do not rescan the entire repository once per open proposal: that
            # turns a metadata check into an O(N²) review loop and can starve
            # the backlog itself.  An explicit --id remains a single-item
            # diagnostic and keeps its strict open-state semantics.
            if proposal_id is None:
                verify_b101_open_state.verify(root)
            else:
                verify_b101_open_state.verify(root, open_ids[0])
        except verify_b101_open_state.OpenStateError as exc:
            raise ProposalVerificationError(f"open-state verifier failed: {exc}") from exc
    return {
        "records": len(selected),
        "unfinished": len(open_ids),
        "done": len(selected) - len(open_ids),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--id")
    args = parser.parse_args(argv)
    try:
        report = verify(Path(args.root).resolve(), args.id)
    except (OSError, UnicodeDecodeError, ProposalVerificationError) as exc:
        print(f"B-101 proposal guard: FAIL: {exc}", file=sys.stderr)
        return 1
    print(
        f"B-101 proposal guard: PASS: checked={report['records']} "
        f"done={report['done']} open={report['unfinished']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
