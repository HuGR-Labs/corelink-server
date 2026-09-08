from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import verify_b263_audit_module_paths as verify


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
    raise AssertionError(f"B-263 accepted mutation in {path}: {old!r}")


def test_current_contract_passes() -> None:
    verify.verify()


def test_missing_path_is_rejected() -> None:
    _reject(verify.EVENTS, '#[path = "synthetic_data.rs"]\n', "")


def test_wrong_path_is_rejected() -> None:
    _reject(verify.EVENTS, 'path = "synthetic_data.rs"', 'path = "events/synthetic_data.rs"')


def test_commented_binding_is_rejected() -> None:
    _reject(
        verify.EVENTS,
        '#[path = "synthetic_data.rs"]\nmod synthetic_data;',
        '// #[path = "synthetic_data.rs"]\n// mod synthetic_data;',
    )


def test_missing_export_is_rejected() -> None:
    _reject(verify.EVENTS, "pub use synthetic_data::synthetic_data_for;", "")


def test_missing_implementation_is_rejected() -> None:
    _reject(verify.SYNTHETIC, "pub fn synthetic_data_for(", "fn removed_synthetic_data_for(")


def test_missing_manifest_dependency_is_rejected() -> None:
    _reject(verify.MANIFEST, 'corelink-gc = { path = "crates/corelink-gc" }', "")


def test_missing_lock_dependency_is_rejected() -> None:
    _reject(verify.LOCKFILE, ' "corelink-gc",\n', "")
