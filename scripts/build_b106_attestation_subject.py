#!/usr/bin/env python3
"""Build the exact B-106 evidence subject that GitHub must attest.

The subject deliberately contains the cold mint, idle, and observation times.
The final packet may carry a public hash of this file, but the hash is not the
trust boundary: the workflow signs this exact subject with GitHub Artifact
Attestations before the packet is joined.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

try:
    from scripts.join_b102_b108_evidence import mint_binding_sha256
except ModuleNotFoundError:  # direct `python scripts/...` execution
    from join_b102_b108_evidence import mint_binding_sha256


SCHEMA = "corelink.performance-evidence.b106-attestation.v1"
WORKFLOW = "perf-production-evidence"


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain an object")
    return value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--context", type=Path, required=True)
    parser.add_argument("--measurements", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    context = load(args.context)
    measurements = load(args.measurements)
    items = measurements.get("items")
    if not isinstance(items, dict) or not isinstance(items.get("B-106"), dict):
        raise ValueError("measurements missing B-106")
    value = items["B-106"]
    cold = value.get("cold_attestation")
    if not isinstance(cold, dict):
        raise ValueError("measurements missing B-106 cold attestation")
    source = {
        "kind": "github_actions_run",
        "workflow": WORKFLOW,
        "repository": context["repository"],
        "run_id": context["github_run_id"],
        "attempt": context["github_run_attempt"],
        "event": context["github_event"],
        "head_sha": context["source_head"],
        "started_at": context["github_run_started_at"],
    }
    attestation = dict(cold)
    attestation["attestation_source"] = source
    attestation["mint_response_binding_sha256"] = mint_binding_sha256(attestation)
    subject = {
        "schema": SCHEMA,
        "repository": context["repository"],
        "workflow": WORKFLOW,
        "run_id": context["github_run_id"],
        "run_attempt": context["github_run_attempt"],
        "event": context["github_event"],
        "head_sha": context["source_head"],
        "run_started_at": context["github_run_started_at"],
        "cold_attestation": attestation,
    }
    args.output.write_text(json.dumps(subject, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
