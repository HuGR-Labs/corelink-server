#!/usr/bin/env python3
"""Verify the retained B-250 deleted-workflow evidence offline.

The canonical backlog verifier must be deterministic and bounded. This
instrument reads the repository's retained, redacted evidence summary; it does
not invoke ``gh``, inspect run history, fetch jobs/logs, or infer that the
external stale emitter was repaired. The owner-only live refresh command is
documented in the audit record and is intentionally separate from this gate.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
SNAPSHOT_PATH = ROOT / "specs/_audits/2026-09-05-b250-deleted-workflow-startup-failure.snapshot.json"
AUDIT_DOCUMENT_PATH = ROOT / "specs/_audits/2026-09-05-b250-deleted-workflow-startup-failure.md"
SNAPSHOT_SCHEMA = "b250-deleted-workflow-evidence/v1"
CANONICAL_REPO = "HuGR-Labs/corelink-server"
CANONICAL_START = "2026-09-04T00:00:00Z"
CANONICAL_END = "2026-09-06T00:00:00Z"
CANONICAL_LAST_REFRESHED = "2026-09-08T06:20:00Z"
CANONICAL_MAX_AGE_HOURS = 72
CANONICAL_COUNT = 278
CANONICAL_WORKFLOW_ID = 303501160
CANONICAL_WORKFLOW_PATH = "BuildFailed"
CANONICAL_WORKFLOW_STATE = "deleted"
OWNER = "tl"
OWNER_ACTION_STATUS = "HOLD"
AUDIT_DOCUMENT_SHA256 = "sha256:3f8e93ab9ca1e0b6edc86bca1c417455a8de2603c3bd599433312c2ca84f4650"
AUDIT_CONTENT_MARKERS = (
    "# B-250 — deleted `BuildFailed` workflow emits startup failures",
    "The workflow-scoped endpoint for workflow ID `303501160` returned **278 runs**.",
    "Every run had conclusion `startup_failure`; every run's jobs endpoint returned",
    "workflow metadata identifies path `BuildFailed` and state\n`deleted`.",
    "## Read-only refresh (2026-09-08)",
    "`closure_permitted: false`",
)

BACKLOG_PATH = ROOT / "BACKLOG.md"
BACKLOG_CONTENT_MARKERS = (
    "snapshot redigido retido em",
    "O verificador canônico é\noffline",
    "não consulta API, não percorre histórico/paginação",
    "O refresh vivo é comando separado e exclusivo do owner.",
    "O owner executa separadamente\no refresh externo autorizado para gerar ou atualizar o snapshot.",
    "status: parked",
)
VERIFIER_RE = re.compile(r"\b(?:o\s+)?(?:verificador|verifier)\b", re.IGNORECASE)
LIVE_RESOURCE_RE = re.compile(
    r"\b(?:api|endpoint|rede|network|paginação|paginacao|pagination|"
    r"histórico|historico|history)\b",
    re.IGNORECASE,
)
NEGATION_RE = re.compile(r"\b(?:não|nao|not|never|sem|without)\b", re.IGNORECASE)
OWNER_REFRESH_SNAPSHOT_RE = re.compile(
    r"\b(?:owner|dono)\b"
    r"(?=[^.?!]*\b(?:refresh|atualiza(?:ção|r|do|da)?|update)\b)"
    r"(?=[^.?!]*\b(?:externo|external)\b)"
    r"(?=[^.?!]*\b(?:executa|executar|faz|fazer|realiza|runs?|performs?)\b)"
    r"(?=[^.?!]*\b(?:gera|gerar|atualiza|atualizar|produz|produce|creates?|updates?)\b"
    r"[^.?!]*\b(?:snapshot|instantâneo|evidence\s+snapshot)\b)",
    re.IGNORECASE,
)


def _clauses(sentence: str) -> list[str]:
    """Split independent claims while keeping each resource's local polarity."""
    return [
        part.strip()
        for part in re.split(
            r"[;:]|\b(?:e|and|mas|but|porém|however)\b",
            sentence,
            flags=re.IGNORECASE,
        )
        if part.strip()
    ]


def _resource_is_negated(clause: str, start: int, end: int) -> bool:
    """Recognize an explicit negation governing a live-resource mention."""
    prefix = clause[:start]
    if re.match(r"\s*(?:não|nao|not|never|sem|without)\b", clause, re.IGNORECASE):
        return True
    prefix_words = re.findall(r"[\wÀ-ÿ]+", prefix)
    if any(NEGATION_RE.fullmatch(word) for word in prefix_words[-3:]):
        return True
    # Passive negation normally follows the resource ("API não é consultada").
    # Bound the look-behind words so a later unrelated negative clause cannot
    # launder an earlier positive resource claim.
    suffix = clause[end:]
    return bool(
        re.match(
            r"\s*(?:[\wÀ-ÿ]+\s+){0,3}(?:não|nao|not|never)\b",
            suffix,
            re.IGNORECASE,
        )
    )


