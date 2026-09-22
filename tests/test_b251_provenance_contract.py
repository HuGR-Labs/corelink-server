from __future__ import annotations

import importlib.util
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "collect_b251_provenance", ROOT / "scripts/collect_b251_provenance.py"
)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def test_static_contract() -> None:
    spec = importlib.util.spec_from_file_location(
        "verify_b251_provenance_contract", ROOT / "scripts/verify_b251_provenance_contract.py"
    )
    assert spec is not None and spec.loader is not None
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    verifier.verify(ROOT)


def test_mutated_identity_is_rejected_without_values() -> None:
    original = {"seed": "a" * 64, "failure": "b" * 64, "blob": "c" * 64}
    mutated = {**original, "blob": "d" * 64}
    with pytest.raises(MODULE.CollectionError, match="blob") as raised:
        MODULE.compare(original, mutated)
    assert "c" * 64 not in str(raised.value)
    assert "d" * 64 not in str(raised.value)


def test_d02_revision_is_an_immutable_producer_contract() -> None:
    assert MODULE.D02_COMMIT == "f88c6ca41868f6a02e78ba9f4357d3abf67da4be"
    assert len(MODULE.D02_COMMIT) == 40
