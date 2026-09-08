from __future__ import annotations

import json
import re
from pathlib import Path

import pytest

from scripts.verify_b006_evidence import EvidenceError, validate_receipt
from scripts.verify_d03_graduation import GraduationError, _check_packets, _load_packets


ROOT = Path(__file__).resolve().parents[1]
BACKLOG = ROOT / "BACKLOG.md"
EVIDENCE = ROOT / "docs/validation/evidence/b006-capability-claim-unserved-2026-09-08.json"
PACKET = ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json"


def b006_block() -> str:
    for block in re.findall(r"```backlog\n(.*?)^```", BACKLOG.read_text(encoding="utf-8"), re.MULTILINE | re.DOTALL):
        if re.search(r"^id:\s*B-006\s*$", block, re.MULTILINE):
            return block
    raise AssertionError("B-006 backlog block is missing")


def test_b006_uses_dedicated_observability_header_and_redacts_labels() -> None:
    block = b006_block()
    assert "status: open" in block
    assert "https://corelink-spawn-worker.gmhelmold.workers.dev/internal/v1/metrics" in block
    assert "METRICS_OBSERVABILITY_KEY" in block
    assert "X-Corelink-Internal-Auth" in block
    assert "wrong credential class" in block
    assert "Omit all labels" in block
    assert "tenant identifiers" in block
    assert "customer identifiers" in block
    assert "capability_claim_unserved > 0" in block
    assert "HTTP 403" in block
    assert "no aggregate\n  counter" in block


def test_b006_does_not_publish_a_bearer_probe_command() -> None:
    block = b006_block()
    assert "curl --fail" not in block
    assert "CORELINK_PROD_TOKEN" in block
    packet = json.loads(PACKET.read_text(encoding="utf-8"))["packets"]["B-006"]
    command = packet["command"]
    assert packet["disposition"] == "REOPENED"
    assert "scripts/collect_b006_metrics.py" in command
    assert "scripts/verify_b006_evidence.py" in command
    assert "--auth-header X-Corelink-Internal-Auth" in command
    assert "--timeout 10" in command
    assert "--max-bytes 1048576" in command
    assert "Authorization: Bearer" not in command


def test_b006_evidence_retains_redacted_403_and_no_guessed_counter() -> None:
    evidence = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    validate_receipt(evidence)
    assert evidence["schema"] == "corelink-b006-capability-metrics-v2"
    assert evidence["http_status"] == 403
    assert evidence["authenticated"] is False
    assert evidence["aggregate_only"] is False
    assert evidence["labels_included"] is False
    assert evidence["capability_claim_unserved"] is None


def test_b006_rejects_unauthenticated_zero_and_binding_mutations() -> None:
    evidence = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    mutations = (
        {"capability_claim_unserved": 0},
        {"authenticated": True},
        {"aggregate_only": True},
        {"deployed_worker_version": "wrong-version"},
        {"deployed_source_sha": "0" * 40},
        {"labels_included": True},
    )
    for mutation in mutations:
        candidate = {**evidence, **mutation}
        with pytest.raises(EvidenceError):
            validate_receipt(candidate)


def test_b006_d03_packet_cannot_mutate_indeterminate_receipt_to_done() -> None:
    packet = _load_packets(PACKET.read_text(encoding="utf-8"))
    packet["packets"]["B-006"]["disposition"] = "DONE"
    with pytest.raises(GraduationError, match="B-006"):
        _check_packets(packet, ROOT)
