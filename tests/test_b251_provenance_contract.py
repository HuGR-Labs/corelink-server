from __future__ import annotations

import importlib.util
from pathlib import Path
from types import SimpleNamespace

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


def test_run_operation_accepts_adapter_already_at_destination(tmp_path: Path, monkeypatch) -> None:
    operation = tmp_path / MODULE.OPERATION
    operation.parent.mkdir(parents=True)
    operation.write_text("checked-in adapter\n", encoding="utf-8")
    output = (
        f"{MODULE.COUNT}1000\n"
        f"{MODULE.FAILURES}\n"
        f"{MODULE.MARKER}00\n"
    )

    def completed_run(*_args, **_kwargs):
        return SimpleNamespace(returncode=0, stdout=output, stderr="")

    monkeypatch.setattr(MODULE.subprocess, "run", completed_run)
    identity = MODULE.run_operation(tmp_path, operation, "0123456789abcdef")

    assert set(identity) == {"seed", "failure", "blob"}
    assert operation.read_text(encoding="utf-8") == "checked-in adapter\n"


def test_deterministic_operation_seed_is_exactly_allowlisted_as_nonsecret() -> None:
    matrix = (ROOT / "scripts/validate_secrets_matrix.py").read_text(encoding="utf-8")
    shell = (ROOT / "scripts/secrets-checklist-verify.sh").read_text(encoding="utf-8")

    assert "|B251_OPERATION_SEED$" in matrix
    assert "|B251_OPERATION_SEED$" in shell
