#!/usr/bin/env python3
"""Static checks for B-251's hosted provenance boundary."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def verify(root: Path = ROOT) -> None:
    workflow = (root / ".github/workflows/b251-d03-read-only-probe.yml").read_text()
    collector = (root / "scripts/collect_b251_provenance.py").read_text()
    operation = (root / "crates/corelink-billing/tests/b251_identity_operation.rs").read_text()
    probe = (root / "scripts/run_b251_latency_probe.py").read_text()
    for forbidden in ("d02_seed:", "d02_failure:", "d02_blob:", "d03_seed:", "d03_failure:", "d03_blob:"):
        if forbidden in workflow:
            raise ValueError(f"workflow still accepts manual identity input: {forbidden}")
    required = (
        "workflow_dispatch:", "github.event_name == 'workflow_dispatch'",
        "github.ref == 'refs/heads/main'", "github.ref_protected == true",
        "ref: ${{ github.event.pull_request.head.sha }}",
        "f88c6ca41868f6a02e78ba9f4357d3abf67da4be",
        "collect_b251_provenance.py",
        "test_b251_provenance_contract.py",
    )
    for text in required:
        if text not in workflow:
            raise ValueError(f"workflow contract missing {text}")
    for forbidden in ("id-token: write", "attestations: write", "attest-build-provenance@"):
        if forbidden in workflow:
            raise ValueError(f"B-251 workflow exceeds read-only permissions: {forbidden}")
    for text in ("cargo", "B251_OPERATION_TRANSCRIPT_HEX", "D02_COMMIT", "mutation"):
        if text not in collector:
            raise ValueError(f"collector contract missing {text}")
    for text in ("try_acquire", "InMemoryAtomicQuotaChecker", "% 900", "1_000 +", "1 +"):
        if text not in operation:
            raise ValueError(f"checker operation contract missing {text}")
    if 'record["production_latency_measured"] is not False' not in probe:
        raise ValueError("existing latency receipt must retain its production boundary")


if __name__ == "__main__":
    verify()
    print("B-251 provenance contract: PASS")
