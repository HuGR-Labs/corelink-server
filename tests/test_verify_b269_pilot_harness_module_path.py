from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import verify_b269_pilot_harness_module_path as verify


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
    raise AssertionError(f"B-269 accepted mutation in {path}: {old!r}")


def test_current_contract_passes() -> None:
    verify.verify()


def test_missing_path_is_rejected() -> None:
    _reject(verify.HARNESS, '#[path = "harness_lifecycle.rs"]\n', "")


def test_wrong_path_is_rejected() -> None:
    _reject(verify.HARNESS, 'path = "harness_lifecycle.rs"', 'path = "harness/harness_lifecycle.rs"')


def test_commented_binding_is_rejected() -> None:
    _reject(
        verify.HARNESS,
        '#[path = "harness_lifecycle.rs"]\nmod harness_lifecycle;',
        '// #[path = "harness_lifecycle.rs"]\n// mod harness_lifecycle;',
    )


def test_missing_lifecycle_method_is_rejected() -> None:
    _reject(
        verify.LIFECYCLE,
        "pub fn request_dsr_erasure(",
        "fn removed_request_dsr_erasure(",
    )
