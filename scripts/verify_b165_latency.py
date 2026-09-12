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
PADDING_DECISION = "retain_padding_for_404_misses_including_unmatched_routes;_never_pad_401"


class EvidenceError(ValueError):
    """The TSV cannot establish a complete population."""


def verify_padding_policy(repo_root: Path) -> dict[str, Any]:
    """Prove the ratified 404-only padding boundary from executable sources.

    Authentication failures return before the finish stage.  Keeping the auth
    stage free of ``applyTimingPad`` prevents unauthenticated 401 traffic from
    becoming a deliberate CPU/latency amplification primitive, while the two
    supported 404 paths retain the cross-tenant enumeration defence.
    """
    def strip_ts_comments(source: str) -> str:
        output: list[str] = []
        index = 0
        quote: str | None = None
        while index < len(source):
            char = source[index]
            following = source[index + 1] if index + 1 < len(source) else ""
            if quote is not None:
                output.append(char)
                if char == "\\" and following:
                    output.append(following)
                    index += 2
                    continue
                if char == quote:
                    quote = None
                index += 1
                continue
            if char in {'"', "'", "`"}:
                quote = char
                output.append(char)
                index += 1
                continue
            if char == "/" and following == "/":
                newline = source.find("\n", index + 2)
                if newline < 0:
                    break
                output.append("\n")
                index = newline + 1
                continue
            if char == "/" and following == "*":
                closing = source.find("*/", index + 2)
                if closing < 0:
                    raise EvidenceError("unterminated TypeScript block comment")
                output.append("\n" * source[index:closing + 2].count("\n"))
                index = closing + 2
                continue
            output.append(char)
            index += 1
        return "".join(output)

    auth = strip_ts_comments((repo_root / "worker/src/index_auth_stage.ts").read_text(encoding="utf-8"))
    finish = strip_ts_comments((repo_root / "worker/src/index_finish_stage.ts").read_text(encoding="utf-8"))
    misc = strip_ts_comments((repo_root / "worker/src/index_special_misc.ts").read_text(encoding="utf-8"))
    def guarded_block(source: str, guard: str) -> str:
        start = source.find(guard)
        if start < 0:
            raise EvidenceError(f"padding guard is missing: {guard}")
        opening = source.find("{", start + len(guard))
        if opening < 0:
            raise EvidenceError(f"padding guard has no block: {guard}")
        depth = 0
        for offset in range(opening, len(source)):
            if source[offset] == "{":
                depth += 1
            elif source[offset] == "}":
                depth -= 1
                if depth == 0:
                    return source[opening + 1:offset]
        raise EvidenceError(f"padding guard block is unterminated: {guard}")

    if 'reapiError("UNAUTHORIZED", "authentication required", 401' not in auth:
        raise EvidenceError("401 authentication rejection anchor is missing")
    if any(marker in auth for marker in ("applyTimingPad", "index_auth_timing", "setTimeout(", "sleep(", "delay(")):
        raise EvidenceError("401 authentication stage must not invoke timing padding")
    forwarded_404 = guarded_block(finish, "if (doResponse.status === 404)")
    unmatched_404 = guarded_block(misc, 'if (route.routeKind === "not_found")')
    if "await applyTimingPad(" not in forwarded_404 or finish.count("applyTimingPad(") != 1:
        raise EvidenceError("forwarded 404 timing-padding boundary is missing")
    if "await applyTimingPad(" not in unmatched_404 or misc.count("applyTimingPad(") != 1:
        raise EvidenceError("unmatched-route 404 timing-padding boundary is missing")
    return {
        "decision": PADDING_DECISION,
        "401": "not_padded",
        "404": "padded",
    }


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


def _load_server_timing(path: Path, populations: dict[str, list[dict[str, Any]]], samples: int) -> list[dict[str, Any]]:
    """Validate server-side timing and its separation from transport time."""
    by_surface: dict[str, list[tuple[int, float, float]]] = {}
    for line_number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        fields = raw.split("\t")
        if len(fields) != 6:
            raise EvidenceError(f"server-timing line {line_number} does not have six TSV fields")
        surface, request_path, sample, curl_seconds, server_ms, transport_ms = fields
        if surface not in SERVING:
            raise EvidenceError(f"server-timing line {line_number} is not a served population")
        try:
            sample_number = int(sample)
            curl_value = float(curl_seconds)
            server_value = float(server_ms)
            transport_value = float(transport_ms)
        except ValueError as exc:
            raise EvidenceError(f"server-timing line {line_number} has malformed numeric evidence") from exc
        if not all(math.isfinite(value) for value in (curl_value, server_value, transport_value)):
            raise EvidenceError(f"server-timing line {line_number} has non-finite numeric evidence")
        matching = [row for row in populations.get(surface, []) if row["sample"] == sample_number]
        if len(matching) != 1 or matching[0]["path"] != request_path:
            raise EvidenceError(f"server-timing line {line_number} has no matching served sample")
        if abs((matching[0]["seconds"] or 0.0) - curl_value) > 0.000001:
            raise EvidenceError(f"server-timing line {line_number} curl total disagrees with raw evidence")
        if not (0 < server_value <= curl_value * 1000):
            raise EvidenceError(f"server-timing line {line_number} has impossible server total")
        if abs((curl_value * 1000 - server_value) - transport_value) > 0.0015:
            raise EvidenceError(f"server-timing line {line_number} has inconsistent transport residual")
        by_surface.setdefault(surface, []).append((sample_number, server_value, transport_value))
    summaries = []
    for surface in sorted(SERVING):
        rows = by_surface.get(surface, [])
        if len(rows) != samples or {row[0] for row in rows} != set(range(1, samples + 1)):
            raise EvidenceError(f"{surface}: incomplete server-timing population")
        retained_server = sorted(row[1] for row in rows if row[0] > 1)
        retained_transport = sorted(row[2] for row in rows if row[0] > 1)
        summaries.append({
            "surface": surface,
            "samples": samples,
            "retained_samples": samples - 1,
            "server_median_ms": statistics.median(retained_server),
            "server_p90_ms": _percentile(retained_server, 0.9),
            "transport_median_ms": statistics.median(retained_transport),
            "transport_p90_ms": _percentile(retained_transport, 0.9),
        })
    return summaries


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
    server_timing_path: Path | None = None,
) -> dict[str, Any]:
    padding_policy = verify_padding_policy(Path(__file__).resolve().parent.parent)
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
        if server_timing_path is None:
            raise EvidenceError("served-path acceptance requires --server-timing-tsv")
    server_timing = (
        _load_server_timing(server_timing_path, populations, samples)
        if server_timing_path is not None
        else None
    )
    closure_allowed = require_served and len(served_summaries) == 2
    return {
        "status": "complete" if closure_allowed else "partial/open",
        "closure_allowed": closure_allowed,
        "refusal": summaries[:-1],
        "control": summaries[-1],
        "served": served_summaries,
        "padding_policy": padding_policy,
        **({"server_timing": server_timing} if server_timing is not None else {}),
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
    parser.add_argument("--server-timing-tsv", type=Path)
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
            args.server_timing_tsv,
        )
    except (OSError, EvidenceError) as exc:
        print(f"INDETERMINATE: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result["closure_allowed"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
