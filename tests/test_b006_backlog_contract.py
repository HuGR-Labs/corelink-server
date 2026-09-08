from __future__ import annotations

import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BACKLOG = ROOT / "BACKLOG.md"
EVIDENCE = ROOT / "docs/validation/evidence/b006-capability-claim-unserved-2026-09-08.json"


def b006_block() -> str:
    for block in re.findall(r"```backlog\n(.*?)^```", BACKLOG.read_text(encoding="utf-8"), re.MULTILINE | re.DOTALL):
        if re.search(r"^id:\s*B-006\s*$", block, re.MULTILINE):
            return block
    raise AssertionError("B-006 backlog block is missing")


def test_b006_uses_dedicated_observability_header_and_redacts_labels() -> None:
    block = b006_block()
    assert "status: done" in block
    assert "https://corelink-spawn-worker.gmhelmold.workers.dev/internal/v1/metrics" in block
    assert "METRICS_OBSERVABILITY_KEY" in block
    assert "X-Corelink-Internal-Auth" in block
    assert "wrong credential class" in block
    assert "Omit all labels" in block
    assert "tenant identifiers" in block
    assert "customer identifiers" in block
    assert "capability_claim_unserved > 0" in block


def test_b006_does_not_publish_a_bearer_probe_command() -> None:
    block = b006_block()
    assert "curl --fail" not in block
    assert "CORELINK_PROD_TOKEN" in block


def test_b006_evidence_is_authenticated_aggregate_only_and_zero() -> None:
    evidence = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    assert set(evidence) == {
        "schema",
        "source",
        "captured_at",
        "http_status",
        "authenticated",
        "aggregate_only",
        "labels_included",
        "capability_claim_unserved",
        "reopen_threshold",
    }
    assert evidence["schema"] == "corelink-b006-capability-metrics-v1"
    assert evidence["source"].endswith("/internal/v1/metrics")
    assert evidence["http_status"] == 200
    assert evidence["authenticated"] is True
    assert evidence["aggregate_only"] is True
    assert evidence["labels_included"] is False
    assert evidence["capability_claim_unserved"] == 0
    assert evidence["reopen_threshold"] == {
        "counter": "capability_claim_unserved",
        "condition": ">0",
    }