def _positive_live_claim(clause: str) -> bool:
    """Return whether a clause makes any unqualified live-resource claim."""
    return any(
        not _resource_is_negated(clause, match.start(), match.end())
        for match in LIVE_RESOURCE_RE.finditer(clause)
    )


class SnapshotError(RuntimeError):
    """The retained evidence cannot establish the bounded B-250 finding."""


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise SnapshotError(f"{label} must be an object")
    return value


def _strict_object(value: Any, label: str, keys: set[str]) -> dict[str, Any]:
    obj = _object(value, label)
    unknown = sorted(set(obj) - keys)
    missing = sorted(keys - set(obj))
    if unknown:
        raise SnapshotError(f"{label} contains unknown field(s): {', '.join(unknown)}")
    if missing:
        raise SnapshotError(f"{label} is missing field(s): {', '.join(missing)}")
    return obj


def _utc_timestamp(value: Any, label: str) -> None:
    if not isinstance(value, str):
        raise SnapshotError(f"{label} must be an explicit UTC timestamp")
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise SnapshotError(f"{label} must be a parseable UTC timestamp") from exc
    if parsed.utcoffset() != dt.timedelta(0) or not value.endswith("Z"):
        raise SnapshotError(f"{label} must be an explicit UTC timestamp")


def _positive_int(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise SnapshotError(f"{label} must be a positive integer")
    return value


def _nonnegative_int(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise SnapshotError(f"{label} must be a non-negative integer")
    return value


def _sha256(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.startswith("sha256:") or len(value) != 71:
        raise SnapshotError(f"{label} must be a sha256 digest")
    try:
        int(value[7:], 16)
    except ValueError as exc:
        raise SnapshotError(f"{label} must be a lowercase sha256 digest") from exc
    if value != value.lower():
        raise SnapshotError(f"{label} must be a lowercase sha256 digest")
    return value


def _uniqueness_digest(population: dict[str, Any], window: dict[str, Any], workflow: dict[str, Any]) -> str:
    material = "\n".join(
        (
            "B-250",
            CANONICAL_REPO,
            window["start"],
            window["end_exclusive"],
            str(population["run_count"]),
            population["conclusion"],
            str(population["job_count"]),
            str(population["unique_count"]),
            str(population["duplicate_count"]),
            str(workflow["id"]),
            workflow["path"],
            workflow["state"],
        )
    ).encode("utf-8")
    return "sha256:" + hashlib.sha256(material).hexdigest()


def _audit_document_hash(path: Path) -> str:
    try:
        content = path.read_bytes()
    except OSError as exc:
        raise SnapshotError("referenced B-250 audit document could not be read") from exc
    digest = "sha256:" + hashlib.sha256(content).hexdigest()
    if digest != AUDIT_DOCUMENT_SHA256:
        raise SnapshotError("referenced B-250 audit document hash is not canonical")
    decoded = content.decode("utf-8", errors="strict")
    missing = [marker for marker in AUDIT_CONTENT_MARKERS if marker not in decoded]
    if missing:
        raise SnapshotError("referenced B-250 audit document content is incomplete")
    return digest


def verify_backlog_description(path: Path = BACKLOG_PATH) -> None:
    """Enforce the closed offline/live-refresh polarity contract in B-250."""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise SnapshotError("BACKLOG.md could not be read for B-250 claim binding") from exc
    start = text.find("### B-250 —")
    end = text.find("### B-251 —", start + 1)
    if start < 0 or end < 0:
        raise SnapshotError("BACKLOG.md B-250 section boundaries are missing")
    section = text[start:end]
    missing = [marker for marker in BACKLOG_CONTENT_MARKERS if marker not in section]
    if missing:
        raise SnapshotError("BACKLOG.md B-250 description is not the offline contract")

    # Every mention of a live resource is a claim-bearing surface. It must be
    # explicitly negated, except for an owner-only external refresh sentence
    # that generates/updates the retained snapshot. This is deliberately
    # subject-independent: a subjectless "consulta API" or mandatory endpoint
    # claim is still rejected.
    # Keep wrapped Markdown lines in one sentence, but isolate paragraph/code
    # mutations inserted after the canonical block.
    for sentence in re.split(r"(?<=[.!?])\s+|(?:\n\s*){2,}", section):
        if not LIVE_RESOURCE_RE.search(sentence):
            continue
        owner_refresh = bool(OWNER_REFRESH_SNAPSHOT_RE.search(sentence))
        for clause in _clauses(sentence):
            if not LIVE_RESOURCE_RE.search(clause):
                continue
            # The exception may authorize network access only for the owner;
            # never let it mask a positive verifier/network attribution.
            if VERIFIER_RE.search(clause) and _positive_live_claim(clause):
                raise SnapshotError(
                    "BACKLOG.md B-250 contains a positive or mandatory API/endpoint/pagination/network claim attributed to the verifier"
                )
            if owner_refresh and not VERIFIER_RE.search(clause):
                continue
            if _positive_live_claim(clause):
                raise SnapshotError(
                    "BACKLOG.md B-250 contains a positive or mandatory API/endpoint/pagination/network claim"
                )


def verify(snapshot: dict[str, Any], *, audit_document: Path = AUDIT_DOCUMENT_PATH) -> dict[str, Any]:
    """Validate the exact retained population without external I/O."""
    top = _strict_object(
        snapshot,
        "snapshot",
        {"schema", "repo", "window", "population", "workflow", "owner_action", "source"},
    )
    if top["schema"] != SNAPSHOT_SCHEMA:
        raise SnapshotError("unsupported or missing snapshot schema")

    source = _strict_object(
        top["source"],
        "source",
        {"document", "document_sha256", "last_refreshed", "max_age_hours", "read_only", "raw_api_payload_persisted"},
    )
    if source["document"] != "specs/_audits/2026-09-05-b250-deleted-workflow-startup-failure.md":
        raise SnapshotError("snapshot source document is not the retained B-250 audit")
    _sha256(source["document_sha256"], "source.document_sha256")
    if source["last_refreshed"] != CANONICAL_LAST_REFRESHED:
        raise SnapshotError("snapshot source last_refreshed is stale or changed")
    _utc_timestamp(source["last_refreshed"], "source.last_refreshed")
    if _nonnegative_int(source["max_age_hours"], "source.max_age_hours") != CANONICAL_MAX_AGE_HOURS:
        raise SnapshotError("snapshot source max-age is changed")
    refreshed = dt.datetime.fromisoformat(source["last_refreshed"].replace("Z", "+00:00"))
    observed = dt.datetime.fromisoformat(CANONICAL_END.replace("Z", "+00:00"))
    # A refresh may legitimately occur after the closed observation window.
    # Bound the retained source age in either direction relative to that window
    # instead of treating a later authenticated refresh as impossible.
    if abs((observed - refreshed).total_seconds()) > source["max_age_hours"] * 3600:
        raise SnapshotError("snapshot source evidence exceeds its declared max-age")
    if source["read_only"] is not True or source["raw_api_payload_persisted"] is not False:
        raise SnapshotError("snapshot provenance must be read-only and redacted")
    if _audit_document_hash(audit_document) != source["document_sha256"]:
        raise SnapshotError("snapshot source hash does not match the referenced audit document")

    if top["repo"] != CANONICAL_REPO:
        raise SnapshotError("snapshot repository is not canonical")
    window = _strict_object(top["window"], "window", {"start", "end_exclusive"})
    _utc_timestamp(window["start"], "window.start")
    _utc_timestamp(window["end_exclusive"], "window.end_exclusive")
    if window["start"] != CANONICAL_START or window["end_exclusive"] != CANONICAL_END:
        raise SnapshotError("snapshot window does not match the retained closed UTC window")

    population = _strict_object(
        top["population"],
        "population",
        {"run_count", "conclusion", "job_count", "unique_count", "duplicate_count", "uniqueness_digest", "run_ids", "timestamps"},
    )
    if population["run_count"] != CANONICAL_COUNT:
        raise SnapshotError(
            f"retained B-250 population count changed (expected {CANONICAL_COUNT}, got {population['run_count']})"
        )
    if population["conclusion"] != "startup_failure":
        raise SnapshotError("retained B-250 population is not uniformly startup_failure")
    if population["job_count"] != 0:
        raise SnapshotError("retained B-250 population does not prove zero jobs")
    if _nonnegative_int(population["unique_count"], "population.unique_count") != CANONICAL_COUNT:
        raise SnapshotError("retained B-250 population does not prove unique run identity")
    if _nonnegative_int(population["duplicate_count"], "population.duplicate_count") != 0:
        raise SnapshotError("retained B-250 population contains duplicate run identity")
    _sha256(population["uniqueness_digest"], "population.uniqueness_digest")
    workflow = _strict_object(top["workflow"], "workflow", {"id", "path", "state"})
    if _positive_int(workflow["id"], "workflow.id") != CANONICAL_WORKFLOW_ID:
        raise SnapshotError("retained workflow identity changed")
    if workflow["path"] != CANONICAL_WORKFLOW_PATH:
        raise SnapshotError("retained workflow path changed")
    if workflow["state"] != CANONICAL_WORKFLOW_STATE:
        raise SnapshotError("retained workflow is not deleted")
    if population["uniqueness_digest"] != _uniqueness_digest(population, window, workflow):
        raise SnapshotError("retained run identity uniqueness digest is invalid")

    # Raw run IDs/timestamps are deliberately not copied into the repository.
    # Their count and retention location are load-bearing, while the owner
    # packet remains the authority for the original redacted evidence.
    run_ids = _strict_object(
        population["run_ids"],
        "population.run_ids",
        {"count", "retained_in", "raw_values_persisted"},
    )
    timestamps = _strict_object(
        population["timestamps"],
        "population.timestamps",
        {"count", "retained_in", "raw_values_persisted"},
    )
    for label, record in (("run_ids", run_ids), ("timestamps", timestamps)):
        if record["count"] != CANONICAL_COUNT:
            raise SnapshotError(f"population.{label} count is not the closed population")
        if record["retained_in"] != "authorized Actions evidence packet":
            raise SnapshotError(f"population.{label} retention provenance is missing")
        if record["raw_values_persisted"] is not False:
            raise SnapshotError(f"population.{label} must not persist raw values")

    action = _strict_object(
        top["owner_action"],
        "owner_action",
        {"owner", "status", "closure_permitted", "next_step"},
    )
    if action["owner"] != OWNER or action["status"] != OWNER_ACTION_STATUS:
        raise SnapshotError("B-250 owner action is not an external HOLD")
    if action["closure_permitted"] is not False:
        raise SnapshotError("offline evidence must never permit B-250 closure")
    if not isinstance(action["next_step"], str) or not action["next_step"].strip():
        raise SnapshotError("B-250 owner next step is missing")

    return {
        "finding": "B-250",
        "status": OWNER_ACTION_STATUS,
        "closure_permitted": False,
        "owner": OWNER,
        "repo": CANONICAL_REPO,
        "window": {"start": CANONICAL_START, "end_exclusive": CANONICAL_END},
        "population": {
            "run_count": CANONICAL_COUNT,
            "run_ids": {"count": CANONICAL_COUNT, "retained_in": run_ids["retained_in"]},
            "timestamps": {"count": CANONICAL_COUNT, "retained_in": timestamps["retained_in"]},
            "conclusion": "startup_failure",
            "job_count": 0,
            "unique_count": CANONICAL_COUNT,
            "duplicate_count": 0,
            "uniqueness_digest": population["uniqueness_digest"],
        },
        "workflow": {
            "id": CANONICAL_WORKFLOW_ID,
            "path": CANONICAL_WORKFLOW_PATH,
            "state": CANONICAL_WORKFLOW_STATE,
        },
        "reason": (
            "The retained closed-window evidence is a separate Actions control-plane finding; "
            "the stale emitter owner must act before closure."
        ),
        "limitations": [
            "offline verification does not query current Actions state",
            "a quiet window, rerun, or unrelated green workflow cannot close B-250",
            "B-250 does not explain or cure B-152 ordinary job deaths",
        ],
    }


def verify_retained_snapshot(
    path: Path = SNAPSHOT_PATH, audit_document: Path = AUDIT_DOCUMENT_PATH
) -> dict[str, Any]:
    try:
        raw = path.read_bytes()
        snapshot = json.loads(raw)
    except OSError as exc:
        raise SnapshotError("retained B-250 snapshot could not be read") from exc
    except json.JSONDecodeError as exc:
        raise SnapshotError("retained B-250 snapshot is not valid JSON") from exc
    verify_backlog_description()
    report = verify(_object(snapshot, "snapshot"), audit_document=audit_document)
    report["snapshot"] = {
        "path": str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path),
        "sha256": "sha256:" + hashlib.sha256(raw).hexdigest(),
    }
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    try:
        report = verify_retained_snapshot()
        rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
        if args.output:
            args.output.write_text(rendered, encoding="utf-8")
        else:
            print(rendered, end="")
    except (OSError, SnapshotError) as exc:
        print(f"INDETERMINATE: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
