#!/usr/bin/env python3
"""Fail-closed verifier for the read-only B-165 latency probe.

The probe's refusal population is useful even while customer PATs are not
available, but it is not a complete B-165 acceptance.  This verifier reports
that distinction explicitly: a refusal-only TSV exits 2 (partial/open), while
``--require-served`` additionally requires two complete authenticated served
populations, explicit canonical tenant bindings, and two distinct redacted PAT
fingerprints; it exits 0 only when all of those are present.  A TSV containing
served rows but verified without ``--require-served`` is always partial/open and
can never authorize closure.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import statistics
import sys
from pathlib import Path
from typing import Any


REFUSAL = {
    "v1": ("/v1/cas/x/y", "401"),
    "npm": ("/npm/x", "401"),
    "pip": ("/pip/simple/x", "401"),
    "v2": ("/v2/x/manifests/latest", "401"),
}
CONTROL = {"health": ("/health", "200")}
SERVING = {"served_a", "served_b"}
PAT_FINGERPRINT = re.compile(r"^sha256:[0-9a-f]{64}$")


class EvidenceError(ValueError):
    """The TSV cannot establish a complete population."""


def _percentile(values: list[float], percentile: float) -> float:
    # Nearest-rank, matching the shell probe's p90 calculation.
    index = max(1, math.ceil(percentile * len(values))) - 1
    return values[index]


def _load(path: Path, samples: int) -> dict[str, list[dict[str, Any]]]:
    if samples < 3:
        raise EvidenceError("samples must be at least 3")
    populations: dict[str, list[dict[str, Any]]] = {}
    for line_number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not raw.strip():
            continue
        fields = raw.split("\t")
        if len(fields) != 7:
            raise EvidenceError(f"line {line_number} does not have seven TSV fields")
        population, surface, request_path, sample, status, curl_rc, seconds = fields
        try:
            sample_number = int(sample)
            rc = int(curl_rc)
        except ValueError as exc:
            raise EvidenceError(f"line {line_number} has malformed integer evidence") from exc
        if sample_number < 1 or sample_number > samples or rc < 0:
            raise EvidenceError(f"line {line_number} has an out-of-range sample or return code")
        if status != "CURL_FAILED":
            try:
                elapsed = float(seconds)
            except ValueError as exc:
                raise EvidenceError(f"line {line_number} has malformed latency") from exc
            if not math.isfinite(elapsed) or elapsed < 0:
                raise EvidenceError(f"line {line_number} has invalid latency")
        else:
            elapsed = None
        populations.setdefault(surface, []).append({
            "population": population,
            "surface": surface,
            "path": request_path,
            "sample": sample_number,
            "status": status,
            "curl_rc": rc,
            "seconds": elapsed,
        })
    return populations


def _summarize(rows: list[dict[str, Any]], expected_path: str, expected_status: str, samples: int) -> dict[str, Any]:
    if len(rows) != samples:
        raise EvidenceError(f"{rows[0]['surface']}: expected {samples} rows, got {len(rows)}")
    if {row["sample"] for row in rows} != set(range(1, samples + 1)):
        raise EvidenceError(f"{rows[0]['surface']}: sample identities are incomplete or duplicated")
    if any(row["path"] != expected_path for row in rows):
        raise EvidenceError(f"{rows[0]['surface']}: path changed within the population")
    if any(row["status"] != expected_status or row["curl_rc"] != 0 for row in rows):
        raise EvidenceError(f"{rows[0]['surface']}: wrong HTTP status or transport failure")
    retained = sorted(row["seconds"] for row in rows if row["sample"] > 1)
    if len(retained) != samples - 1 or any(value is None for value in retained):
        raise EvidenceError(f"{rows[0]['surface']}: retained population is incomplete")
    return {
        "surface": rows[0]["surface"],
        "path": expected_path,
        "samples": samples,
        "retained_samples": len(retained),
        "status": expected_status,
        "median_ms": statistics.median(retained) * 1000,
        "p90_ms": _percentile(retained, 0.9) * 1000,
        "min_ms": retained[0] * 1000,
        "max_ms": retained[-1] * 1000,
    }


def _canonical_tenant(path: str) -> str:
    """Extract a tenant only from the supported CAS object path formats."""
    segments = path.split("/")
    if any(not segment for segment in segments[1:]):
        raise EvidenceError(f"served path is not canonical: {path}")
    if len(segments) == 5 and segments[1:3] == ["v1", "cas"]:
        return segments[3]
    if len(segments) == 4 and segments[1] == "cargo":
        return segments[2]
    raise EvidenceError(f"served path has no canonical tenant segment: {path}")


def _require_served_identity(
    served_summaries: list[dict[str, Any]],
    tenant_a: str | None,
    tenant_b: str | None,
    pat_fingerprint_a: str | None,
    pat_fingerprint_b: str | None,
) -> dict[str, dict[str, str]]:
    """Require distinct canonical tenant paths and redacted PAT fingerprints."""
    if not tenant_a or not tenant_b:
        raise EvidenceError(
            "served-path acceptance requires --tenant-a and --tenant-b bindings"
        )
    if tenant_a == tenant_b:
        raise EvidenceError("served-path acceptance requires distinct tenant bindings")
    by_surface = {summary["surface"]: summary for summary in served_summaries}
    path_a, path_b = by_surface["served_a"]["path"], by_surface["served_b"]["path"]
    if path_a == path_b:
        raise EvidenceError("served-path acceptance requires distinct served paths")
    if _canonical_tenant(path_a) != tenant_a or _canonical_tenant(path_b) != tenant_b:
        raise EvidenceError("served paths are not bound to their declared tenants")
    if not PAT_FINGERPRINT.fullmatch(pat_fingerprint_a or "") or not PAT_FINGERPRINT.fullmatch(
        pat_fingerprint_b or ""
    ):
        raise EvidenceError("served-path acceptance requires two redacted PAT fingerprints")
    if pat_fingerprint_a == pat_fingerprint_b:
        raise EvidenceError("served-path acceptance requires distinct PAT fingerprints")
    return {
        "served_a": {"tenant": tenant_a, "pat_fingerprint": pat_fingerprint_a},
        "served_b": {"tenant": tenant_b, "pat_fingerprint": pat_fingerprint_b},
    }


def verify(
    path: Path,
    samples: int,
    require_served: bool = False,
    tenant_a: str | None = None,
    tenant_b: str | None = None,
    pat_fingerprint_a: str | None = None,
    pat_fingerprint_b: str | None = None,
) -> dict[str, Any]:
    populations = _load(path, samples)
    summaries = []
    for surface, (request_path, status) in {**REFUSAL, **CONTROL}.items():
        if surface not in populations:
            raise EvidenceError(f"missing required population: {surface}")
        summaries.append(_summarize(populations[surface], request_path, status, samples))

    served_summaries = []
    for surface in sorted(SERVING & populations.keys()):
        served_summaries.append(_summarize(populations[surface], populations[surface][0]["path"], "200", samples))
    served_credentials = None
    if require_served:
        if {summary["surface"] for summary in served_summaries} != SERVING:
            raise EvidenceError("served-path acceptance requires both served_a and served_b populations")
        served_credentials = _require_served_identity(
            served_summaries, tenant_a, tenant_b, pat_fingerprint_a, pat_fingerprint_b
        )
    closure_allowed = require_served and len(served_summaries) == 2
    return {
        "status": "complete" if closure_allowed else "partial/open",
        "closure_allowed": closure_allowed,
        "refusal": summaries[:-1],
        "control": summaries[-1],
        "served": served_summaries,
        **({"served_credentials": served_credentials} if served_credentials else {}),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("tsv", type=Path)
    parser.add_argument("--samples", type=int, default=10)
    parser.add_argument("--require-served", action="store_true")
    parser.add_argument("--tenant-a")
    parser.add_argument("--tenant-b")
    parser.add_argument("--pat-fingerprint-a")
    parser.add_argument("--pat-fingerprint-b")
    args = parser.parse_args(argv)
    try:
        result = verify(
            args.tsv,
            args.samples,
            args.require_served,
            args.tenant_a,
            args.tenant_b,
            args.pat_fingerprint_a,
            args.pat_fingerprint_b,
        )
    except (OSError, EvidenceError) as exc:
        print(f"INDETERMINATE: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result["closure_allowed"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
