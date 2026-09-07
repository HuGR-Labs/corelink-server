#!/usr/bin/env python3
"""Closed-population graduation gate for the D03 TL backlog lane.

This is intentionally a bounded register check.  It does not dispatch CI,
contact GitHub, or pretend that production evidence is present.  It proves
that the one DCO candidate accounts for the exact original population, that
every parked item has an executable owner packet, and that the two DONE items
still pass their local inverted guards.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.backlog_verify import parse


ROOT = Path(__file__).resolve().parents[1]
PACKET_PATH = ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json"
OWNER_POPULATION = ("B-039",)
OWNER_PACKET_FIELDS = {"owner", "status", "dependency", "action", "artifact", "command"}
OWNER_MIRROR_URL = "https://corelink-artifacts.humangr.com/tlaplus/v1.8.0/eabd140a70f49eb9305a3bd3f3df944eddf87e5a90d329789085f8953a80533a/tla2tools.jar"
OWNER_MIRROR_SHA256 = "eabd140a70f49eb9305a3bd3f3df944eddf87e5a90d329789085f8953a80533a"

# Frozen from the 42 TL/open records at the D03 starting head.  Do not derive
# this set from the candidate: doing so would make deletion look like closure.
ORIGINAL_TL_OPEN = (
    "B-044", "B-046", "B-216", "B-229", "B-028", "B-029", "B-054", "B-112",
    "B-113", "B-061", "B-063", "B-068", "B-071", "B-072", "B-074", "B-083",
    "B-098", "B-102", "B-103", "B-104", "B-105", "B-106", "B-107", "B-114",
    "B-118", "B-122", "B-125", "B-126", "B-127", "B-128", "B-129", "B-134",
    "B-135", "B-138", "B-139", "B-142", "B-152", "B-250", "B-251", "B-155",
    "B-165", "B-253",
)
EXCLUDED = frozenset(("B-061", "B-126", "B-155"))
GRADUATED = tuple(item for item in ORIGINAL_TL_OPEN if item not in EXCLUDED) + ("B-006",)
GRADUATED_SET = frozenset(GRADUATED)
ORIGINAL_SET = frozenset(ORIGINAL_TL_OPEN)
DONE_SET = frozenset(("B-074", "B-253"))
EXCLUDED_FINGERPRINTS = {
    "B-061": "d759a0591e6f867b4245f09512963f2ae10924c7b25cfad02754dc1b323657dc",
    "B-126": "88e2fe5082ad1ad9c393c633c862f947043b378c1fd36393a949b64eab34b089",
    "B-155": "46809913325793d5ad014897b0259919c37cce21d46bd864f719110bbccf316b",
}


class GraduationError(ValueError):
    pass


def _json_no_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise GraduationError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _load_packets(text: str) -> dict[str, Any]:
    try:
        value = json.loads(text, object_pairs_hook=_json_no_duplicates)
    except (json.JSONDecodeError, GraduationError) as exc:
        raise GraduationError(f"invalid graduation packet JSON: {exc}") from exc
    if not isinstance(value, dict):
        raise GraduationError("graduation packet root must be an object")
    return value


def _read(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        raise GraduationError(f"missing/non-regular graduation input: {path}")
    return path.read_text(encoding="utf-8")


def _require_string(mapping: dict[str, Any], key: str, item: str) -> str:
    value = mapping.get(key)
    if not isinstance(value, str) or not value.strip():
        raise GraduationError(f"{item}: packet field {key!r} is missing or empty")
    return value


def _check_packets(packets: dict[str, Any]) -> dict[str, dict[str, Any]]:
    if packets.get("schema_version") != 1:
        raise GraduationError("packet schema_version must be 1")
    if tuple(packets.get("original_tl_open", ())) != ORIGINAL_TL_OPEN:
        raise GraduationError("packet original population is not the frozen 42-item order")
    if tuple(packets.get("graduated_scope", ())) != GRADUATED:
        raise GraduationError("packet graduated scope is not exactly the 39-item lane plus B-006")
    entries = packets.get("packets")
    if not isinstance(entries, dict):
        raise GraduationError("packets must be an object")
    if set(entries) != GRADUATED_SET:
        missing = sorted(GRADUATED_SET - set(entries))
        extra = sorted(set(entries) - GRADUATED_SET)
        raise GraduationError(f"packet population mismatch: missing={missing}, extra={extra}")
    for item in GRADUATED:
        packet = entries[item]
        if not isinstance(packet, dict):
            raise GraduationError(f"{item}: packet must be an object")
        disposition = _require_string(packet, "disposition", item)
        if disposition not in ("DONE", "PARKED"):
            raise GraduationError(f"{item}: unclassified disposition {disposition!r}")
        _require_string(packet, "artifact", item)
        _require_string(packet, "command", item)
        _require_string(packet, "owner", item)
        _require_string(packet, "dependency", item)
        _require_string(packet, "action", item)
        if disposition == "PARKED":
            artifact = _require_string(packet, "artifact", item)
            command = _require_string(packet, "command", item)
            if artifact not in command:
                raise GraduationError(f"{item}: gate command does not create declared artifact {artifact}")
            if not re.search(r"(?:>|tee)\s*[^\n;]*" + re.escape(artifact), command):
                raise GraduationError(f"{item}: gate command has no stdout/tee capture for {artifact}")
        if disposition == "DONE":
            _require_string(packet, "evidence", item)
        else:
            if packet.get("verify_means") != "parked":
                raise GraduationError(f"{item}: parked packet must declare verify_means=parked")
    return entries


def _check_owner_packets(
    packets: dict[str, Any], records: dict[str, Any]
) -> dict[str, dict[str, Any]]:
    """Validate owner-owned records without changing the frozen TL lane."""
    population = packets.get("owner_population")
    if tuple(population or ()) != OWNER_POPULATION:
        raise GraduationError("packet owner population is not the closed B-039 scope")
    entries = packets.get("owner_packets")
    if not isinstance(entries, dict) or set(entries) != set(OWNER_POPULATION):
        raise GraduationError("owner packet population is missing or not closed")
    for item in OWNER_POPULATION:
        packet = entries[item]
        if not isinstance(packet, dict) or set(packet) != OWNER_PACKET_FIELDS:
            raise GraduationError(f"{item}: owner packet fields are missing or ambiguous")
        record = records.get(item)
        if record is None:
            raise GraduationError(f"{item}: owner packet has no BACKLOG record")
        raw = record.raw
        if raw.get("owner") != packet["owner"] or raw.get("status") != packet["status"]:
            raise GraduationError(f"{item}: owner packet status disagrees with BACKLOG")
        for field in OWNER_PACKET_FIELDS:
            _require_string(packet, field, item)
        artifact = packet["artifact"]
        command = packet["command"]
        if artifact not in command or not re.search(r">\s*" + re.escape(artifact), command):
            raise GraduationError(f"{item}: owner command does not capture declared artifact")
        if "--offline" in command or OWNER_MIRROR_URL not in command:
            raise GraduationError(f"{item}: owner command must probe the immutable mirror URL")
        if OWNER_MIRROR_SHA256 not in command or "sha256sum" not in command:
            raise GraduationError(f"{item}: owner command must verify the pinned SHA-256")
        if raw.get("action-packet") != str(PACKET_PATH.relative_to(ROOT)):
            raise GraduationError(f"{item}: BACKLOG action-packet wiring is missing")
    return entries


def verify_document(
    root: Path = ROOT,
    *,
    backlog_text: str | None = None,
    packet_text: str | None = None,
    run_guards: bool = True,
    run_gates: bool = True,
) -> dict[str, int]:
    backlog_text = _read(root / "BACKLOG.md") if backlog_text is None else backlog_text
    packet_text = _read(root / PACKET_PATH.relative_to(ROOT)) if packet_text is None else packet_text
    records = parse(backlog_text)
    ids = [record.id for record in records]
    if len(ids) != len(set(ids)):
        raise GraduationError("BACKLOG contains duplicate ids")
    by_id = {record.id: record for record in records}
    if not ORIGINAL_SET.issubset(by_id) or "B-006" not in by_id:
        raise GraduationError("BACKLOG is missing an original TL/open item or B-006")
    remaining_open = sorted(
        record.id for record in records
        if record.raw.get("owner") == "tl" and record.raw.get("status") == "open"
    )
    if remaining_open:
        raise GraduationError(f"repository still has owner tl/status open: {remaining_open}")
    packet_data = _load_packets(packet_text)
    _check_owner_packets(packet_data, by_id)
    packets = _check_packets(packet_data)
    packet_done = frozenset(item for item, packet in packets.items() if packet["disposition"] == "DONE")
    if packet_done != DONE_SET:
        raise GraduationError(f"DONE population is not exactly {sorted(DONE_SET)}: {sorted(packet_done)}")

    for item in ORIGINAL_TL_OPEN:
        record = by_id[item]
        data = record.raw
        if data.get("owner") != "tl":
            raise GraduationError(f"{item}: owner changed from tl")
        if item in EXCLUDED:
            if data.get("status") != "done":
                raise GraduationError(f"{item}: excluded lane must be done on the integrated head")
            load_bearing = "\0".join(str(data.get(key, "")) for key in ("status", "verify", "verify-means"))
            fingerprint = hashlib.sha256(load_bearing.encode()).hexdigest()
            if fingerprint != EXCLUDED_FINGERPRINTS[item]:
                raise GraduationError(f"{item}: parent load-bearing fingerprint changed")
            continue
        packet = packets[item]
        status = data.get("status")
        expected = packet["disposition"].lower()
        if status != expected:
            raise GraduationError(f"{item}: BACKLOG status {status!r} disagrees with packet {expected!r}")
        means = str(data.get("verify-means", ""))
        if expected == "done":
            if not means.lstrip().lower().startswith("done —"):
                raise GraduationError(f"{item}: DONE verify-means is not inverted to done")
            if re.search(r"(?im)^\s*(?:open|manual|parked)\b", means):
                raise GraduationError(f"{item}: DONE verify-means contains stale status language")
        else:
            if not means.lstrip().lower().startswith("parked —"):
                raise GraduationError(f"{item}: PARKED verify-means must start with parked —")
            if re.search(r"(?im)^\s*(?:open|manual)\b", means):
                raise GraduationError(f"{item}: PARKED verify-means contains stale open/manual language")

    b006 = by_id["B-006"]
    if b006.raw.get("owner") != "tl" or b006.raw.get("status") != "parked":
        raise GraduationError("B-006 must remain owner tl / parked")
    if not str(b006.raw.get("verify-means", "")).lstrip().lower().startswith("parked —"):
        raise GraduationError("B-006 verify-means must be truthful parked language")
    if packets["B-129"]["disposition"] != "PARKED" or "<10%" not in packets["B-129"]["evidence"]:
        raise GraduationError("B-129 must remain parked until production residual is <10%")

    if run_guards:
        commands = (
            ("B-074", (sys.executable, "scripts/verify_b074_money_path_auth.py", "--self-test")),
            ("B-253", (sys.executable, "-m", "pytest", "-q", "tests/test_b253_openapi_version.py")),
        )
        for item, command in commands:
            result = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=120)
            if result.returncode != 0:
                raise GraduationError(f"{item}: inverted guard failed: {result.stdout}{result.stderr}")
    if run_gates:
        _run_parked_gates(root, by_id)
    return {"original": len(ORIGINAL_TL_OPEN), "graduated": len(GRADUATED), "done": 2, "parked": len(GRADUATED) - 2}


_OFFLINE_ONLY_MARKERS = ("manual", "gh ", "gh\\n", "wrangler", "cargo ")
_OFFLINE_ONLY_IDS = frozenset((
    "B-028", "B-083", "B-098", "B-112", "B-129", "B-216", "B-229", "B-250",
))


def _run_parked_gates(root: Path, records: dict[str, Any]) -> None:
    """Run each parked gate once, replacing external actions with its offline guard."""
    for item in GRADUATED:
        if records[item].raw.get("status") != "parked":
            continue
        declared = str(records[item].raw.get("verify", ""))
        if item in _OFFLINE_ONLY_IDS or any(marker in declared for marker in _OFFLINE_ONLY_MARKERS):
            command = (sys.executable, "scripts/verify_d03_parked_gate.py", "--id", item)
        else:
            command = (sys.executable, "scripts/backlog_verify.py", "--id", item, "--format", "json")
        environment = dict(os.environ)
        environment["D03_GRADUATION_NESTED"] = "1"
        environment["D03_GRADUATION_OFFLINE"] = "1"
        try:
            result = subprocess.run(
                command, cwd=root, env=environment, capture_output=True, text=True, timeout=120
            )
        except subprocess.TimeoutExpired as exc:
            raise GraduationError(f"{item}: parked gate exceeded 120s") from exc
        if result.returncode != 0:
            output = (result.stdout + result.stderr).strip()[-1000:]
            raise GraduationError(f"{item}: parked gate failed rc={result.returncode}: {output}")


def self_test(root: Path = ROOT) -> None:
    backlog = _read(root / "BACKLOG.md")
    packets = _read(root / PACKET_PATH.relative_to(ROOT))
    cases = (
        ("status-open", backlog.replace("id: B-044\nrepo: corelink-runners\nowner: tl\nstatus: parked", "id: B-044\nrepo: corelink-runners\nowner: tl\nstatus: open", 1), packets),
        ("stale-means", backlog.replace("verify-means: |\n  parked —", "verify-means: |\n  open —", 1), packets),
        ("packet-command-removed", backlog, packets.replace('"command":"', '"command_removed":"', 1)),
        ("packet-unclassified", backlog, packets.replace('"disposition":"PARKED"', '"disposition":"UNKNOWN"', 1)),
        ("fake-done", backlog.replace("id: B-044\nrepo: corelink-runners\nowner: tl\nstatus: parked", "id: B-044\nrepo: corelink-runners\nowner: tl\nstatus: done", 1), packets.replace('"B-044": {"disposition":"PARKED"', '"B-044": {"disposition":"DONE"', 1)),
        ("artifact-capture-removed", backlog, packets.replace(" > artifacts/d03/B102-server-timing.json", "", 1)),
    )
    for name, mutated_backlog, mutated_packets in cases:
        try:
            verify_document(root, backlog_text=mutated_backlog, packet_text=mutated_packets, run_guards=False, run_gates=False)
        except GraduationError:
            continue
        raise GraduationError(f"mutation unexpectedly passed: {name}")
    for item in EXCLUDED:
        marker = f"id: {item}"
        start = backlog.index(marker)
        end = backlog.index("```", start)
        original = backlog[start:end]
        mutated = backlog[:start] + original.replace("verify-means:", "verify-means: MUTATED ", 1) + backlog[end:]
        try:
            verify_document(root, backlog_text=mutated, packet_text=packets, run_guards=False, run_gates=False)
        except GraduationError:
            continue
        raise GraduationError(f"mutation unexpectedly passed: {item} verify-means")
    owner_data = _load_packets(packets)
    for name, mutation in (
        ("owner-packet-missing", lambda data: data["owner_packets"].pop("B-039")),
        ("owner-status-missing", lambda data: data["owner_packets"]["B-039"].pop("status")),
        (
            "owner-offline-command",
            lambda data: data["owner_packets"]["B-039"].update(
                command="python3 scripts/verify_b155_owned.py --id B-039 --expect open --offline > artifacts/d03/B039-mirror-availability.json"
            ),
        ),
    ):
        mutated = copy.deepcopy(owner_data)
        mutation(mutated)
        try:
            verify_document(
                root,
                packet_text=json.dumps(mutated),
                run_guards=False,
                run_gates=False,
            )
        except GraduationError:
            continue
        raise GraduationError(f"mutation unexpectedly passed: {name}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--no-guards", action="store_true")
    parser.add_argument("--schema-only", action="store_true")
    args = parser.parse_args(argv)
    try:
        nested = bool(os.environ.get("D03_GRADUATION_NESTED"))
        result = verify_document(
            run_guards=not args.no_guards and not args.schema_only and not nested,
            run_gates=not args.schema_only and not nested,
        )
        if args.self_test:
            self_test()
        print(f"D03 graduation: PASS; original={result['original']} graduated={result['graduated']} done={result['done']} parked={result['parked']}")
        return 0
    except (GraduationError, OSError, subprocess.SubprocessError) as exc:
        print(f"D03 graduation FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
