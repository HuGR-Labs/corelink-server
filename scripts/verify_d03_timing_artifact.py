#!/usr/bin/env python3
"""Verify the offline shape of the bounded B-107/B-122/B-129 packet output.

The owner commands capture only curl metadata and Server-Timing headers.  This
checker deliberately does not contact a service: it rejects missing samples,
malformed durations, phase over-counting, and residuals that are too large.
"""

from __future__ import annotations

import argparse
import math
import re
from pathlib import Path


class TimingError(ValueError):
    pass


_SAMPLE = re.compile(
    r"^(?:ordinal|sample)=(?P<ordinal>[0-9]+)\s+status=(?P<status>[0-9]+)"
    r"(?:\s+bytes=(?P<bytes>[0-9]+))?\s+wall_s=(?P<wall>[0-9]+(?:\.[0-9]+)?)$"
)
_PHASE = re.compile(
    r"(?P<name>[a-z][a-z0-9_-]*);dur=(?P<dur>[0-9]+(?:\.[0-9]+)?)"
    r"(?:;desc=\"(?P<desc>[^\"]*)\")?"
)
_KNOWN_ORIGIN = (
    "opat", "oquota", "ostore", "oaccounting", "oargon", "opermit",
    "ortier", "oaudit", "oratelimit", "ohandler",
)
_Q_PHASES = ("qtier", "qdo", "qbatch", "qresid", "qcontrol")


def _phases(header: str) -> tuple[dict[str, float], dict[str, str]]:
    if not header.lower().startswith("server-timing:"):
        raise TimingError("missing Server-Timing header")
    payload = header.split(":", 1)[1]
    values: dict[str, float] = {}
    descriptions: dict[str, str] = {}
    matches = list(_PHASE.finditer(payload))
    if not matches:
        raise TimingError("Server-Timing header has no duration entries")
    if any(";dur=" in part and _PHASE.search(part) is None for part in payload.split(",")):
        raise TimingError("Server-Timing header contains a malformed duration")
    for match in matches:
        name = match.group("name")
        if name in values:
            raise TimingError(f"duplicate Server-Timing phase: {name}")
        value = float(match.group("dur"))
        if not math.isfinite(value):
            raise TimingError(f"non-finite duration: {name}")
        values[name] = value
        descriptions[name] = match.group("desc") or ""
    if "oother" in values:
        # During the rollout old containers can emit the compatibility alias.
        # It is safe only when it agrees exactly with canonical ohandler.
        if "ohandler" in values and not math.isclose(values["oother"], values["ohandler"], abs_tol=1e-6):
            raise TimingError("conflicting ohandler/oother alias durations")
        values.setdefault("ohandler", values["oother"])
        descriptions.setdefault("ohandler", descriptions["oother"])
    return values, descriptions


def _records(path: Path, expected: int) -> list[tuple[int, int, int | None, float, dict[str, float], dict[str, str]]]:
    current: tuple[int, int, int | None, float] | None = None
    records: list[tuple[int, int, int | None, float, dict[str, float], dict[str, str]]] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        match = _SAMPLE.match(line)
        if match:
            if current is not None:
                raise TimingError("sample has no Server-Timing header")
            current = (
                int(match.group("ordinal")),
                int(match.group("status")),
                int(match.group("bytes")) if match.group("bytes") is not None else None,
                float(match.group("wall")),
            )
            continue
        if line.lower().startswith("server-timing:"):
            if current is None:
                raise TimingError("Server-Timing header precedes its sample")
            values, descriptions = _phases(line)
            records.append((*current, values, descriptions))
            current = None
    if current is not None:
        raise TimingError("last sample has no Server-Timing header")
    if len(records) != expected:
        raise TimingError(f"expected {expected} samples, found {len(records)}")
    if [record[0] for record in records] != list(range(1, expected + 1)):
        raise TimingError("samples are not sequential 1..N")
    return records


def verify_writes(path: Path) -> None:
    for ordinal, status, payload_bytes, wall_s, values, _ in _records(path, 3):
        if status < 200 or status >= 300 or payload_bytes != 1024:
            raise TimingError("writes require 2xx status and exactly 1024 uploaded bytes")
        if wall_s <= 0:
            raise TimingError("write wall clock must be positive")
        if "ostore" not in values or "oaccounting" not in values:
            raise TimingError(f"sample {ordinal} is missing ostore/oaccounting")
        phase_sum = values["ostore"] + values["oaccounting"]
        if phase_sum > wall_s * 1000.0 + 1.0:
            raise TimingError(f"sample {ordinal} phase sum exceeds request wall clock")
    print("phase sum <= wall clock: PASS")


def verify_reads(path: Path) -> None:
    residuals: list[float] = []
    for ordinal, status, _, wall_s, values, descriptions in _records(path, 10):
        if status != 200 or wall_s <= 0:
            raise TimingError(f"sample {ordinal} is not a successful cache read")
        if any(name not in values for name in (*_Q_PHASES, "ohop", "wdb", "origin", "total")):
            raise TimingError(f"sample {ordinal} is missing a required reconciliation phase")
        if descriptions.get("qcontrol") == "unreconciled" or descriptions.get("ohop") == "unreconciled":
            raise TimingError(f"sample {ordinal} contains an unreconciled phase")
        origin_population = [name for name in _KNOWN_ORIGIN if name in values]
        if not origin_population:
            raise TimingError(f"sample {ordinal} has an empty origin phase population")
        q_sum = sum(values[name] for name in _Q_PHASES)
        if not math.isclose(q_sum, values["wdb"], abs_tol=1e-6):
            raise TimingError(f"sample {ordinal} q phase sum does not reconcile to wdb")
        origin_sum = values["ohop"] + sum(values.get(name, 0.0) for name in _KNOWN_ORIGIN)
        if not math.isclose(origin_sum, values["origin"], abs_tol=1e-6):
            raise TimingError(f"sample {ordinal} origin phase sum does not reconcile")
        if values["total"] <= 0 or values["total"] + 1e-6 < values["wdb"] + values["origin"]:
            raise TimingError(f"sample {ordinal} total is over-counted")
        residuals.append(100.0 * (values["total"] - values["wdb"] - values["origin"]) / values["total"])
    maximum = max(residuals)
    if maximum >= 10.0:
        raise TimingError(f"residual is not below 10%: {maximum:.2f}%")
    print(f"q phase sum=wdb; origin phase sum=origin; residual_max_pct=<10%:{maximum:.2f}")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--writes", action="store_true")
    mode.add_argument("--reads", action="store_true")
    parser.add_argument("artifact", type=Path)
    args = parser.parse_args()
    try:
        (verify_writes if args.writes else verify_reads)(args.artifact)
    except (OSError, TimingError) as exc:
        parser.error(str(exc))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
