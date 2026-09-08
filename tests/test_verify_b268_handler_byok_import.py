from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import verify_b268_handler_byok_import as verify


def _text(path: str) -> str:
    return (verify.ROOT / path).read_text(encoding="utf-8")


def _reject(path: str, old: str, new: str) -> None:
    source = _text(path)
    mutated = source.replace(old, new, 1)
    assert mutated != source
    try:
        verify.verify(overrides={path: mutated})
    except verify.VerificationError:
        return
    raise AssertionError(f"B-268 accepted mutation in {path}: {old!r}")


def test_current_contract_passes() -> None:
    verify.verify()


def test_missing_import_is_rejected() -> None:
    _reject(verify.KEYS, "use super::overview::ByokStatus;\n", "")


def test_commented_import_is_rejected() -> None:
    _reject(verify.KEYS, "use super::overview::ByokStatus;", "// use super::overview::ByokStatus;")


def test_wrong_module_is_rejected() -> None:
    _reject(verify.KEYS, "super::overview::ByokStatus", "super::keys::ByokStatus")


def test_missing_shared_definition_is_rejected() -> None:
    _reject(verify.OVERVIEW, "pub struct ByokStatus {", "struct RemovedByokStatus {")


def test_response_type_binding_is_rejected_when_removed() -> None:
    _reject(verify.KEYS, "pub byok: ByokStatus,", "pub byok: String,")
