from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BACKLOG = ROOT / "BACKLOG.md"


def b006_block() -> str:
    for block in re.findall(r"```backlog\n(.*?)^```", BACKLOG.read_text(encoding="utf-8"), re.MULTILINE | re.DOTALL):
        if re.search(r"^id:\s*B-006\s*$", block, re.MULTILINE):
            return block
    raise AssertionError("B-006 backlog block is missing")


def test_b006_uses_dedicated_observability_header_and_redacts_labels() -> None:
    block = b006_block()
    assert "https://corelink-spawn-worker.gmhelmold.workers.dev/internal/v1/metrics" in block
    assert "METRICS_OBSERVABILITY_KEY" in block
    assert "X-Corelink-Internal-Auth" in block
    assert "wrong credential class" in block
    assert "Omit all labels" in block
    assert "tenant identifiers" in block
    assert "customer identifiers" in block


def test_b006_does_not_publish_a_bearer_probe_command() -> None:
    block = b006_block()
    assert "curl --fail" not in block
    assert "CORELINK_PROD_TOKEN" in block
